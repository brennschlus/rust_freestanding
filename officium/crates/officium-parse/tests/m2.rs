//! M2 exit criteria: the meteor score parses; malformed input never
//! panics (always `Dissonance::Parse`); type clashes surface as
//! `Dissonant` at eval time.

use officium_core::builtins::make_corpus;
use officium_core::machine::{empty_state, run_verse};
use officium_core::types::{Dissonance, Mode, Value};
use officium_core::Env;
use officium_parse::parse;

const METEOR: &str = include_str!("../../../scores/meteor.off");

#[test]
fn meteor_score_parses() {
    let prog = parse(METEOR).expect("meteor score must parse");
    assert_eq!(prog.fugues.len(), 1);
    assert_eq!(prog.fugues[0].name, "gravitas");
    assert!(prog.fugues[0].subject.is_some());
    assert_eq!(prog.fugues[0].real.len(), 1);
    assert_eq!(prog.fugues[0].tonal.len(), 1);
    assert_eq!(prog.fugues[0].contra.len(), 1);
    assert_eq!(prog.verses.len(), 3);
    assert_eq!(prog.verse("correctio").unwrap().mode, Mode::Dorian);
    assert_eq!(prog.verse("consilium").unwrap().mode, Mode::Hypomixolydian);
    assert_eq!(prog.verse("executio").unwrap().mode, Mode::Mixolydian);
}

#[test]
fn malformed_scores_yield_parse_dissonance() {
    let bad = [
        "fuga {",
        "versus x in Xy { amen 1 }",
        "versus x in Re { amen }",
        "versus x in Re { y <- }",
        "fuga f in Sol { pedale = 3 }",
        "versus x in Re { si verum tunc { amen 1 } }",
        "\"unterminated",
        "versus x in Re { amen 1e+ }",
        "@@@",
    ];
    for src in bad {
        match parse(src) {
            Err(Dissonance::Parse { .. }) => {}
            other => panic!("{src:?} should be a Parse dissonance, got {other:?}"),
        }
    }
}

/// Pseudo-random byte soup + token soup: the parser must never panic.
#[test]
fn fuzz_lite_never_panics() {
    let mut seed: u64 = 0x0FF1C1
        ;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let vocab = [
        "fuga", "versus", "in", "Re", "sol", "{", "}", "(", ")", "=", "@", "<-", "->",
        "\\", "amen", "nihil", "clama", "perage", "si", "tunc", "aliter", "pone",
        "mitte", "pedale", "subiectum", "reale", "tonale", "contra", "ask", "resolve",
        "arg", "lege", "pure", "x", "1.5", "\"s\"", "+", "*", "-", "/", "\n", ";",
    ];
    for _ in 0..20_000 {
        let len = (rand() % 24) as usize;
        let mut src = String::new();
        for _ in 0..len {
            src.push_str(vocab[(rand() % vocab.len() as u64) as usize]);
            src.push(' ');
        }
        let _ = parse(&src); // must not panic; Err is fine
    }
    // raw byte soup as well
    for _ in 0..20_000 {
        let len = (rand() % 40) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (rand() % 128) as u8).collect();
        if let Ok(s) = std::str::from_utf8(&bytes) {
            let _ = parse(s);
        }
    }
}

/// M2 exit: a type clash in a parsed score surfaces as `Dissonant`.
#[test]
fn type_clash_surfaces_as_dissonant() {
    // `mass 5` — mass of a number instead of a record
    let src = "versus rupta in Re {\n x <- pure (mass 5)\n amen x\n}\n";
    let prog = parse(src).unwrap();
    let v = prog.verse("rupta").unwrap();
    let env = Env::default();
    let r = run_verse(&env, v.mode, &v.body, Value::Unit, empty_state(), 10_000);
    assert!(
        matches!(r, Err(Dissonance::Dissonant { want: "record", .. })),
        "got {r:?}"
    );
}

/// The parsed correctio verse actually runs against the parsed fugue.
#[test]
fn parsed_correctio_runs() {
    let prog = parse(METEOR).unwrap();
    let env = Env::from_fugues(&prog.fugues);
    let v = prog.verse("correctio").unwrap();

    // unsafe asteroid -> a Field comes back
    let body = make_corpus(1, "asteroides", 1e20, 1e5);
    let out = run_verse(&env, v.mode, &v.body, body, empty_state(), 1_000_000).unwrap();
    assert!(matches!(out.value, Some(Value::Field(_))), "got {:?}", out.value);

    // safe pebble -> nihil (Silent)
    let pebble = make_corpus(2, "asteroides", 1.0, 1e9);
    let r = run_verse(&env, v.mode, &v.body, pebble, empty_state(), 1_000_000);
    assert!(matches!(r, Err(Dissonance::Silent)));
}
