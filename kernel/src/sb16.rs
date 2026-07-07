//! Sound Blaster 16 driver with an additive "organ" synthesizer.
//!
//! The card plays a small DMA ring buffer in auto-init mode and raises
//! IRQ 5 every time half of it has been consumed. The interrupt handler
//! then synthesizes the next half: for every active voice it sums a few
//! sine harmonics (like the drawbars of a Hammond organ), all in fixed
//! point integer math -- this kernel targets soft-float.

use crate::memory::BootInfoFrameAllocator;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::{Context, Poll};
use futures_util::task::AtomicWaker;
use spinning_top::Spinlock;
use x86_64::VirtAddr;
use x86_64::instructions::port::Port;
use x86_64::structures::paging::{FrameAllocator, PhysFrame};

// --- hardware constants (QEMU defaults: -device sb16) ---
const DSP_BASE: u16 = 0x220;
const DSP_RESET: u16 = DSP_BASE + 0x6;
const DSP_READ: u16 = DSP_BASE + 0xA;
const DSP_WRITE: u16 = DSP_BASE + 0xC; // also: write-busy status (bit 7)
const DSP_READ_STATUS: u16 = DSP_BASE + 0xE; // bit 7: data available
const DSP_ACK_16BIT: u16 = DSP_BASE + 0xF; // reading acks a 16-bit-DMA IRQ

pub const SAMPLE_RATE: u32 = 22_050;
/// Whole DMA ring buffer in 16-bit samples; the card IRQs per half.
const BUFFER_SAMPLES: usize = 4096;
const HALF_SAMPLES: usize = BUFFER_SAMPLES / 2;

const MAX_VOICES: usize = 8;

/// Drawbar-style harmonic mix: (multiple of the fundamental, amplitude).
/// Roughly the 8', 4', 2 2/3' and 2' registers of an organ.
const HARMONICS: [(u32, i32); 4] = [(1, 32), (2, 16), (3, 8), (4, 4)];

static SB16: Spinlock<Option<Sb16>> = Spinlock::new(None);

/// Bit mask of buffer halves that finished playing and need a refill.
/// Set by the IRQ handler, consumed by the synth task.
static REFILL_MASK: AtomicU8 = AtomicU8::new(0);
/// Which half the card is currently playing (toggles on every IRQ).
static PLAYING_HALF: AtomicU8 = AtomicU8::new(1);
static SYNTH_WAKER: AtomicWaker = AtomicWaker::new();

struct Sb16 {
    /// The DMA ring buffer, accessed through the physical memory mapping.
    buffer: &'static mut [i16],
    voices: [Option<Voice>; MAX_VOICES],
}

#[derive(Clone, Copy)]
struct Voice {
    /// Phase accumulator: the full u32 range is one period.
    phase: u32,
    /// Phase increment per sample, i.e. the frequency.
    step: u32,
}

// --- DSP helpers ---

fn dsp_write(value: u8) -> Result<(), &'static str> {
    let mut status = Port::<u8>::new(DSP_WRITE);
    for _ in 0..100_000 {
        if unsafe { status.read() } & 0x80 == 0 {
            unsafe { Port::<u8>::new(DSP_WRITE).write(value) };
            return Ok(());
        }
    }
    Err("DSP write timeout")
}

fn dsp_read() -> Result<u8, &'static str> {
    let mut status = Port::<u8>::new(DSP_READ_STATUS);
    for _ in 0..100_000 {
        if unsafe { status.read() } & 0x80 != 0 {
            return Ok(unsafe { Port::<u8>::new(DSP_READ).read() });
        }
    }
    Err("DSP read timeout")
}

fn dsp_reset() -> Result<(), &'static str> {
    unsafe {
        Port::<u8>::new(DSP_RESET).write(1u8);
        // ~3 microseconds; unused-port reads are the classic delay
        for _ in 0..16 {
            Port::<u8>::new(0x80).read();
        }
        Port::<u8>::new(DSP_RESET).write(0u8);
    }
    if dsp_read()? == 0xAA {
        Ok(())
    } else {
        Err("DSP did not answer 0xAA")
    }
}

// --- ISA DMA controller (8237), 16-bit channel 5 ---

/// Program DMA channel 5 for an auto-init playback loop over the buffer.
unsafe fn setup_dma(phys_addr: u64) {
    let word_addr = (phys_addr >> 1) as u16; // 16-bit channels count in words
    let words = BUFFER_SAMPLES as u16;
    unsafe {
        Port::<u8>::new(0xD4).write(0x05u8); // mask channel 5 (ch 1 of 2nd ctrl)
        Port::<u8>::new(0xD8).write(0u8); // clear byte flip-flop
        Port::<u8>::new(0xD6).write(0x59u8); // single, auto-init, read (mem->card), ch 1
        let mut addr = Port::<u8>::new(0xC4);
        addr.write(word_addr as u8);
        addr.write((word_addr >> 8) as u8);
        let mut count = Port::<u8>::new(0xC6);
        count.write((words - 1) as u8);
        count.write(((words - 1) >> 8) as u8);
        Port::<u8>::new(0x8B).write((phys_addr >> 16) as u8); // page register
        Port::<u8>::new(0xD4).write(0x01u8); // unmask channel 5
    }
}

