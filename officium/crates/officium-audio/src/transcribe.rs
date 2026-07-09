//! Transcription (M8, §8.1–8.3): audio-in. A monophonic phrase played
//! on the manual becomes a plain-form score, which then flows through
//! the ordinary parser and the M7 checker — audio is just another
//! surface syntax.
//!
//! Mode recovery (§8.1): the **final** is the pitch class the phrase
//! cadences to (its last note); the **ambitus** decides authentic vs
//! plagal — a melody that dips below its cadence is plagal, exactly
//! how the modes were historically told apart. The anchor is the
//! cadence pitch itself, so the player may sing in any octave.
//!
//! Each note is a scale degree relative to the final; degrees map to
//! operations through the bijective table in `degree_op`. The op
//! stream is parsed as the exact inverse of `render::walk_verse`:
//! statements until a cadence, and a Branch note opens an `si` whose
//! two blocks each cadence on their own. The emitted statements are
//! skeletal but well-typed and runnable — a melody underdetermines
//! its words, so the transcriber chooses canonical ones.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use officium_core::types::{Dissonance, Final, Mode};

use crate::degree_op::{degree_offset, op_of_degree, RenderOp};
use crate::render::Pitch;

/// Transcribe a played phrase into a plain-form score. Errors are
/// `Dissonance::Parse` whose `line` is the 1-based note number.
pub fn transcribe(pitches: &[Pitch], name: &str) -> Result<String, Dissonance> {
    let err = |note: usize, msg: String| Dissonance::Parse { line: note as u32, msg };

    let Some(&cadence) = pitches.last() else {
        return Err(err(0, String::from("silence: nothing to transcribe")));
    };
    let final_ = match cadence % 12 {
        2 => Final::Re,
        4 => Final::Mi,
        5 => Final::Fa,
        7 => Final::Sol,
        _ => {
            return Err(err(
                pitches.len(),
                String::from("cadentia extra modos: the phrase must end on D, E, F or G"),
            ))
        }
    };
    let plagal = pitches.iter().any(|&p| p < cadence);
    let mode = Mode::from_final(final_, plagal);

    // pitches -> degrees -> ops, all relative to the cadence pitch
    let mut ops: Vec<RenderOp> = Vec::new();
    for (i, &p) in pitches.iter().enumerate() {
        let rel = p as i16 - cadence as i16;
        let deg = (1..=8u8).find(|&d| degree_offset(mode, d) as i16 == rel);
        let Some(deg) = deg else {
            return Err(err(i + 1, format!("nota extra modum: pitch {p} is no degree of {mode:?}")));
        };
        ops.push(op_of_degree(mode, deg).unwrap());
    }

    // build the verse: the exact inverse of render's walk
    let mut t = Transcriber {
        ops: &ops,
        mode,
        pos: 0,
        body: String::new(),
        vars: 0,
        prev: None,
        needs_fugue: false,
    };
    t.block(1)?;
    if t.pos != ops.len() {
        return Err(err(t.pos + 1, String::from("cantus post amen: notes after the final cadence")));
    }

    // final token: uppercase = authentic, lowercase = plagal (§7.1)
    let tok = match final_ {
        Final::Re => "Re",
        Final::Mi => "Mi",
        Final::Fa => "Fa",
        Final::Sol => "Sol",
    };
    let vtok = if plagal { tok.to_ascii_lowercase() } else { String::from(tok) };

    let mut out = format!(
        "; transcribed from the manual: {} notes, {:?} on {}\n",
        pitches.len(),
        mode,
        tok
    );
    if t.needs_fugue {
        // the organ must hold something for ask/resolve to touch: the
        // reciting tone in the pedals, and an identity subject to answer
        let reciting = crate::degree_op::degree_pitch(mode, if plagal { 3 } else { 5 });
        out.push_str(&format!("fuga organum in {tok} {{\n"));
        out.push_str(&format!("  pedale Dominus = {reciting}\n"));
        out.push_str("  reale  vox @ cantus = \\x -> x\n");
        out.push_str("}\n");
    }
    out.push_str(&format!("versus {name} in {vtok} {{\n"));
    out.push_str(&t.body);
    out.push_str("}\n");
    Ok(out)
}

