//! M6 exit criteria (§7.3): the liturgical eight-line form lowers to
//! the same plain form — the sung meteor score parses, its rhyme
//! scheme is recorded, and malformed liturgy is refused with a Parse
//! dissonance, never a panic.

use officium_core::types::Dissonance;
use officium_parse::parse;

const CANTUS: &str = include_str!("../../../scores/meteor_cantus.off");

#[test]
fn sung_meteor_parses_with_rhyme_scheme() {
    let prog = parse(CANTUS).expect("the sung score must parse");
    assert_eq!(prog.fugues.len(), 1);
    assert_eq!(prog.verses.len(), 3);
    for v in &prog.verses {
        assert_eq!(v.rhymes.len(), 8, "verse {} is not eight lines", v.name);
        let scheme: String = v.rhymes.iter().map(|(l, _)| *l).collect();
        assert_eq!(scheme, "ABABABCC", "verse {} rhyme scheme", v.name);
        assert!(
            v.rhymes.iter().all(|(_, text)| !text.is_empty()),
            "verse {} has a silent line",
            v.name
        );
    }
}

#[test]
fn plain_verses_carry_no_rhymes() {
    let prog = parse("versus x in Re { amen 1 }").unwrap();
    assert!(prog.verse("x").unwrap().rhymes.is_empty());
}

#[test]
fn plain_and_sung_verses_coexist() {
    let src = r#"
versus planus in Re { amen 1 }
versus cantatus in Re {
  A "prima" ; a <- pure 1
  B "secunda" ; b <- pure 2
  A "tertia" ; c <- pure 3
  B "quarta" ; d <- pure 4
  A "quinta" ; e <- pure 5
  B "sexta" ; f <- pure 6
  C "septima" ; g <- pure 7
  C "octava" ; amen g
}
"#;
    let prog = parse(src).unwrap();
    assert!(prog.verse("planus").unwrap().rhymes.is_empty());
    assert_eq!(prog.verse("cantatus").unwrap().rhymes.len(), 8);
}

#[test]
fn malformed_liturgy_is_refused() {
    let seven_lines = r#"
versus x in Re {
  A "una" ; a <- pure 1
  B "duae" ; b <- pure 2
  A "tres" ; c <- pure 3
  B "quattuor" ; d <- pure 4
  A "quinque" ; e <- pure 5
  C "sex" ; f <- pure 6
  C "septem" ; amen f
}
"#;
    let broken_couplet = r#"
versus x in Re {
  A "una" ; a <- pure 1
  B "duae" ; b <- pure 2
  A "tres" ; c <- pure 3
  B "quattuor" ; d <- pure 4
  A "quinque" ; e <- pure 5
  B "sex" ; f <- pure 6
  C "septem" ; g <- pure 7
  D "octo" ; amen g
}
"#;
    let unanswered_rhyme = r#"
versus x in Re {
  A "una" ; a <- pure 1
  B "duae" ; b <- pure 2
  A "tres" ; c <- pure 3
  B "quattuor" ; d <- pure 4
  A "quinque" ; e <- pure 5
  X "sex" ; f <- pure 6
  C "septem" ; g <- pure 7
  C "octo" ; amen g
}
"#;
    // first line omits ';' — the text swallows into application
    let missing_semi = r#"
versus x in Re {
  A "una" a <- pure 1
  B "duae" ; amen 2
}
"#;
    for (what, src) in [
        ("seven lines", seven_lines),
        ("broken couplet", broken_couplet),
        ("unanswered rhyme", unanswered_rhyme),
        ("missing semi", missing_semi),
    ] {
        match parse(src) {
            Err(Dissonance::Parse { .. }) => {}
            other => panic!("{what} should be a Parse dissonance, got {other:?}"),
        }
    }
}

#[test]
fn plain_form_semicolon_comments_are_untouched() {
    // `;` after anything but a rhyme-label head is still a comment —
    // including a second `;` on a sung line
    let src = r#"
versus x in Re {   ; a comment with "a string" ; in it
  A "una" ; a <- pure 1 ; trailing comment on a sung line
  B "duae" ; b <- pure 2
  A "tres" ; c <- pure 3
  B "quattuor" ; d <- pure 4
  A "quinque" ; e <- pure 5
  B "sex" ; f <- pure 6
  C "septem" ; g <- pure 7
  C "octo" ; amen a
}
"#;
    let prog = parse(src).unwrap();
    assert_eq!(prog.verse("x").unwrap().rhymes.len(), 8);
}