/// Allocate physically contiguous frames for the DMA buffer. ISA DMA can
/// only reach the first 16 MiB and must not cross a 128 KiB boundary.
fn alloc_dma_frames(frame_allocator: &mut BootInfoFrameAllocator) -> Option<u64> {
    let frames_needed = (BUFFER_SAMPLES * 2).div_ceil(4096);
    let mut start: Option<PhysFrame> = None;
    let mut have = 0;
    for _ in 0..4096 {
        let frame = frame_allocator.allocate_frame()?;
        let addr = frame.start_address().as_u64();
        let contiguous = match start {
            Some(s) => addr == s.start_address().as_u64() + have as u64 * 4096,
            None => false,
        };
        if !contiguous {
            start = Some(frame);
            have = 0;
        }
        have += 1;
        if have == frames_needed {
            let base = start.unwrap().start_address().as_u64();
            let end = base + (BUFFER_SAMPLES * 2) as u64 - 1;
            let ok = end < 16 * 1024 * 1024 && (base >> 17) == (end >> 17);
            if ok {
                return Some(base);
            }
            // unsuitable run (boundary / too high): start over
            start = None;
            have = 0;
        }
    }
    None
}

/// Detect the card, set up the DMA loop and start silent playback.
pub fn init(
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_mem_offset: VirtAddr,
) -> Result<(), &'static str> {
    dsp_reset()?;

    let phys = alloc_dma_frames(frame_allocator).ok_or("no DMA-capable memory")?;
    let virt = phys_mem_offset + phys;
    let buffer =
        unsafe { core::slice::from_raw_parts_mut(virt.as_mut_ptr::<i16>(), BUFFER_SAMPLES) };
    buffer.fill(0);

    unsafe { setup_dma(phys) };

    // output sample rate (0x41 takes the high byte first)
    dsp_write(0x41)?;
    dsp_write((SAMPLE_RATE >> 8) as u8)?;
    dsp_write(SAMPLE_RATE as u8)?;

    // 16-bit auto-init playback with FIFO, signed mono; the block size
    // (half the ring) sets the IRQ pace
    let block = (HALF_SAMPLES - 1) as u16;
    dsp_write(0xB6)?;
    dsp_write(0x10)?;
    dsp_write(block as u8)?;
    dsp_write((block >> 8) as u8)?;

    *SB16.lock() = Some(Sb16 {
        buffer,
        voices: [None; MAX_VOICES],
    });
    Ok(())
}

pub fn is_available() -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| SB16.lock().is_some())
}

// --- the synthesizer ---

fn freq_to_step(freq_hz: u32) -> u32 {
    ((freq_hz as u64) * (1u64 << 32) / SAMPLE_RATE as u64) as u32
}

/// Start a voice. Polyphonic: up to MAX_VOICES notes sound at once.
pub fn note_on(freq_hz: u32) {
    let step = freq_to_step(freq_hz);
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(sb) = SB16.lock().as_mut() {
            // retrigger if the note is already sounding
            if let Some(voice) = sb.voices.iter_mut().flatten().find(|v| v.step == step) {
                voice.phase = 0;
                return;
            }
            if let Some(slot) = sb.voices.iter_mut().find(|v| v.is_none()) {
                *slot = Some(Voice { phase: 0, step });
            }
        }
    });
}

pub fn note_off(freq_hz: u32) {
    let step = freq_to_step(freq_hz);
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(sb) = SB16.lock().as_mut() {
            for slot in sb.voices.iter_mut() {
                if matches!(slot, Some(v) if v.step == step) {
                    *slot = None;
                }
            }
        }
    });
}

pub fn all_notes_off() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(sb) = SB16.lock().as_mut() {
            sb.voices = [None; MAX_VOICES];
        }
    });
}

