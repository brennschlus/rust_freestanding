//! M6 exit criteria: §14 keeps working through the eight-line form —
//! the sung score runs the same pipeline to the same Plan as the
//! plain one, and it still renders on the organ.

use officium_core::builtins::make_corpus;
use officium_core::machine::{empty_state, resume, run_verse};
use officium_core::types::{Dissonance, Value};
use officium_core::{Env, Plan, Program, Versus};
use officium_parse::parse;

const METEOR: &str = include_str!("../../../scores/meteor.off");
const CANTUS: &str = include_str!("../../../scores/meteor_cantus.off");

fn run_full(env: &Env, v: &Versus, arg: Value) -> Result<officium_core::VerseOutcome, Dissonance> {
    let mut r = run_verse(env, v.mode, &v.body, arg, empty_state(), 500);
    loop {
        match r {
            Err(Dissonance::OutOfFuel(cont)) => r = resume(env, *cont, 500),
            other => return other,
        }
    }
}

/// correctio -> consilium -> executio; returns the committed Plan,
/// or the Silent dissonance if correctio cadenced nihil.
fn pipeline(prog: &Program, body: Value) -> Result<Plan, Dissonance> {
    let env = Env::from_fugues(&prog.fugues);
    let out = run_full(&env, prog.verse("correctio").unwrap(), body)?;
    let field = out.value.expect("correctio must cadence with a value");
    let out = run_full(&env, prog.verse("consilium").unwrap(), field)?;
    let out = run_full(&env, prog.verse("executio").unwrap(), Value::Plan(out.plan))?;
    Ok(out.committed.expect("executio must commit"))
}

#[test]
fn sung_pipeline_commits_the_same_plan_as_the_plain_one() {
    let plain = parse(METEOR).unwrap();
    let sung = parse(CANTUS).unwrap();

    for (id, genus, mass, dist) in
        [(1, "asteroides", 1e20, 1e5), (2, "cometes", 5e19, 2e5)]
    {
        let p = pipeline(&plain, make_corpus(id, genus, mass, dist)).unwrap();
        let s = pipeline(&sung, make_corpus(id, genus, mass, dist)).unwrap();
        assert_eq!(p.0.len(), s.0.len());
        assert_eq!(p.0[0].target, s.0[0].target, "{genus}");
        assert_eq!(p.0[0].dv.x, s.0[0].dv.x, "{genus}");
    }
}

#[test]
fn sung_correctio_still_falls_silent_for_a_safe_body() {
    let sung = parse(CANTUS).unwrap();
    let env = Env::from_fugues(&sung.fugues);
    let pebble = make_corpus(3, "asteroides", 1.0, 1e9);
    let r = run_full(&env, sung.verse("correctio").unwrap(), pebble);
    assert!(matches!(r, Err(Dissonance::Silent)), "got {r:?}");
}

#[test]
fn sung_score_renders_and_cadences_to_the_tonic() {
    let sung = parse(CANTUS).unwrap();
    let events = officium_audio::render_program(&sung);
    assert!(!events.is_empty(), "the sung office must be audible");
    for (i, v) in sung.verses.iter().enumerate() {
        let voice = 2 + i as u8;
        assert!(
            events.iter().any(|n| n.voice == voice),
            "verse {} is silent",
            v.name
        );
    }
}
