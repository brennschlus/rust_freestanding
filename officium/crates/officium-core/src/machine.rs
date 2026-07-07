//! The evaluator: an explicit-stack trampoline (§6.6). No native
//! recursion — kernel stacks are small and a stretto can be deep or
//! infinite. A step budget (fuel, §6.5) makes non-terminating fugues
//! coexist with a live system: when fuel runs out the machine returns a
//! resumable `Continuation` instead of hanging.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::builtins;
use crate::env::Env;
use crate::ir::Expr;
use crate::types::{Dissonance, Mode, OpKind, Plan, Scope, Sym, Value};

/// What one verse run yields (§6.4): the pure result triple plus the
/// committed plan, which the *driver* — not the machine — hands to the
/// platform. `value` is `None` when the verse cadenced with `nihil`.
#[derive(Debug)]
pub struct VerseOutcome {
    pub value: Option<Value>,
    pub state: Value,
    pub plan: Plan,
    /// Set iff the verse cadenced with `perage`.
    pub committed: Option<Plan>,
}

enum Control {
    Eval(Expr, Scope),
    Value(Value),
}

enum Frame {
    /// Function value ready; evaluate the argument next.
    AppArg(Expr, Scope),
    /// Argument under evaluation; apply the saved function afterwards.
    AppCall(Value),
    IfK(Expr, Expr, Scope),
    /// After m completes: bind its value and evaluate k.
    BindK(Sym, Expr, Scope),
    /// Cadence: halt the verse with the computed value.
    ReturnK,
    ClamaK,
    /// Catch frame: on `User` unwind, bind the error and run the handler.
    RecipeH(Sym, Expr, Scope),
    PoneK,
    MitteK,
    PerageK,
    /// Evaluated the genus; resolve the overload in the saved scope.
    ResolveK(Sym, Scope),
    /// Evaluated the fixpoint operand; tie the knot.
    FixK(Expr, Scope),
    /// Restore the mode on leaving a `Lift` body.
    ModeRestore(Mode),
    /// Finish a builtin that had to apply a closure (solve).
    BuiltinPost(builtins::PostOp),
}

/// A paused machine, resumable with more fuel. Opaque on purpose.
pub struct Continuation {
    control: Control,
    stack: Vec<Frame>,
    st: Value,
    plan: Plan,
    mode: Mode,
    arg: Value,
}

/// Fresh named-slot state (§15: `St` is a Record so a verse can hold
/// several state slots without nesting).
pub fn empty_state() -> Value {
    Value::Record(Rc::new(BTreeMap::new()))
}

/// Run a verse body against an environment (§6.4). The machine itself
/// is pure: `perage` only records the committed plan in the outcome.
pub fn run_verse(
    env: &Env,
    mode: Mode,
    body: &Expr,
    arg: Value,
    st: Value,
    fuel: u64,
) -> Result<VerseOutcome, Dissonance> {
    let cont = Continuation {
        control: Control::Eval(body.clone(), Scope::default()),
        stack: Vec::new(),
        st,
        plan: Plan::empty(),
        mode,
        arg,
    };
    resume(env, cont, fuel)
}

/// Resume a paused verse with a fresh fuel budget.
pub fn resume(
    env: &Env,
    mut m: Continuation,
    mut fuel: u64,
) -> Result<VerseOutcome, Dissonance> {
    loop {
        if fuel == 0 {
            return Err(Dissonance::OutOfFuel(Box::new(m)));
        }
        fuel -= 1;

        let control = core::mem::replace(&mut m.control, Control::Value(Value::Unit));
        match control {
            Control::Eval(expr, scope) => match eval_step(env, &mut m, expr, scope) {
                Ok(control) => m.control = control,
                Err(d) => match unwind(&mut m, d) {
                    Ok(control) => m.control = control,
                    Err(d) => return Err(d),
                },
            },
            Control::Value(value) => match m.stack.pop() {
                None => {
                    // the bind chain ran dry without an explicit cadence:
                    // treat the last value as the verse's result
                    return Ok(finish(m.st, m.plan, Some(value), None));
                }
                Some(frame) => match value_step(env, &mut m, frame, value) {
                    Ok(StepOut::Continue(control)) => m.control = control,
                    Ok(StepOut::Halt(outcome)) => return Ok(outcome),
                    Err(d) => match unwind(&mut m, d) {
                        Ok(control) => m.control = control,
                        Err(d) => return Err(d),
                    },
                },
            },
        }
    }
}

fn finish(st: Value, plan: Plan, value: Option<Value>, committed: Option<Plan>) -> VerseOutcome {
    VerseOutcome { value, state: st, plan, committed }
}

