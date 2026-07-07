//! Degree ↔ operation table (§8.2). THE single place where scale
//! degrees map to language operations; render and (future) live
//! transcription must both read from here. Totality matters more than
//! the particular assignment — every renderable op has a degree.

use officium_core::types::{Final, Mode};

/// What a verse line "is", for rendering purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOp {
    Cadence, // amen / nihil / clama / perage — the phrase ends
    Ask,     // Dominus: the reciting tone
    Arg,
    Pure,
    Resolve,
    Modal, // the mode's own op: lege/pone/mitte/...
    BindOther,
    Branch, // si/aliter
}

/// Scale degree (1-based) for an op. Degree 1 is always the final;
/// the reciting tone is degree 5 (authentic) or degree 3 (plagal) —
/// exactly how the psalm tones recite.
pub fn degree(mode: Mode, op: RenderOp) -> u8 {
    let reciting = if mode.is_plagal() { 3 } else { 5 };
    match op {
        RenderOp::Cadence => 1,
        RenderOp::Ask => reciting,
        RenderOp::Arg => 2,
        RenderOp::Pure => 3,
        RenderOp::Resolve => 7,
        RenderOp::Modal => 6,
        RenderOp::BindOther => 4,
        RenderOp::Branch => 5,
    }
}

/// MIDI pitch of a final (octave 4).
pub fn final_pitch(f: Final) -> u8 {
    match f {
        Final::Re => 62,  // D4
        Final::Mi => 64,  // E4
        Final::Fa => 65,  // F4
        Final::Sol => 67, // G4
    }
}

/// Semitone offsets of the white-key (church mode) scale on each final.
/// The modes really are these scales; the render is not a metaphor.
pub fn scale(f: Final) -> [u8; 8] {
    match f {
        Final::Re => [0, 2, 3, 5, 7, 9, 10, 12],  // dorian
        Final::Mi => [0, 1, 3, 5, 7, 8, 10, 12],  // phrygian
        Final::Fa => [0, 2, 4, 6, 7, 9, 11, 12],  // lydian
        Final::Sol => [0, 2, 4, 5, 7, 9, 10, 12], // mixolydian
    }
}

/// MIDI pitch of a 1-based scale degree in a mode. Plagal modes sit an
/// octave lower — they "reach below the final".
pub fn degree_pitch(mode: Mode, deg: u8) -> u8 {
    let f = mode.final_();
    let base = final_pitch(f) - if mode.is_plagal() { 12 } else { 0 };
    let idx = ((deg - 1) % 8) as usize;
    base + scale(f)[idx]
}
