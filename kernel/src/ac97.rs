//! AC'97 audio driver (Intel 82801AA) with an additive "organ"
//! synthesizer.
//!
//! The card streams a small ring of DMA buffers (a buffer descriptor
//! list in bus master mode) and raises its PCI interrupt every time one
//! buffer finishes. The interrupt handler only records which buffers
//! completed and wakes the synth task, which refills them: for every
//! active voice it sums a few sine harmonics (like the drawbars of a
//! Hammond organ), all in fixed point integer math -- this kernel
//! targets soft-float.
//!
//! AC'97 replaced the earlier SB16 driver: QEMU's SB16 sits behind the
//! emulated i8257 ISA DMA controller whose transfer loop spins the QEMU
//! main loop at 100% whenever the host audio buffer is full, freezing
//! the display. The AC'97 device is paced by the audio backend itself
//! and has no such loop.

use crate::memory::BootInfoFrameAllocator;
use crate::pci;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use core::task::{Context, Poll};
use futures_util::task::AtomicWaker;
use spinning_top::Spinlock;
use x86_64::VirtAddr;
use x86_64::instructions::port::Port;
use x86_64::structures::paging::{FrameAllocator, PhysFrame};

// --- hardware constants (QEMU: -device AC97) ---
const VENDOR_INTEL: u16 = 0x8086;
const DEVICE_82801AA: u16 = 0x2415;

// mixer registers (I/O BAR 0)
const MIXER_RESET: u16 = 0x00;
const MIXER_MASTER_VOLUME: u16 = 0x02;
const MIXER_PCM_OUT_VOLUME: u16 = 0x18;

// bus master registers (I/O BAR 1), PCM-out channel + globals
const PO_BDBAR: u16 = 0x10; // buffer descriptor list base (u32)
const PO_CIV: u16 = 0x14; // current index (u8, read-only)
const PO_LVI: u16 = 0x15; // last valid index (u8)
const PO_SR: u16 = 0x16; // status (u16)
const PO_CR: u16 = 0x1B; // control (u8)
const GLOB_CNT: u16 = 0x2C; // global control (u32)

// PO_SR bits (write 1 to clear)
const SR_LVBCI: u16 = 1 << 2; // last valid buffer completed
const SR_BCIS: u16 = 1 << 3; // buffer completed
const SR_FIFOE: u16 = 1 << 4; // FIFO error
const SR_INT_BITS: u16 = SR_LVBCI | SR_BCIS | SR_FIFOE;

// PO_CR bits
const CR_RPBM: u8 = 1 << 0; // run
const CR_RR: u8 = 1 << 1; // reset registers
const CR_IOCE: u8 = 1 << 4; // interrupt on completion enable

/// The PCM DAC rate without the variable-rate extension.
pub const SAMPLE_RATE: u32 = 48_000;

/// DMA ring: each buffer is one page of interleaved stereo frames. The
/// hardware descriptor list always has 32 entries (CIV/LVI are 5-bit
/// indices), so the entries point at the pages round robin.
const BDL_ENTRIES: usize = 32;
const BUFFERS: usize = 4;
const FRAMES_PER_BUFFER: usize = 1024;
const SAMPLES_PER_BUFFER: usize = FRAMES_PER_BUFFER * 2;

const MAX_VOICES: usize = 8;

/// Drawbar-style harmonic mix: (multiple of the fundamental, amplitude).
/// Roughly the 8', 4', 2 2/3' and 2' registers of an organ.
const HARMONICS: [(u32, i32); 4] = [(1, 32), (2, 16), (3, 8), (4, 4)];

static AC97: Spinlock<Option<Ac97>> = Spinlock::new(None);

/// Bus master base port for the interrupt handler; 0 = not initialized.
static NABM_BASE: AtomicU16 = AtomicU16::new(0);
/// Bit mask of ring buffers that finished playing and need a refill.
/// Set by the IRQ handler, consumed by the synth task.
static REFILL_MASK: AtomicU8 = AtomicU8::new(0);
/// The descriptor index the handler expects to complete next.
static NEXT_DONE: AtomicU8 = AtomicU8::new(0);
static SYNTH_WAKER: AtomicWaker = AtomicWaker::new();

