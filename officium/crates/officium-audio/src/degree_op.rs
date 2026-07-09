//! Degree ↔ operation table (§8.2). THE single place where scale
//! degrees map to language operations; render and live transcription
//! (§8.1, M8) both read from here. The table is a bijection per mode —
//! audio-out and audio-in share one mapping, so a rendered verse
//! transcribes back to the same operation sequence.

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
/// exactly how the psalm tones recite. `pure` takes whichever of the
/// two the reciting tone left free, and the branch turns on the
/// octave — so the table is injective and transcription can invert it.
pub fn degree(mode: Mode, op: RenderOp) -> u8 {
    let plagal = mode.is_plagal();
    match op {
        RenderOp::Cadence => 1,
        RenderOp::Arg => 2,
        RenderOp::Ask => if plagal { 3 } else { 5 },
        RenderOp::Pure => if plagal { 5 } else { 3 },
        RenderOp::BindOther => 4,
        RenderOp::Modal => 6,
        RenderOp::Resolve => 7,
        RenderOp::Branch => 8,
    }
}

/// The inverse of `degree` — total on 1..=8.
pub fn op_of_degree(mode: Mode, deg: u8) -> Option<RenderOp> {
    let plagal = mode.is_plagal();
    Some(match deg {
        1 => RenderOp::Cadence,
        2 => RenderOp::Arg,
        3 => if plagal { RenderOp::Ask } else { RenderOp::Pure },
        4 => RenderOp::BindOther,
        5 => if plagal { RenderOp::Pure } else { RenderOp::Ask },
        6 => RenderOp::Modal,
        7 => RenderOp::Resolve,
        8 => RenderOp::Branch,
        _ => return None,
    })
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

/// Semitone offset of a 1-based degree from the final. Authentic modes
/// sit on final..final+octave; plagal modes "reach below the final":
/// their upper degrees fold down, spanning a fourth below to a fifth
/// above — the historic ambitus, and exactly what transcription (§8.1)
/// reads back: a melody that dips under its cadence is plagal.
pub fn degree_offset(mode: Mode, deg: u8) -> i8 {
    let s = scale(mode.final_())[((deg - 1) % 8) as usize] as i8;
    if mode.is_plagal() {
        match deg {
            6 | 7 => s - 12,
            8 => -5, // the fourth below: the plagal bottom
            _ => s,
        }
    } else {
        s
    }
}

/// MIDI pitch of a 1-based scale degree in a mode (octave-4 finals).
pub fn degree_pitch(mode: Mode, deg: u8) -> u8 {
    (final_pitch(mode.final_()) as i16 + degree_offset(mode, deg) as i16) as u8
}
