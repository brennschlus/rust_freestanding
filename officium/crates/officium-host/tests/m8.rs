//! M8 exit criteria (§8.1–8.3): transcription inverts render. A verse
//! rendered to notes and transcribed back yields a score that parses,
//! typechecks, and renders to the very same melody; mode recovery
//! (final + ambitus) identifies all the modes; off-mode playing is
//! refused with a note number.

use officium_audio::{render_verse, transcribe, Pitch};
use officium_core::check_program;
use officium_core::machine::{empty_state, run_verse};
use officium_core::types::{Dissonance, Mode, Value};
use officium_core::Env;
use officium_parse::parse;

const METEOR: &str = include_str!("../../../scores/meteor.off");
const CANTUS: &str = include_str!("../../../scores/meteor_cantus.off");

fn melody(body: &officium_core::Expr, mode: Mode) -> Vec<Pitch> {
    render_verse(body, mode, 2).iter().map(|n| n.pitch).collect()
}

#[test]
fn every_reference_verse_survives_the_roundtrip() {
    for src in [METEOR, CANTUS] {
        let prog = parse(src).unwrap();
        for v in &prog.verses {
            let played = melody(&v.body, v.mode);
            let text = transcribe(&played, "auditum")
                .unwrap_or_else(|d| panic!("verse {} untranscribable: {d:?}", v.name));
            let heard = parse(&text)
                .unwrap_or_else(|d| panic!("transcript of {} unparsable: {d:?}\n{text}", v.name));
            check_program(&heard)
                .unwrap_or_else(|d| panic!("transcript of {} discors: {d:?}\n{text}", v.name));

            let hv = heard.verse("auditum").unwrap();
            assert_eq!(hv.mode, v.mode, "mode recovery for {}", v.name);
            assert_eq!(
                melody(&hv.body, hv.mode),
                played,
                "verse {} does not re-render to the played melody:\n{text}",
                v.name
            );
        }
    }
}

#[test]
fn ambitus_tells_authentic_from_plagal() {
    // same pitch classes, cadence on G: staying above the cadence is
    // authentic Mixolydian, dipping below it is Hypomixolydian
    let above = [69, 67]; // A4 G4
    let below = [69, 64, 67]; // A4 E4 G4 — dips under the cadence
    let auth = parse(&transcribe(&above, "x").unwrap()).unwrap();
    let plag = parse(&transcribe(&below, "x").unwrap()).unwrap();
    assert_eq!(auth.verse("x").unwrap().mode, Mode::Mixolydian);
    assert_eq!(plag.verse("x").unwrap().mode, Mode::Hypomixolydian);
}

#[test]
fn the_player_may_sing_in_any_octave() {
    // the same authentic Dorian phrase, an octave apart
    let low = [52, 55, 50]; // E3 G3 D3
    let high = [64, 67, 62]; // E4 G4 D4
    let a = transcribe(&low, "x").unwrap();
    let b = transcribe(&high, "x").unwrap();
    let (a, b) = (parse(&a).unwrap(), parse(&b).unwrap());
    assert_eq!(a.verse("x").unwrap().mode, Mode::Dorian);
    assert_eq!(b.verse("x").unwrap().mode, Mode::Dorian);
}

#[test]
fn a_transcript_actually_runs() {
    // arg — pure — ask (reciting) — cadence, in authentic Dorian:
    // D4 E4 F4 A4 D4 = arg, pure, pure, ask, amen
    let played = [64, 65, 69, 62];
    let text = transcribe(&played, "improvisus").unwrap();
    let prog = parse(&text).unwrap();
    check_program(&prog).unwrap();
    let env = Env::from_fugues(&prog.fugues);
    let v = prog.verse("improvisus").unwrap();
    let out = run_verse(&env, v.mode, &v.body, Value::Num(9.0), empty_state(), 10_000).unwrap();
    // the phrase cadences on what the reciting tone asked: the Dominus
    // pedal the transcriber wrote into the fugue (A4 = 69)
    assert_eq!(out.value, Some(Value::Num(69.0)));
}

#[test]
fn off_final_cadence_is_refused() {
    let r = transcribe(&[60, 62, 60], "x"); // cadences on C
    match r {
        Err(Dissonance::Parse { line, msg }) => {
            assert_eq!(line, 3, "the offending note number");
            assert!(msg.contains("cadentia"), "got: {msg}");
        }
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[test]
fn chromatic_notes_are_refused_with_their_note_number() {
    let r = transcribe(&[63, 62], "x"); // Eb over a D final is no degree
    match r {
        Err(Dissonance::Parse { line, msg }) => {
            assert_eq!(line, 1);
            assert!(msg.contains("nota extra modum"), "got: {msg}");
        }
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[test]
fn notes_after_the_cadence_are_refused() {
    // degree 1 mid-phrase cadences early; the rest is unreachable
    let r = transcribe(&[62, 64, 62], "x");
    match r {
        Err(Dissonance::Parse { msg, .. }) => {
            assert!(msg.contains("post amen"), "got: {msg}");
        }
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[test]
fn silence_is_refused() {
    assert!(transcribe(&[], "x").is_err());
}

#[test]
fn the_degree_table_is_a_bijection_in_every_mode() {
    use officium_audio::degree_op::{degree, degree_offset, op_of_degree, RenderOp};
    use officium_core::types::Final;
    let ops = [
        RenderOp::Cadence,
        RenderOp::Ask,
        RenderOp::Arg,
        RenderOp::Pure,
        RenderOp::Resolve,
        RenderOp::Modal,
        RenderOp::BindOther,
        RenderOp::Branch,
    ];
    for f in [Final::Re, Final::Mi, Final::Fa, Final::Sol] {
        for plagal in [false, true] {
            let mode = Mode::from_final(f, plagal);
            // op -> degree -> op closes
            for op in ops {
                let d = degree(mode, op);
                assert_eq!(op_of_degree(mode, d), Some(op), "{mode:?} {op:?}");
            }
            // and the eight degrees land on eight distinct pitches
            let mut offs: Vec<i8> = (1..=8).map(|d| degree_offset(mode, d)).collect();
            offs.sort_unstable();
            offs.dedup();
            assert_eq!(offs.len(), 8, "{mode:?} degree pitches collide");
        }
    }
}