struct Ac97 {
    /// All ring buffers as one slice of interleaved stereo samples,
    /// accessed through the physical memory mapping.
    buffer: &'static mut [i16],
    voices: [Option<Voice>; MAX_VOICES],
}

#[derive(Clone, Copy)]
struct Voice {
    /// Phase accumulator: the full u32 range is one period.
    phase: u32,
    /// Phase increment per frame, i.e. the frequency.
    step: u32,
}

/// One entry of the buffer descriptor list.
#[repr(C)]
struct BdlEntry {
    addr: u32,
    /// Length in 16-bit samples (not frames, not bytes).
    samples: u16,
    /// Bit 15: interrupt on completion.
    control: u16,
}

const BDL_IOC: u16 = 1 << 15;

/// Allocate physically contiguous frames; PCI bus mastering has none of
/// the ISA DMA reach/boundary restrictions.
fn alloc_contiguous(frame_allocator: &mut BootInfoFrameAllocator, count: usize) -> Option<u64> {
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
        if have == count {
            return Some(start.unwrap().start_address().as_u64());
        }
    }
    None
}

/// Detect the card, set up the DMA ring and start silent playback.
pub fn init(
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_mem_offset: VirtAddr,
) -> Result<(), &'static str> {
    let device = pci::find(VENDOR_INTEL, DEVICE_82801AA).ok_or("no AC97 on the PCI bus")?;
    device.enable_io_and_bus_master();
    let mixer = device.io_bar(0).ok_or("mixer BAR is not I/O")?;
    let nabm = device.io_bar(1).ok_or("bus master BAR is not I/O")?;

    let irq_line = device.interrupt_line();
    if irq_line == 0 || irq_line >= 16 {
        return Err("firmware routed no usable IRQ line");
    }
    crate::interrupts::register_pci_sound_irq(irq_line)?;

    unsafe {
        // release the AC-link cold reset so the codec talks to us
        Port::<u32>::new(nabm + GLOB_CNT).write(0x2);
        // codec reset, then unmute: 0x0000 = 0 dB master, 0x0808 = 0 dB PCM
        Port::<u16>::new(mixer + MIXER_RESET).write(0);
        Port::<u16>::new(mixer + MIXER_MASTER_VOLUME).write(0x0000);
        Port::<u16>::new(mixer + MIXER_PCM_OUT_VOLUME).write(0x0808);
    }

    // one page for the descriptor list + one page per ring buffer
    let phys = alloc_contiguous(frame_allocator, 1 + BUFFERS).ok_or("no contiguous memory")?;
    let virt = phys_mem_offset + phys;
    let buffers_phys = phys + 4096;

    let bdl =
        unsafe { core::slice::from_raw_parts_mut(virt.as_mut_ptr::<BdlEntry>(), BDL_ENTRIES) };
    for (i, entry) in bdl.iter_mut().enumerate() {
        *entry = BdlEntry {
            addr: (buffers_phys + (i % BUFFERS * 4096) as u64) as u32,
            samples: SAMPLES_PER_BUFFER as u16,
            control: BDL_IOC,
        };
    }

    let buffer = unsafe {
        core::slice::from_raw_parts_mut(
            (phys_mem_offset + buffers_phys).as_mut_ptr::<i16>(),
            BUFFERS * SAMPLES_PER_BUFFER,
        )
    };
    buffer.fill(0);

    unsafe {
        // reset the PCM-out channel registers
        Port::<u8>::new(nabm + PO_CR).write(CR_RR);
        for _ in 0..100_000 {
            if Port::<u8>::new(nabm + PO_CR).read() & CR_RR == 0 {
                break;
            }
        }
        Port::<u32>::new(nabm + PO_BDBAR).write(phys as u32);
        Port::<u8>::new(nabm + PO_LVI).write((BDL_ENTRIES - 1) as u8);
        Port::<u16>::new(nabm + PO_SR).write(SR_INT_BITS);
        Port::<u8>::new(nabm + PO_CR).write(CR_RPBM | CR_IOCE);
    }

    NEXT_DONE.store(0, Ordering::Relaxed);
    NABM_BASE.store(nabm, Ordering::Release);
    *AC97.lock() = Some(Ac97 {
        buffer,
        voices: [None; MAX_VOICES],
    });
    log::info!("ac97: initialized (IRQ {})", irq_line);
    Ok(())
}

