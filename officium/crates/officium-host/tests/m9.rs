//! M9, surface side: `leva` is the transformer's lift in score text,
//! and the checker agrees with the tower about where it is legal.

use officium_core::check_program;
use officium_core::machine::{empty_state, run_verse};
use officium_core::types::{Dissonance, Value};
use officium_core::Env;
use officium_parse::parse;

#[test]
fn leva_reaches_the_inner_state_cell() {
    let src = r#"
versus x in fa {
  pone 5
  a <- leva (pone 9)
  b <- leva lege
  c <- lege
  amen (c - b)
}
"#;
    let prog = parse(src).unwrap();
    check_program(&prog).unwrap();
    let v = prog.verse("x").unwrap();
    let out =
        run_verse(&Env::default(), v.mode, &v.body, Value::Unit, empty_state(), 100_000).unwrap();
    // outer cell holds 5, inner holds 9 — two real states
    assert_eq!(out.value, Some(Value::Num(5.0 - 9.0)));
    assert_eq!(out.state, Value::Num(5.0));
}

#[test]
fn leva_in_an_authentic_mode_is_statically_discors() {
    let src = "versus x in Fa {\n  a <- leva lege\n  amen a\n}\n";
    let prog = parse(src).unwrap();
    match check_program(&prog) {
        Err(Dissonance::Discors { versus, msg }) => {
            assert_eq!(versus, "x");
            assert!(msg.contains("Lift"), "got: {msg}");
        }
        other => panic!("expected Discors, got {other:?}"),
    }
}

#[test]
fn leva_within_leva_is_statically_discors() {
    let src = "versus x in fa {\n  a <- leva (leva lege)\n  amen a\n}\n";
    let prog = parse(src).unwrap();
    assert!(matches!(check_program(&prog), Err(Dissonance::Discors { .. })));
}
