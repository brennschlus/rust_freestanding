//! M1 exit criteria: factorial-via-Fix+State runs; monad laws pass;
//! the capability mask and purity guards fire; OutOfFuel is resumable.

use officium_core::machine::{empty_state, resume, run_verse};
use officium_core::types::{Dissonance, GravityCmd, Mode, OpKind, Plan, Value, Vec3};
use officium_core::{b, Env, Expr};

fn var(s: &str) -> Expr {
    Expr::Var(s.into())
}
fn num(n: f64) -> Expr {
    Expr::Lit(Value::Num(n))
}
fn app(f: Expr, a: Expr) -> Expr {
    Expr::App(b(f), b(a))
}
fn app2(f: Expr, a: Expr, c: Expr) -> Expr {
    app(app(f, a), c)
}
fn lam(p: &str, body: Expr) -> Expr {
    Expr::Lam(p.into(), b(body))
}

fn run(mode: Mode, body: &Expr, arg: Value) -> Result<(Option<Value>, Value, Plan), Dissonance> {
    let env = Env::default();
    run_verse(&env, mode, body, arg, empty_state(), 1_000_000)
        .map(|o| (o.value, o.state, o.plan))
}

/// factorial = fix (\self n -> if n <= 1 then 1 else n * self (n - 1))
fn factorial_expr() -> Expr {
    Expr::Fix(b(lam(
        "self",
        lam(
            "n",
            Expr::If(
                b(app2(var("le"), var("n"), num(1.0))),
                b(num(1.0)),
                b(app2(
                    var("mul"),
                    var("n"),
                    app(var("self"), app2(var("sub"), var("n"), num(1.0))),
                )),
            ),
        ),
    )))
}

#[test]
fn factorial_via_fix() {
    let body = Expr::Return(Mode::Lydian, b(app(factorial_expr(), Expr::Arg)));
    let (v, _, _) = run(Mode::Lydian, &body, Value::Num(10.0)).unwrap();
    assert_eq!(v, Some(Value::Num(3628800.0)));
}

/// The State leg of the TC witness: count down `arg` in the state cell,
/// multiplying an accumulator — factorial through lege/pone, recursion
/// through Fix. Unbounded memory + fixpoint = Turing complete.
#[test]
fn factorial_via_fix_and_state() {
    // state = Record is the default; here we just overwrite the whole
    // cell with a Num accumulator via pone
    // loop = fix (\self n -> if n <= 1 then lege else
    //                        bind (lege) acc (bind (pone (acc*n)) _ (self (n-1))))
    let loop_ = Expr::Fix(b(lam(
        "self",
        lam(
            "n",
            Expr::If(
                b(app2(var("le"), var("n"), num(1.0))),
                b(Expr::Lege),
                b(Expr::Bind(
                    Mode::Lydian,
                    b(Expr::Lege),
                    "acc".into(),
                    b(Expr::Bind(
                        Mode::Lydian,
                        b(Expr::Pone(b(app2(var("mul"), var("acc"), var("n"))))),
                        "_".into(),
                        b(app(var("self"), app2(var("sub"), var("n"), num(1.0)))),
                    )),
                )),
            ),
        ),
    )));
    let body = Expr::Bind(
        Mode::Lydian,
        b(Expr::Pone(b(num(1.0)))),
        "_".into(),
        b(Expr::Return(Mode::Lydian, b(app(loop_, Expr::Arg)))),
    );
    let (v, st, _) = run(Mode::Lydian, &body, Value::Num(6.0)).unwrap();
    assert_eq!(v, Some(Value::Num(720.0)));
    assert_eq!(st, Value::Num(720.0));
}

// --- monad laws ("the liturgy must be lawful") ---
// Hand-rolled property loop over a pool of values and Kleisli arrows;
// equality = (value, state, plan) triple equality.

fn kleisli_pool() -> Vec<Expr> {
    vec![
        // f x = pure (x + 1)
        lam("x", Expr::Pure(Mode::Lydian, b(app2(var("add"), var("x"), num(1.0))))),
        // f x = pone x; lege  (writes then reads back)
        lam(
            "x",
            Expr::Bind(
                Mode::Lydian,
                b(Expr::Pone(b(var("x")))),
                "_".into(),
                b(Expr::Lege),
            ),
        ),
        // f x = pure (x * x)
        lam("x", Expr::Pure(Mode::Lydian, b(app2(var("mul"), var("x"), var("x"))))),
    ]
}

fn observe(body: Expr) -> (Option<Value>, Value, Plan) {
    run(Mode::Lydian, &body, Value::Unit).unwrap()
}

fn bind_to(m: Expr, k_var: &str, k: Expr) -> Expr {
    // m >>= \x. k x   written as Bind(m, x, App(k, x))
    Expr::Bind(Mode::Lydian, b(m), k_var.into(), b(app(k, var(k_var))))
}

