//! PC speaker driver: square wave tones via PIT channel 2.
//!
//! Channel 0 of the PIT drives the timer interrupt; channel 2 is wired
//! to the speaker. Programming a frequency divisor into channel 2 and
//! opening the gate in port 0x61 produces a steady square wave without
//! any further CPU involvement.

use x86_64::instructions::port::Port;

/// Base frequency of the Programmable Interval Timer.
const PIT_FREQUENCY: u32 = 1_193_182;

/// Start a continuous square wave tone. The tone keeps sounding until
/// `stop_tone` is called.
pub fn start_tone(freq_hz: u32) {
    // the 16-bit divisor limits the range to ~19 Hz .. ~1.19 MHz
    let divisor = (PIT_FREQUENCY / freq_hz.max(19)).clamp(1, 0xFFFF) as u16;
    unsafe {
        // command: channel 2, access lobyte/hibyte, mode 3 (square wave)
        Port::<u8>::new(0x43).write(0b1011_0110u8);
        let mut channel_2 = Port::<u8>::new(0x42);
        channel_2.write(divisor as u8);
        channel_2.write((divisor >> 8) as u8);

        // bit 0: gate channel 2, bit 1: connect its output to the speaker
        let mut control = Port::<u8>::new(0x61);
        let value = control.read();
        control.write(value | 0b11);
    }
}

pub fn stop_tone() {
    unsafe {
        let mut control = Port::<u8>::new(0x61);
        let value = control.read();
        control.write(value & !0b11);
    }
}

/// Frequency of a note like "a4" or "c#5" (octaves 0-8), equal
/// temperament rounded to whole hertz.
pub fn note_freq(note: &str) -> Option<u32> {
    if note.len() < 2 {
        return None;
    }
    let (name, octave) = note.split_at(note.len() - 1);
    let octave: u32 = octave.parse().ok()?;
    // octave 4 base frequencies; other octaves are powers of two away
    let base: u32 = match name {
        "c" => 262,
        "c#" => 277,
        "d" => 294,
        "d#" => 311,
        "e" => 330,
        "f" => 349,
        "f#" => 370,
        "g" => 392,
        "g#" => 415,
        "a" => 440,
        "a#" => 466,
        "b" => 494,
        _ => return None,
    };
    match octave {
        0..=4 => Some(base >> (4 - octave)),
        5..=8 => Some(base << (octave - 4)),
        _ => None,
    }
}