struct Transcriber<'a> {
    ops: &'a [RenderOp],
    mode: Mode,
    pos: usize,
    body: String,
    vars: u32,
    /// The most recent binder — what the next line sings about.
    prev: Option<String>,
    needs_fugue: bool,
}

impl Transcriber<'_> {
    fn err(&self, msg: &str) -> Dissonance {
        Dissonance::Parse { line: (self.pos + 1) as u32, msg: String::from(msg) }
    }

    fn next_var(&mut self) -> String {
        self.vars += 1;
        format!("v{}", self.vars)
    }

    /// The previous value, or a literal when the phrase opens cold.
    fn prev_val(&self) -> String {
        self.prev.clone().unwrap_or_else(|| String::from("0"))
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.body.push_str("  ");
        }
        self.body.push_str(text);
        self.body.push('\n');
    }

    /// One block: statements until a cadence (or a branch, which
    /// carries a cadence in each of its arms) — `walk_verse` inverted.
    fn block(&mut self, depth: usize) -> Result<(), Dissonance> {
        loop {
            let Some(&op) = self.ops.get(self.pos) else {
                return Err(self.err("phrasis sine cadentia: the phrase never cadences"));
            };
            self.pos += 1;
            match op {
                RenderOp::Cadence => {
                    let v = self.prev_val();
                    self.line(depth, &format!("amen {v}"));
                    return Ok(());
                }
                RenderOp::Branch => {
                    // the volta rings the octave: si with two sung arms
                    self.line(depth, "si verum tunc {");
                    self.block(depth + 1)?;
                    self.line(depth, "} aliter {");
                    self.block(depth + 1)?;
                    self.line(depth, "}");
                    return Ok(());
                }
                RenderOp::Arg => {
                    let v = self.next_var();
                    self.line(depth, &format!("{v} <- arg"));
                    self.prev = Some(v);
                }
                RenderOp::Ask => {
                    self.needs_fugue = true;
                    let v = self.next_var();
                    self.line(depth, &format!("{v} <- ask \"Dominus\""));
                    self.prev = Some(v);
                }
                RenderOp::Pure => {
                    let (v, p) = (self.next_var(), self.prev_val());
                    self.line(depth, &format!("{v} <- pure {p}"));
                    self.prev = Some(v);
                }
                RenderOp::BindOther => {
                    let (v, p) = (self.next_var(), self.prev_val());
                    self.line(depth, &format!("{v} <- {p}"));
                    self.prev = Some(v);
                }
                RenderOp::Resolve => {
                    self.needs_fugue = true;
                    let v = self.next_var();
                    self.line(
                        depth,
                        &format!("{v} <- resolve \"vox\" (genus (corpus 0 \"cantus\" 0 0))"),
                    );
                    self.prev = Some(v);
                }
                RenderOp::Modal => {
                    // the mode's own non-cadence op where one exists;
                    // elsewhere the note degrades to a plain bind
                    let p = self.prev_val();
                    match self.mode {
                        Mode::Lydian | Mode::Hypolydian => {
                            self.line(depth, &format!("pone {p}"));
                        }
                        Mode::Hypomixolydian => {
                            self.line(depth, &format!("mitte (cmd {p} {p} {p})"));
                        }
                        // Protus/Deuterus modal ops are cadences and
                        // Mixolydian's is perage — a mid-phrase modal
                        // note there can only be a plain bind
                        _ => {
                            let v = self.next_var();
                            self.line(depth, &format!("{v} <- {p}"));
                            self.prev = Some(v);
                        }
                    }
                }
            }
        }
    }
}
