//! M9 exit criteria: the capability mask is gone — legality is the
//! structure of a genuine transformer tower, and `Lift` is the
//! transformer's lift. Observable consequences: a plagal mode carries
//! two real copies of its effect (two state cells, two throw
//! channels), `lift` reaches the inner one, and the tower has a
//! bottom.

use officium_core::machine::{empty_state, run_verse};
use officium_core::types::{Dissonance, LayerKind, Mode, OpKind, Value};
use officium_core::{b, Env, Expr};

fn num(n: f64) -> Expr {
    Expr::Lit(Value::Num(n))
}

fn run(mode: Mode, body: &Expr) -> Result<officium_core::VerseOutcome, Dissonance> {
    run_verse(&Env::default(), mode, body, Value::Unit, empty_state(), 100_000)
}

fn bind(m: Expr, x: &str, k: Expr) -> Expr {
    Expr::Bind(Mode::Hypolydian, b(m), x.into(), b(k))
}

#[test]
fn the_towers_reproduce_the_old_mask_exactly() {
    use LayerKind::*;
    let expect = [
        (Mode::Dorian, &[Maybe][..]),
        (Mode::Hypodorian, &[Maybe, Maybe][..]),
        (Mode::Phrygian, &[Except][..]),
        (Mode::Hypophrygian, &[Except, Except][..]),
        (Mode::Lydian, &[State][..]),
        (Mode::Hypolydian, &[State, State][..]),
        (Mode::Mixolydian, &[Commit][..]),
        (Mode::Hypomixolydian, &[Writer][..]),
    ];
    for (mode, tower) in expect {
        assert_eq!(mode.tower(), tower, "{mode:?}");
        // lift is legal exactly where there is a layer beneath
        assert_eq!(mode.allows(OpKind::Lift), tower.len() > 1, "{mode:?}");
    }
}

#[test]
fn hypolydian_has_two_state_cells() {
    // pone 5; lift (pone 9); a <- lift lege; c <- lege; amen (c - a)
    let body = bind(
        Expr::Pone(b(num(5.0))),
        "_",
        bind(
            Expr::Lift(b(Expr::Pone(b(num(9.0))))),
            "_",
            bind(
                Expr::Lift(b(Expr::Lege)),
                "a",
                bind(
                    Expr::Lege,
                    "c",
                    Expr::Return(
                        Mode::Hypolydian,
                        b(Expr::App(
                            b(Expr::App(
                                b(Expr::Var("sub".into())),
                                b(Expr::Var("c".into())),
                            )),
                            b(Expr::Var("a".into())),
                        )),
                    ),
                ),
            ),
        ),
    );
    let out = run(Mode::Hypolydian, &body).unwrap();
    // the outer cell (5) was untouched by the lifted pone (9)
    assert_eq!(out.value, Some(Value::Num(5.0 - 9.0)));
    // the verse's visible state is the outer cell
    assert_eq!(out.state, Value::Num(5.0));
}

#[test]
fn the_inner_state_cell_starts_empty() {
    let body = bind(
        Expr::Pone(b(num(5.0))),
        "_",
        bind(Expr::Lift(b(Expr::Lege)), "a", Expr::Return(Mode::Hypolydian, b(Expr::Var("a".into())))),
    );
    let out = run(Mode::Hypolydian, &body).unwrap();
    assert_eq!(out.value, Some(empty_state()));
}

#[test]
fn an_inner_clama_sails_past_an_outer_recipe() {
    // recipe (lift (clama "deep")) x -> "caught": the throw happens on
    // the inner Except layer; the outer handler must NOT see it
    let body = Expr::Return(
        Mode::Hypophrygian,
        b(Expr::Recipe(
            b(Expr::Lift(b(Expr::Clama(b(Expr::Lit(Value::Str("deep".into()))))))),
            "x".into(),
            b(Expr::Lit(Value::Str("caught".into()))),
        )),
    );
    let r = run(Mode::Hypophrygian, &body);
    assert!(
        matches!(&r, Err(Dissonance::User(Value::Str(s))) if s == "deep"),
        "got {r:?}"
    );
}

#[test]
fn a_same_layer_clama_is_still_caught() {
    let body = Expr::Return(
        Mode::Hypophrygian,
        b(Expr::Recipe(
            b(Expr::Clama(b(Expr::Lit(Value::Str("shallow".into()))))),
            "x".into(),
            b(Expr::Var("x".into())),
        )),
    );
    let out = run(Mode::Hypophrygian, &body).unwrap();
    assert_eq!(out.value, Some(Value::Str("shallow".into())));
}

#[test]
fn a_lifted_recipe_catches_a_lifted_clama() {
    // both the handler and the throw live on the inner layer
    let body = Expr::Return(
        Mode::Hypophrygian,
        b(Expr::Lift(b(Expr::Recipe(
            b(Expr::Clama(b(Expr::Lit(Value::Str("inner".into()))))),
            "x".into(),
            b(Expr::Var("x".into())),
        )))),
    );
    let out = run(Mode::Hypophrygian, &body).unwrap();
    assert_eq!(out.value, Some(Value::Str("inner".into())));
}

#[test]
fn the_tower_has_a_bottom() {
    // lift within lift in a two-layer mode: nothing further down
    let body = Expr::Return(
        Mode::Hypodorian,
        b(Expr::Lift(b(Expr::Lift(b(num(1.0)))))),
    );
    let r = run(Mode::Hypodorian, &body);
    assert!(
        matches!(r, Err(Dissonance::WrongMode { op: OpKind::Lift, .. })),
        "got {r:?}"
    );
}

#[test]
fn nihil_from_the_depths_is_still_silent() {
    let body = Expr::Return(Mode::Hypodorian, b(Expr::Lift(b(Expr::Nihil))));
    let r = run(Mode::Hypodorian, &body);
    assert!(matches!(r, Err(Dissonance::Silent)), "got {r:?}");
}