#[test]
fn monad_law_left_identity() {
    // pure a >>= f  ≡  f a
    for a in [0.0, 2.5, -7.0] {
        for f in kleisli_pool() {
            let lhs = observe(bind_to(
                Expr::Pure(Mode::Lydian, b(num(a))),
                "x",
                f.clone(),
            ));
            let rhs = observe(app(f, num(a)));
            assert_eq!(lhs, rhs, "left identity failed for a={a}");
        }
    }
}

#[test]
fn monad_law_right_identity() {
    // m >>= pure  ≡  m
    let ms = vec![
        Expr::Pure(Mode::Lydian, b(num(42.0))),
        Expr::Bind(
            Mode::Lydian,
            b(Expr::Pone(b(num(3.0)))),
            "_".into(),
            b(Expr::Lege),
        ),
    ];
    for m in ms {
        let lhs = observe(Expr::Bind(
            Mode::Lydian,
            b(m.clone()),
            "x".into(),
            b(Expr::Pure(Mode::Lydian, b(var("x")))),
        ));
        let rhs = observe(m);
        assert_eq!(lhs, rhs, "right identity failed");
    }
}

#[test]
fn monad_law_associativity() {
    // (m >>= f) >>= g  ≡  m >>= (\x -> f x >>= g)
    let m = Expr::Pure(Mode::Lydian, b(num(5.0)));
    for f in kleisli_pool() {
        for g in kleisli_pool() {
            let lhs = observe(bind_to(bind_to(m.clone(), "x", f.clone()), "y", g.clone()));
            let rhs = observe(Expr::Bind(
                Mode::Lydian,
                b(m.clone()),
                "x".into(),
                b(bind_to(app(f.clone(), var("x")), "y", g.clone())),
            ));
            assert_eq!(lhs, rhs, "associativity failed");
        }
    }
}

// --- capability mask: each mode rejects out-of-set ops ---

#[test]
fn capability_mask_rejects_wrong_mode() {
    let cases: Vec<(Expr, OpKind)> = vec![
        (Expr::Nihil, OpKind::Nihil),
        (Expr::Clama(b(num(1.0))), OpKind::Clama),
        (Expr::Lege, OpKind::Lege),
        (Expr::Pone(b(num(1.0))), OpKind::Pone),
        (Expr::Mitte(b(num(1.0))), OpKind::Mitte),
        (Expr::Perage(b(num(1.0))), OpKind::Perage),
        (Expr::Lift(b(num(1.0))), OpKind::Lift),
    ];
    let modes = [
        Mode::Dorian,
        Mode::Hypodorian,
        Mode::Phrygian,
        Mode::Hypophrygian,
        Mode::Lydian,
        Mode::Hypolydian,
        Mode::Mixolydian,
        Mode::Hypomixolydian,
    ];
    for mode in modes {
        for (expr, op) in &cases {
            let r = run(mode, expr, Value::Unit);
            if mode.allows(*op) {
                if let Err(d) = &r {
                    assert!(
                        !matches!(d, Dissonance::WrongMode { .. }),
                        "{mode:?} should allow {op:?}, got {d:?}"
                    );
                }
            } else {
                assert!(
                    matches!(r, Err(Dissonance::WrongMode { .. })),
                    "{mode:?} must reject {op:?}"
                );
            }
        }
    }
}

// --- Protus / Deuterus semantics ---

#[test]
fn nihil_short_circuits_binds() {
    // x <- pure 1; _ <- nihil; amen x   -> Silent, never reaches amen
    let body = Expr::Bind(
        Mode::Dorian,
        b(Expr::Pure(Mode::Dorian, b(num(1.0)))),
        "x".into(),
        b(Expr::Bind(
            Mode::Dorian,
            b(Expr::Nihil),
            "_".into(),
            b(Expr::Return(Mode::Dorian, b(var("x")))),
        )),
    );
    assert!(matches!(run(Mode::Dorian, &body, Value::Unit), Err(Dissonance::Silent)));
}

#[test]
fn clama_caught_by_recipe() {
    // recipe (clama 13) e (amen e)  -> 13
    let body = Expr::Recipe(
        b(Expr::Clama(b(num(13.0)))),
        "e".into(),
        b(Expr::Return(Mode::Phrygian, b(var("e")))),
    );
    let (v, _, _) = run(Mode::Phrygian, &body, Value::Unit).unwrap();
    assert_eq!(v, Some(Value::Num(13.0)));
}

#[test]
fn uncaught_clama_surfaces_as_user() {
    let body = Expr::Clama(b(num(4.0)));
    assert!(matches!(
        run(Mode::Phrygian, &body, Value::Unit),
        Err(Dissonance::User(Value::Num(n))) if n == 4.0
    ));
}

// --- purity guards ---

#[test]
fn perage_of_non_plan_is_unconsecrated() {
    let body = Expr::Perage(b(num(1.0)));
    assert!(matches!(
        run(Mode::Mixolydian, &body, Value::Unit),
        Err(Dissonance::Unconsecrated)
    ));
}