/// Additive synthesis of one buffer half.
fn synthesize(voices: &mut [Option<Voice>], out: &mut [i16]) {
    for sample in out.iter_mut() {
        let mut acc: i32 = 0;
        for voice in voices.iter_mut().flatten() {
            let phase = voice.phase;
            voice.phase = voice.phase.wrapping_add(voice.step);
            for (multiple, amplitude) in HARMONICS {
                // top 8 bits of the (harmonic) phase index the sine table
                let index = (phase.wrapping_mul(multiple) >> 24) as usize;
                acc += SINE[index] as i32 * amplitude;
            }
        }
        // headroom for 8 voices * 60 amplitude units
        *sample = (acc >> 9).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
}

/// Called from the IRQ 5 handler. Deliberately minimal: synthesis in
/// interrupt context starved the rest of the system (keyboard input
/// stopped being delivered while chords were playing), so the handler
/// only records which half finished and wakes the synth task.
pub(crate) fn handle_irq() {
    let finished = PLAYING_HALF.fetch_xor(1, Ordering::Relaxed);
    REFILL_MASK.fetch_or(1 << finished, Ordering::Release);
    SYNTH_WAKER.wake();

    // ack the 16-bit DMA interrupt on the card
    unsafe {
        Port::<u8>::new(DSP_ACK_16BIT).read();
    }
}

/// Future that resolves with the mask of halves needing a refill.
struct Refill;

impl Future for Refill {
    type Output = u8;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<u8> {
        let mask = REFILL_MASK.swap(0, Ordering::Acquire);
        if mask != 0 {
            return Poll::Ready(mask);
        }
        SYNTH_WAKER.register(cx.waker());
        // re-check: an IRQ could have fired between swap and register
        let mask = REFILL_MASK.swap(0, Ordering::Acquire);
        if mask != 0 {
            Poll::Ready(mask)
        } else {
            Poll::Pending
        }
    }
}

/// The synth task: refills finished buffer halves outside of interrupt
/// context, so long synthesis can never block interrupt delivery.
pub async fn run_synth() {
    if !is_available() {
        return;
    }
    loop {
        let mask = Refill.await;
        for half in 0..2u8 {
            if mask & (1 << half) == 0 {
                continue;
            }
            let mut guard = SB16.lock();
            if let Some(sb) = guard.as_mut() {
                let start = half as usize * HALF_SAMPLES;
                let Sb16 { buffer, voices } = sb;
                synthesize(voices, &mut buffer[start..start + HALF_SAMPLES]);
            }
        }
    }
}

/// One period of a sine wave, amplitude i16::MAX.
static SINE: [i16; 256] = [
    0, 804, 1608, 2410, 3212, 4011, 4808, 5602,
    6393, 7179, 7962, 8739, 9512, 10278, 11039, 11793,
    12539, 13279, 14010, 14732, 15446, 16151, 16846, 17530,
    18204, 18868, 19519, 20159, 20787, 21403, 22005, 22594,
    23170, 23731, 24279, 24811, 25329, 25832, 26319, 26790,
    27245, 27683, 28105, 28510, 28898, 29268, 29621, 29956,
    30273, 30571, 30852, 31113, 31356, 31580, 31785, 31971,
    32137, 32285, 32412, 32521, 32609, 32678, 32728, 32757,
    32767, 32757, 32728, 32678, 32609, 32521, 32412, 32285,
    32137, 31971, 31785, 31580, 31356, 31113, 30852, 30571,
    30273, 29956, 29621, 29268, 28898, 28510, 28105, 27683,
    27245, 26790, 26319, 25832, 25329, 24811, 24279, 23731,
    23170, 22594, 22005, 21403, 20787, 20159, 19519, 18868,
    18204, 17530, 16846, 16151, 15446, 14732, 14010, 13279,
    12539, 11793, 11039, 10278, 9512, 8739, 7962, 7179,
    6393, 5602, 4808, 4011, 3212, 2410, 1608, 804,
    0, -804, -1608, -2410, -3212, -4011, -4808, -5602,
    -6393, -7179, -7962, -8739, -9512, -10278, -11039, -11793,
    -12539, -13279, -14010, -14732, -15446, -16151, -16846, -17530,
    -18204, -18868, -19519, -20159, -20787, -21403, -22005, -22594,
    -23170, -23731, -24279, -24811, -25329, -25832, -26319, -26790,
    -27245, -27683, -28105, -28510, -28898, -29268, -29621, -29956,
    -30273, -30571, -30852, -31113, -31356, -31580, -31785, -31971,
    -32137, -32285, -32412, -32521, -32609, -32678, -32728, -32757,
    -32767, -32757, -32728, -32678, -32609, -32521, -32412, -32285,
    -32137, -31971, -31785, -31580, -31356, -31113, -30852, -30571,
    -30273, -29956, -29621, -29268, -28898, -28510, -28105, -27683,
    -27245, -26790, -26319, -25832, -25329, -24811, -24279, -23731,
    -23170, -22594, -22005, -21403, -20787, -20159, -19519, -18868,
    -18204, -17530, -16846, -16151, -15446, -14732, -14010, -13279,
    -12539, -11793, -11039, -10278, -9512, -8739, -7962, -7179,
    -6393, -5602, -4808, -4011, -3212, -2410, -1608, -804,
];