/// Propagate a raised dissonance through the stack. `User` errors are
/// caught by the nearest `RecipeH`; everything else aborts the verse —
/// except `Silent`, which the driver reports as "no correction".
fn unwind(m: &mut Continuation, d: Dissonance) -> Result<Control, Dissonance> {
    if matches!(d, Dissonance::User(_)) {
        while let Some(frame) = m.stack.pop() {
            match frame {
                Frame::RecipeH(name, handler, scope) => {
                    let err_value = match d {
                        Dissonance::User(v) => v,
                        _ => unreachable!(),
                    };
                    let scope = scope.bind(name, err_value);
                    return Ok(Control::Eval(handler, scope));
                }
                Frame::ModeRestore(mode) => m.mode = mode,
                _ => {}
            }
        }
        return Err(Dissonance::User(match d {
            Dissonance::User(v) => v,
            _ => unreachable!(),
        }));
    }
    Err(d)
}

fn check(mode: Mode, op: OpKind) -> Result<(), Dissonance> {
    if mode.allows(op) {
        Ok(())
    } else {
        Err(Dissonance::WrongMode { mode, op })
    }
}

/// One step with an expression in control position.
fn eval_step(
    env: &Env,
    m: &mut Continuation,
    expr: Expr,
    scope: Scope,
) -> Result<Control, Dissonance> {
    Ok(match expr {
        Expr::Lit(v) => Control::Value(v),

        Expr::Var(name) => {
            if let Some(v) = scope.lookup(&name) {
                Control::Value(v.clone())
            } else if let Some(v) = builtins::value(&name) {
                Control::Value(v)
            } else if let Ok(v) = env.ask(&name) {
                // pedals are in scope as free variables too: the fugue
                // sounds underneath everything the verse sings
                Control::Value(v)
            } else {
                return Err(Dissonance::UnboundVar { name });
            }
        }

        Expr::Lam(param, body) => Control::Value(Value::Closure {
            param,
            body: Rc::new(*body),
            env: scope,
        }),

        Expr::App(f, a) => {
            m.stack.push(Frame::AppArg(*a, scope.clone()));
            Control::Eval(*f, scope)
        }

        Expr::If(c, t, e) => {
            m.stack.push(Frame::IfK(*t, *e, scope.clone()));
            Control::Eval(*c, scope)
        }

        Expr::Fix(f) => {
            m.stack.push(Frame::FixK((*f).clone(), scope.clone()));
            Control::Eval(*f, scope)
        }

        Expr::Pure(_, e) => Control::Eval(*e, scope),

        Expr::Return(_, e) => {
            m.stack.push(Frame::ReturnK);
            Control::Eval(*e, scope)
        }

        Expr::Bind(_, mval, name, k) => {
            m.stack.push(Frame::BindK(name, *k, scope.clone()));
            Control::Eval(*mval, scope)
        }

        Expr::Arg => Control::Value(m.arg.clone()),

        Expr::Ask(name) => Control::Value(env.ask(&name)?),

        Expr::Resolve(name, genus_expr) => {
            m.stack.push(Frame::ResolveK(name, scope.clone()));
            Control::Eval(*genus_expr, scope)
        }

        Expr::Nihil => {
            check(m.mode, OpKind::Nihil)?;
            return Err(Dissonance::Silent);
        }

        Expr::Clama(e) => {
            check(m.mode, OpKind::Clama)?;
            m.stack.push(Frame::ClamaK);
            Control::Eval(*e, scope)
        }

        Expr::Recipe(body, name, handler) => {
            check(m.mode, OpKind::Recipe)?;
            m.stack.push(Frame::RecipeH(name, *handler, scope.clone()));
            Control::Eval(*body, scope)
        }

        Expr::Lege => {
            check(m.mode, OpKind::Lege)?;
            Control::Value(m.st.clone())
        }

        Expr::Pone(e) => {
            check(m.mode, OpKind::Pone)?;
            m.stack.push(Frame::PoneK);
            Control::Eval(*e, scope)
        }

        Expr::Mitte(e) => {
            check(m.mode, OpKind::Mitte)?;
            m.stack.push(Frame::MitteK);
            Control::Eval(*e, scope)
        }

        Expr::Perage(e) => {
            check(m.mode, OpKind::Perage)?;
            m.stack.push(Frame::PerageK);
            Control::Eval(*e, scope)
        }

        Expr::Lift(e) => {
            check(m.mode, OpKind::Lift)?;
            m.stack.push(Frame::ModeRestore(m.mode));
            // v1: the enclosed computation runs as the base (authentic)
            // mode of the same final; the value passes through unchanged
            m.mode = Mode::from_final(m.mode.final_(), false);
            Control::Eval(*e, scope)
        }
    })
}