#[test]
fn perage_of_empty_plan_is_premature() {
    let body = Expr::Perage(b(Expr::Lit(Value::Plan(Plan::empty()))));
    assert!(matches!(
        run(Mode::Mixolydian, &body, Value::Unit),
        Err(Dissonance::Premature)
    ));
}

#[test]
fn perage_of_computed_plan_commits() {
    let cmd = GravityCmd { target: 7, dv: Vec3::along_x(0.5), at: 0 };
    let body = Expr::Perage(b(Expr::Lit(Value::Plan(Plan(vec![cmd.clone()])))));
    let env = Env::default();
    let out = run_verse(&env, Mode::Mixolydian, &body, Value::Unit, empty_state(), 1000).unwrap();
    assert_eq!(out.committed, Some(Plan(vec![cmd])));
}

#[test]
fn mitte_accumulates_plan_purely() {
    let cmd = |t| Expr::Lit(Value::Cmd(GravityCmd { target: t, dv: Vec3::along_x(1.0), at: 0 }));
    let body = Expr::Bind(
        Mode::Hypomixolydian,
        b(Expr::Mitte(b(cmd(1)))),
        "_".into(),
        b(Expr::Bind(
            Mode::Hypomixolydian,
            b(Expr::Mitte(b(cmd(2)))),
            "_".into(),
            b(Expr::Return(Mode::Hypomixolydian, b(num(0.0)))),
        )),
    );
    let (_, _, plan) = run(Mode::Hypomixolydian, &body, Value::Unit).unwrap();
    assert_eq!(plan.0.len(), 2);
    assert_eq!(plan.0[0].target, 1);
    assert_eq!(plan.0[1].target, 2);
}

// --- fuel: OutOfFuel is a resumable signal, not a failure ---

#[test]
fn out_of_fuel_resumes() {
    let env = Env::default();
    let body = Expr::Return(Mode::Lydian, b(app(factorial_expr(), num(12.0))));
    let mut r = run_verse(&env, Mode::Lydian, &body, Value::Unit, empty_state(), 10);
    let mut hops = 0;
    loop {
        match r {
            Ok(out) => {
                assert_eq!(out.value, Some(Value::Num(479001600.0)));
                break;
            }
            Err(Dissonance::OutOfFuel(cont)) => {
                assert!(!Dissonance::OutOfFuel(cont_placeholder()).is_failure());
                hops += 1;
                assert!(hops < 10_000, "never finished");
                r = resume(&env, *cont, 10);
            }
            Err(d) => panic!("unexpected dissonance: {d:?}"),
        }
    }
    assert!(hops > 0, "budget of 10 must not finish factorial(12)");
}

fn cont_placeholder() -> Box<officium_core::Continuation> {
    // is_failure only looks at the variant; get a real continuation cheaply
    let env = Env::default();
    match run_verse(
        &env,
        Mode::Lydian,
        &Expr::Return(Mode::Lydian, b(app(factorial_expr(), num(20.0)))),
        Value::Unit,
        empty_state(),
        1,
    ) {
        Err(Dissonance::OutOfFuel(c)) => c,
        _ => unreachable!(),
    }
}

/// A fugue that never cadences (planetary defense loops forever) must
/// keep yielding OutOfFuel and never hang: run N slices, still pending.
#[test]
fn per_omnia_saecula_never_cadences_but_always_yields() {
    let env = Env::default();
    // fix (\self x -> self x) — the eternal stretto
    let eternal = app(
        Expr::Fix(b(lam("self", lam("x", app(var("self"), var("x")))))),
        num(0.0),
    );
    let mut r = run_verse(&env, Mode::Lydian, &eternal, Value::Unit, empty_state(), 1000);
    for _ in 0..50 {
        match r {
            Err(Dissonance::OutOfFuel(cont)) => r = resume(&env, *cont, 1000),
            other => panic!("eternal fugue terminated: {other:?}"),
        }
    }
    assert!(matches!(r, Err(Dissonance::OutOfFuel(_))));
}

// --- lift ---

#[test]
fn lift_is_plagal_only_and_transparent() {
    // in Hypolydian: lift (pure 3) is legal and passes the value through
    let body = Expr::Return(
        Mode::Hypolydian,
        b(Expr::Lift(b(Expr::Pure(Mode::Lydian, b(num(3.0)))))),
    );
    let (v, _, _) = run(Mode::Hypolydian, &body, Value::Unit).unwrap();
    assert_eq!(v, Some(Value::Num(3.0)));

    // in plain Lydian the same lift is a WrongMode dissonance
    assert!(matches!(
        run(Mode::Lydian, &Expr::Lift(b(num(1.0))), Value::Unit),
        Err(Dissonance::WrongMode { op: OpKind::Lift, .. })
    ));
}
