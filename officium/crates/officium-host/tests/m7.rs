//! M7 exit criteria (§7.4): rhyme = unification. The checker accepts
//! both reference scores, refuses a rhyme that does not unify, and
//! catches type/mode clashes statically — before a note sounds.

use officium_core::check_program;
use officium_core::types::Dissonance;
use officium_parse::parse;

const METEOR: &str = include_str!("../../../scores/meteor.off");
const CANTUS: &str = include_str!("../../../scores/meteor_cantus.off");

fn refused(src: &str) -> String {
    let prog = parse(src).expect("score must parse; the checker is under test");
    match check_program(&prog) {
        Err(Dissonance::Discors { versus, msg }) => format!("{versus}: {msg}"),
        other => panic!("expected Discors, got {other:?}"),
    }
}

#[test]
fn both_reference_scores_are_consonant() {
    for (name, src) in [("meteor", METEOR), ("cantus", CANTUS)] {
        let prog = parse(src).unwrap();
        if let Err(d) = check_program(&prog) {
            panic!("{name} must typecheck, got {d:?}");
        }
    }
}

#[test]
fn a_rhyme_that_does_not_unify_is_discors() {
    // A-lines rhyme a num with the corpus record
    let src = r#"
fuga gravitas in Sol { pedale G = 6.674e-11 }
versus x in Re {
  A "corpus advenit" ; corpus <- arg
  B "pondus sonat"   ; g <- ask "G"
  A "iterum pondus"  ; h <- ask "G"
  B "pondus iterum"  ; i <- ask "G"
  C "quintum verbum" ; j <- pure (dist corpus)
  C "sextum verbum"  ; k <- pure (dist corpus)
  D "septimum"       ; l <- pure 7
  D "octavum"        ; amen l
}
"#;
    let why = refused(src);
    assert!(why.contains("rhyme on 'A'"), "got: {why}");
}

#[test]
fn couplet_carries_the_result_type() {
    // line 7 binds a str, line 8 cadences a num: both couplet lines
    // stand for the verse's result, so the bind's own type is free —
    // this must be accepted
    let src = r#"
versus x in Re {
  A "unum"    ; a <- pure 1
  A "alterum" ; b <- pure 2
  B "tertium" ; c <- pure "tres"
  B "quartum" ; d <- pure "quattuor"
  C "quintum" ; e <- pure verum
  C "sextum"  ; f <- pure falsum
  D "septimum"; g <- pure "gloria"
  D "octavum" ; amen (a + b)
}
"#;
    let prog = parse(src).unwrap();
    assert!(check_program(&prog).is_ok());
}

#[test]
fn static_type_clash_is_caught_before_eval() {
    // dist pins the argument as a corpus record; deflect then demands
    // a num of the same value — no body need arrive to see the clash
    let src = r#"
versus x in Re {
  corpus <- arg
  r <- pure (dist corpus)
  amen (deflect corpus)
}
"#;
    let why = refused(src);
    assert!(why.contains("type clash"), "got: {why}");
}

#[test]
fn static_perage_of_a_non_plan_is_caught() {
    let src = r#"
versus x in Sol {
  n <- pure 42
  perage n
}
"#;
    let why = refused(src);
    assert!(why.contains("type clash"), "got: {why}");
}

#[test]
fn static_wrong_mode_is_caught() {
    // pone is Tritus; a Dorian verse may not touch the state
    let src = r#"
versus x in Re {
  pone 1
  amen 2
}
"#;
    let why = refused(src);
    assert!(why.contains("illegal"), "got: {why}");
}

#[test]
fn static_unbound_name_is_caught() {
    let src = "versus x in Re { amen (nusquam 1) }";
    let why = refused(src);
    assert!(why.contains("nusquam"), "got: {why}");
}

#[test]
fn resolve_types_every_overload_at_the_call_site() {
    // the tonal answer returns a str while the real one returns a
    // function of r — the overloads cannot share a type
    let src = r#"
fuga f in Sol {
  pedale G = 1
  reale  trahere @ asteroides = \r -> r * G
  tonale trahere @ cometes    = "non cantabile"
}
versus x in Re {
  corpus <- arg
  f <- resolve "trahere" (genus corpus)
  amen (f 2)
}
"#;
    let why = refused(src);
    assert!(why.contains("type clash"), "got: {why}");
}

#[test]
fn mitte_of_a_non_command_is_caught() {
    let src = r#"
versus x in sol {
  mitte 42
  amen 1
}
"#;
    let why = refused(src);
    assert!(why.contains("cmd"), "got: {why}");
}

#[test]
fn eq_is_polymorphic_but_its_arguments_must_agree() {
    let ok = parse("versus x in Re { amen (eq 1 2) }").unwrap();
    assert!(check_program(&ok).is_ok());
    let why = refused("versus x in Re { amen (eq 1 \"duo\") }");
    assert!(why.contains("type clash"), "got: {why}");
}