enum StepOut {
    Continue(Control),
    Halt(VerseOutcome),
}

/// One step with a computed value meeting the top stack frame.
fn value_step(
    env: &Env,
    m: &mut Continuation,
    frame: Frame,
    value: Value,
) -> Result<StepOut, Dissonance> {
    use StepOut::*;
    Ok(match frame {
        Frame::AppArg(arg_expr, scope) => {
            m.stack.push(Frame::AppCall(value));
            Continue(Control::Eval(arg_expr, scope))
        }

        Frame::AppCall(fun) => Continue(apply(m, fun, value)?),

        Frame::IfK(t, e, scope) => match value {
            Value::Bool(true) => Continue(Control::Eval(t, scope)),
            Value::Bool(false) => Continue(Control::Eval(e, scope)),
            other => {
                return Err(Dissonance::Dissonant { want: "bool", got: other.ty() })
            }
        },

        Frame::BindK(name, k, scope) => {
            let scope = scope.bind(name, value);
            Continue(Control::Eval(k, scope))
        }

        Frame::ReturnK => {
            // Amen: cadence to the tonic halts the whole verse
            let st = core::mem::replace(&mut m.st, Value::Unit);
            let plan = core::mem::take(&mut m.plan);
            Halt(finish(st, plan, Some(value), None))
        }

        Frame::ClamaK => return Err(Dissonance::User(value)),

        // the guarded computation finished without clamare: drop the handler
        Frame::RecipeH(_, _, _) => Continue(Control::Value(value)),

        Frame::PoneK => {
            m.st = value;
            Continue(Control::Value(Value::Unit))
        }

        Frame::MitteK => match value {
            Value::Cmd(cmd) => {
                m.plan.0.push(cmd);
                Continue(Control::Value(Value::Unit))
            }
            Value::Plan(p) => {
                m.plan.append(p);
                Continue(Control::Value(Value::Unit))
            }
            other => return Err(Dissonance::Dissonant { want: "cmd", got: other.ty() }),
        },

        Frame::PerageK => match value {
            Value::Plan(p) => {
                if p.0.is_empty() {
                    // committing before the field was computed: the plan
                    // is empty — the real mass would jerk on garbage
                    return Err(Dissonance::Premature);
                }
                let st = core::mem::replace(&mut m.st, Value::Unit);
                let plan = core::mem::take(&mut m.plan);
                Halt(finish(st, plan, Some(Value::Unit), Some(p)))
            }
            other => {
                let _ = other;
                return Err(Dissonance::Unconsecrated);
            }
        },

        Frame::ResolveK(name, scope) => match value {
            Value::Genus(g) => {
                let overload = env.resolve(&name, &g)?.clone();
                // evaluate the overload in the caller's scope: the
                // subject parameter and pedals are its free variables
                Continue(Control::Eval(overload, scope))
            }
            other => {
                return Err(Dissonance::Dissonant { want: "genus", got: other.ty() })
            }
        },

        Frame::FixK(f_expr, scope) => {
            // fix f = f (\x -> (fix f) x), unrolled lazily: the argument
            // is a closure that re-enters the fixpoint when applied
            let self_ref = Value::Closure {
                param: String::from("$stretto"),
                body: Rc::new(Expr::App(
                    Box::new(Expr::Fix(Box::new(f_expr))),
                    Box::new(Expr::Var(String::from("$stretto"))),
                )),
                env: scope,
            };
            Continue(apply(m, value, self_ref)?)
        }

        Frame::ModeRestore(mode) => {
            m.mode = mode;
            Continue(Control::Value(value))
        }

        Frame::BuiltinPost(post) => {
            Continue(Control::Value(builtins::apply_post(post, value)?))
        }
    })
}

/// Apply a function value to an argument value.
fn apply(m: &mut Continuation, fun: Value, arg: Value) -> Result<Control, Dissonance> {
    match fun {
        Value::Closure { param, body, env } => {
            let scope = env.bind(param, arg);
            Ok(Control::Eval((*body).clone(), scope))
        }
        Value::Builtin { name, mut args } => {
            args.push(arg);
            let want = builtins::arity(&name).unwrap_or(0);
            if args.len() < want {
                return Ok(Control::Value(Value::Builtin { name, args }));
            }
            match builtins::call(&name, &args)? {
                builtins::BuiltinResult::Done(v) => Ok(Control::Value(v)),
                builtins::BuiltinResult::Apply { f, arg, post } => {
                    m.stack.push(Frame::BuiltinPost(post));
                    apply(m, f, arg)
                }
            }
        }
        other => Err(Dissonance::Dissonant { want: "closure", got: other.ty() }),
    }
}
