//! Render (§8.4): IR -> note events -> an `Organ`. This is how a
//! program is read back aurally. The interpreter emits notes; it never
//! touches AC'97 registers, the BDL, or DMA (§9).

use alloc::string::String;
use alloc::vec::Vec;

use officium_core::env::Program;
use officium_core::ir::Expr;
use officium_core::types::Mode;

use crate::degree_op::{degree, degree_pitch, final_pitch, RenderOp};

pub type Pitch = u8; // MIDI note number
pub type Ticks = u32;
pub type Micros = u64;
/// Organ stops. v1: a plain bitmask stub.
pub type Registration = u8;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub pitch: Pitch,
    pub onset: Ticks,
    pub dur: Ticks,
    pub voice: u8,
}

pub type Events = Vec<Note>;

/// The seam to the synthesizer (§9). The kernel wraps its AC'97 synth
/// in this; hosted mode uses a mock.
pub trait Organ {
    fn note_on(&mut self, voice: u8, pitch: Pitch, stops: Registration);
    fn note_off(&mut self, voice: u8, pitch: Pitch);
    /// Advance time by dt (the kernel impl lets the synth run; the mock
    /// just accounts for it).
    fn tick(&mut self, dt: Micros);
}

/// Classify an expression head for the degree table.
fn classify(e: &Expr) -> RenderOp {
    match e {
        Expr::Return(_, _) | Expr::Nihil | Expr::Clama(_) | Expr::Perage(_) => RenderOp::Cadence,
        Expr::Ask(_) => RenderOp::Ask,
        Expr::Arg => RenderOp::Arg,
        Expr::Pure(_, _) => RenderOp::Pure,
        Expr::Resolve(_, _) => RenderOp::Resolve,
        Expr::Lege | Expr::Pone(_) | Expr::Mitte(_) | Expr::Lift(_) => RenderOp::Modal,
        Expr::If(_, _, _) => RenderOp::Branch,
        _ => RenderOp::BindOther,
    }
}

/// Walk a verse body's bind spine, one note per line (§8.2).
fn walk_verse(e: &Expr, mode: Mode, voice: u8, onset: &mut Ticks, out: &mut Events) {
    match e {
        Expr::Bind(_, m, _, k) => {
            push_note(out, mode, classify(m), voice, onset, 1);
            walk_verse(k, mode, voice, onset, out);
        }
        Expr::If(_, t, els) => {
            push_note(out, mode, RenderOp::Branch, voice, onset, 1);
            walk_verse(t, mode, voice, onset, out);
            walk_verse(els, mode, voice, onset, out);
        }
        Expr::Return(_, _) | Expr::Nihil | Expr::Clama(_) | Expr::Perage(_) => {
            // the cadence lands on the final and is held
            push_note(out, mode, RenderOp::Cadence, voice, onset, 2);
        }
        // a bare trailing expression (e.g. the Unit placeholder)
        _ => {}
    }
}

fn push_note(out: &mut Events, mode: Mode, op: RenderOp, voice: u8, onset: &mut Ticks, dur: Ticks) {
    out.push(Note {
        pitch: degree_pitch(mode, degree(mode, op)),
        onset: *onset,
        dur,
        voice,
    });
    *onset += dur;
}

/// The fugue subject as a fixed motif: final — reciting — 3rd — final.
const SUBJECT_MOTIF: [u8; 4] = [1, 5, 3, 1];

/// Render a whole program: pedals on voice 0, subjects on voice 1,
/// verses on voices 2+.
pub fn render_program(prog: &Program) -> Events {
    let mut out = Events::new();
    let mut onset: Ticks = 0;

    for fugue in &prog.fugues {
        let mode = Mode::from_final(fugue.final_, false);
        // pedal point: held under the whole exposition
        let pedal_start = onset;
        if fugue.subject.is_some() {
            for deg in SUBJECT_MOTIF {
                push_note(&mut out, mode, RenderOp::BindOther, 1, &mut onset, 0);
                // overwrite with the motif degree (the table is for verse
                // ops; the subject is literal melody)
                let n = out.last_mut().unwrap();
                n.pitch = degree_pitch(mode, deg);
                n.dur = 1;
                onset += 1;
            }
        }
        let pedal_dur = (onset - pedal_start).max(2);
        if !fugue.pedals.is_empty() {
            out.push(Note {
                pitch: final_pitch(fugue.final_) - 24, // down in the pedals
                onset: pedal_start,
                dur: pedal_dur,
                voice: 0,
            });
        }
    }

    for (i, versus) in prog.verses.iter().enumerate() {
        walk_verse(&versus.body, versus.mode, 2 + i as u8, &mut onset, &mut out);
    }
    out
}

/// Render a single verse.
pub fn render_verse(body: &Expr, mode: Mode, voice: u8) -> Events {
    let mut out = Events::new();
    let mut onset = 0;
    walk_verse(body, mode, voice, &mut onset, &mut out);
    out
}

/// Microseconds per tick (tempo knob).
pub const TICK_US: Micros = 250_000;

/// Drive an `Organ` through an event list, tick by tick.
pub fn play(events: &Events, organ: &mut dyn Organ) {
    if events.is_empty() {
        return;
    }
    let end = events.iter().map(|n| n.onset + n.dur).max().unwrap_or(0);
    for t in 0..end {
        for n in events.iter().filter(|n| n.onset == t) {
            organ.note_on(n.voice, n.pitch, 0);
        }
        organ.tick(TICK_US);
        for n in events.iter().filter(|n| n.onset + n.dur == t + 1) {
            organ.note_off(n.voice, n.pitch);
        }
    }
}

/// Equal-temperament pitch -> Hz (rounded), for synths that speak Hz.
pub fn pitch_hz(p: Pitch) -> u32 {
    let semis = p as f64 - 69.0;
    let hz = 440.0 * libm::exp2(semis / 12.0);
    // round to nearest integer Hz
    libm::round(hz) as u32
}

/// A mock organ that records everything (hosted mode / tests).
#[derive(Default)]
pub struct MockOrgan {
    pub log: Vec<String>,
    pub time: Micros,
}

impl Organ for MockOrgan {
    fn note_on(&mut self, voice: u8, pitch: Pitch, _stops: Registration) {
        let hz = pitch_hz(pitch);
        self.log.push(alloc::format!("t={:>7}us  v{} ON  {} ({} Hz)", self.time, voice, pitch, hz));
    }
    fn note_off(&mut self, voice: u8, pitch: Pitch) {
        self.log
            .push(alloc::format!("t={:>7}us  v{} OFF {}", self.time, voice, pitch));
    }
    fn tick(&mut self, dt: Micros) {
        self.time += dt;
    }
}

/// Convenience constructor mirroring the kernel's organ factory.
pub fn mock_organ() -> MockOrgan {
    MockOrgan::default()
}
