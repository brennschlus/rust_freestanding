//! M3: the meteor score runs end-to-end to a Plan; the mock sink
//! receives it; ordering/consecration guards fire on broken scores.
//! M4: the meteor fugue+verses render to audible events.

use officium_core::builtins::{make_corpus, SAFE_FORCE, SOLVE_K};
use officium_core::machine::{empty_state, resume, run_verse};
use officium_core::types::{Dissonance, Value};
use officium_core::{Env, Versus};
use officium_parse::parse;

const METEOR: &str = include_str!("../../../scores/meteor.off");

fn run_full(env: &Env, v: &Versus, arg: Value) -> Result<officium_core::VerseOutcome, Dissonance> {
    let mut r = run_verse(env, v.mode, &v.body, arg, empty_state(), 500);
    loop {
        match r {
            Err(Dissonance::OutOfFuel(cont)) => r = resume(env, *cont, 500),
            other => return other,
        }
    }
}

#[test]
fn meteor_pipeline_produces_the_expected_plan() {
    let prog = parse(METEOR).unwrap();
    let env = Env::from_fugues(&prog.fugues);

    let (mass, dist) = (1e20, 1e5);
    let g = 6.674e-11;
    assert!(g * mass / (dist * dist) >= SAFE_FORCE, "test body must be unsafe");

    // correctio: Field with dv = solve(force law, r) = f(r) * SOLVE_K
    let body = make_corpus(1, "asteroides", mass, dist);
    let out = run_full(&env, prog.verse("correctio").unwrap(), body).unwrap();
    let field = match out.value {
        Some(Value::Field(f)) => f,
        other => panic!("expected a Field, got {other:?}"),
    };
    let expected_dv = (g * mass / (dist * dist)) * SOLVE_K;
    assert_eq!(field.dv.x, expected_dv);

    // consilium: pure Plan accumulation via mitte
    let out = run_full(&env, prog.verse("consilium").unwrap(), Value::Field(field)).unwrap();
    assert_eq!(out.plan.0.len(), 1);
    assert_eq!(out.plan.0[0].dv.x, expected_dv);
    assert!(out.committed.is_none(), "consilium must not commit anything");

    // executio: the only impure verse — committed plan comes back
    let out = run_full(&env, prog.verse("executio").unwrap(), Value::Plan(out.plan)).unwrap();
    let committed = out.committed.expect("executio must commit");
    assert_eq!(committed.0.len(), 1);
    assert_eq!(committed.0[0].dv.x, expected_dv);
}

#[test]
fn comet_takes_the_tonal_answer_with_outgassing() {
    let prog = parse(METEOR).unwrap();
    let env = Env::from_fugues(&prog.fugues);
    let (mass, dist) = (5e19, 2e5);
    let g = 6.674e-11;
    let body = make_corpus(2, "cometes", mass, dist);
    let out = run_full(&env, prog.verse("correctio").unwrap(), body).unwrap();
    let field = match out.value {
        Some(Value::Field(f)) => f,
        other => panic!("expected a Field, got {other:?}"),
    };
    // tonal answer injects the outgassing countersubject (default 0.1)
    let expected_dv = (g * mass / (dist * dist) + 0.1) * SOLVE_K;
    assert_eq!(field.dv.x, expected_dv);
}

#[test]
fn premature_commit_is_rejected_loudly() {
    // running executio on a plan nobody computed: empty plan -> Premature
    let prog = parse(METEOR).unwrap();
    let env = Env::from_fugues(&prog.fugues);
    let r = run_full(
        &env,
        prog.verse("executio").unwrap(),
        Value::Plan(officium_core::Plan::empty()),
    );
    assert!(matches!(r, Err(Dissonance::Premature)), "got {r:?}");
}

#[test]
fn unconsecrated_commit_is_rejected() {
    // committing a Field (not a Plan): the value was never consecrated
    let prog = parse(METEOR).unwrap();
    let env = Env::from_fugues(&prog.fugues);
    let body = make_corpus(1, "asteroides", 1e20, 1e5);
    let out = run_full(&env, prog.verse("correctio").unwrap(), body).unwrap();
    let r = run_full(&env, prog.verse("executio").unwrap(), out.value.unwrap());
    assert!(matches!(r, Err(Dissonance::Unconsecrated)), "got {r:?}");
}

#[test]
fn perage_outside_mixolydian_is_wrong_mode() {
    // a deliberately broken score: hypomixolydian tries to commit
    let src = "versus impius in sol {\n plan <- arg\n perage plan\n}\n";
    let prog = parse(src).unwrap();
    let env = Env::default();
    let r = run_full(
        &env,
        prog.verse("impius").unwrap(),
        Value::Plan(officium_core::Plan::empty()),
    );
    assert!(matches!(r, Err(Dissonance::WrongMode { .. })), "got {r:?}");
}

#[test]
fn unknown_genus_is_unresolved() {
    let prog = parse(METEOR).unwrap();
    let env = Env::from_fugues(&prog.fugues);
    let body = make_corpus(9, "luna", 1e20, 1e5);
    let r = run_full(&env, prog.verse("correctio").unwrap(), body);
    assert!(matches!(r, Err(Dissonance::Unresolved { .. })), "got {r:?}");
}

// --- M4: the office can be heard ---

#[test]
fn meteor_score_renders_and_plays() {
    let prog = parse(METEOR).unwrap();
    let events = officium_audio::render_program(&prog);
    assert!(!events.is_empty(), "the office must be audible");

    // the pedal voice holds under the subject
    assert!(events.iter().any(|n| n.voice == 0), "no pedal point");
    // each verse sings on its own voice and ends on its final
    for (i, v) in prog.verses.iter().enumerate() {
        let voice = 2 + i as u8;
        let notes: Vec<_> = events.iter().filter(|n| n.voice == voice).collect();
        assert!(!notes.is_empty(), "verse {} is silent", v.name);
        let last = notes.last().unwrap();
        let tonic = officium_audio::render_verse(&v.body, v.mode, voice)
            .last()
            .unwrap()
            .pitch;
        assert_eq!(last.pitch, tonic, "verse {} must cadence to its tonic", v.name);
    }

    let mut organ = officium_audio::mock_organ();
    officium_audio::play(&events, &mut organ);
    let ons = organ.log.iter().filter(|l| l.contains("ON")).count();
    let offs = organ.log.iter().filter(|l| l.contains("OFF")).count();
    assert_eq!(ons, events.len());
    assert_eq!(offs, events.len(), "every note must be released");
}

#[test]
fn pitch_hz_sanity() {
    assert_eq!(officium_audio::pitch_hz(69), 440); // A4
    assert_eq!(officium_audio::pitch_hz(60), 262); // C4 (rounded)
    assert_eq!(officium_audio::pitch_hz(67), 392); // G4
}