pub fn is_available() -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| AC97.lock().is_some())
}

// --- the synthesizer ---

fn freq_to_step(freq_hz: u32) -> u32 {
    ((freq_hz as u64) * (1u64 << 32) / SAMPLE_RATE as u64) as u32
}

/// Start a voice. Polyphonic: up to MAX_VOICES notes sound at once.
pub fn note_on(freq_hz: u32) {
    let step = freq_to_step(freq_hz);
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(ac97) = AC97.lock().as_mut() {
            // retrigger if the note is already sounding
            if let Some(voice) = ac97.voices.iter_mut().flatten().find(|v| v.step == step) {
                voice.phase = 0;
                return;
            }
            if let Some(slot) = ac97.voices.iter_mut().find(|v| v.is_none()) {
                *slot = Some(Voice { phase: 0, step });
            }
        }
    });
}

pub fn note_off(freq_hz: u32) {
    let step = freq_to_step(freq_hz);
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(ac97) = AC97.lock().as_mut() {
            for slot in ac97.voices.iter_mut() {
                if matches!(slot, Some(v) if v.step == step) {
                    *slot = None;
                }
            }
        }
    });
}

pub fn all_notes_off() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(ac97) = AC97.lock().as_mut() {
            ac97.voices = [None; MAX_VOICES];
        }
    });
}

/// Additive synthesis of one ring buffer (interleaved stereo).
fn synthesize(voices: &mut [Option<Voice>], out: &mut [i16]) {
    for frame in out.chunks_exact_mut(2) {
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
        let sample = (acc >> 9).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        frame[0] = sample;
        frame[1] = sample;
    }
}

/// Called from the PCI sound interrupt handler. Returns false if the
/// card did not actually interrupt (the line may be shared). Minimal on
/// purpose: synthesis in interrupt context starved the rest of the
/// system, so the handler only records completed buffers and wakes the
/// synth task.
pub(crate) fn handle_irq() -> bool {
    let nabm = NABM_BASE.load(Ordering::Acquire);
    if nabm == 0 {
        return false;
    }
    let mut status_port = Port::<u16>::new(nabm + PO_SR);
    let status = unsafe { status_port.read() };
    if status & SR_INT_BITS == 0 {
        return false;
    }
    // ack the card (write-1-to-clear), otherwise the level-triggered
    // line stays high and the CPU interrupts forever
    unsafe { status_port.write(status & SR_INT_BITS) };

    let civ = unsafe { Port::<u8>::new(nabm + PO_CIV).read() } as usize % BDL_ENTRIES;
    // every descriptor between the last known position and CIV has
    // completed; mark its buffer page for a refill
    let mut next = NEXT_DONE.load(Ordering::Relaxed) as usize;
    let mut mask = 0u8;
    while next != civ {
        mask |= 1 << (next % BUFFERS);
        next = (next + 1) % BDL_ENTRIES;
    }
    NEXT_DONE.store(next as u8, Ordering::Relaxed);
    if mask != 0 {
        REFILL_MASK.fetch_or(mask, Ordering::Release);
        SYNTH_WAKER.wake();
    }

    // keep the ring endless: the last valid index stays just behind the
    // current one; writing LVI also restarts the DMA if it halted
    unsafe {
        Port::<u8>::new(nabm + PO_LVI).write(((civ + BDL_ENTRIES - 1) % BDL_ENTRIES) as u8);
    }
    true
}

/// Future that resolves with the mask of buffers needing a refill.
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

/// The synth task: refills finished ring buffers outside of interrupt
/// context, so long synthesis can never block interrupt delivery.
pub async fn run_synth() {
    if !is_available() {
        return;
    }
    loop {
        let mask = Refill.await;
        for buf in 0..BUFFERS {
            if mask & (1 << buf) == 0 {
                continue;
            }
            let mut guard = AC97.lock();
            if let Some(ac97) = guard.as_mut() {
                let start = buf * SAMPLES_PER_BUFFER;
                let Ac97 { buffer, voices } = ac97;
                synthesize(voices, &mut buffer[start..start + SAMPLES_PER_BUFFER]);
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
