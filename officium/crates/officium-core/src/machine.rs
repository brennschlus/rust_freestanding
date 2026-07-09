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
use crate::types::{Dissonance, LayerKind, Mode, OpKind, Plan, Scope, Sym, Value};

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

/// One layer of the verse's monad stack (M9), carrying its effect
/// state. Index 0 is the top of the tower; ops bind to the topmost
/// layer of their kind at or below the current `Lift` depth.
enum Layer {
    Maybe,
    Except,
    State(Value),
    Writer(Plan),
    Commit,
}

impl Layer {
    fn kind(&self) -> LayerKind {
        match self {
            Layer::Maybe => LayerKind::Maybe,
            Layer::Except => LayerKind::Except,
            Layer::State(_) => LayerKind::State,
            Layer::Writer(_) => LayerKind::Writer,
            Layer::Commit => LayerKind::Commit,
        }
    }
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
    /// Throw on the Except layer at this index.
    ClamaK(usize),
    /// Catch frame: on `User` unwind from the *same* Except layer,
    /// bind the error and run the handler. A `clama` thrown on an
    /// inner (lifted) layer sails past outer handlers — transformer
    /// semantics, not a mask.
    RecipeH(Sym, Expr, Scope, usize),
    /// Write the State layer at this index.
    PoneK(usize),
    /// Append to the Writer layer at this index.
    MitteK(usize),
    PerageK,
    /// Evaluated the genus; resolve the overload in the saved scope.
    ResolveK(Sym, Scope),
    /// Evaluated the fixpoint operand; tie the knot.
    FixK(Expr, Scope),
    /// Leaving a `Lift` body: restore the previous depth.
    LiftK(usize),
    /// Finish a builtin that had to apply a closure (solve).
    BuiltinPost(builtins::PostOp),
}

/// A paused machine, resumable with more fuel. Opaque on purpose.
pub struct Continuation {
    control: Control,
    stack: Vec<Frame>,
    /// The mode's transformer tower with its effect state (M9).
    tower: Vec<Layer>,
    /// How many top layers `Lift` has currently peeled away.
    depth: usize,
    /// Which Except layer the in-flight `User` error was thrown on.
    thrown: usize,
    mode: Mode,
    arg: Value,
}

/// Fresh named-slot state (§15: `St` is a Record so a verse can hold
/// several state slots without nesting).
pub fn empty_state() -> Value {
    Value::Record(Rc::new(BTreeMap::new()))
}

/// Build the mode's tower, seeding the topmost State layer with `st`;
/// deeper State layers (the plagal sub-basement) start empty.
fn build_tower(mode: Mode, st: Value) -> Vec<Layer> {
    let mut st = Some(st);
    mode.tower()
        .iter()
        .map(|k| match k {
            LayerKind::Maybe => Layer::Maybe,
            LayerKind::Except => Layer::Except,
            LayerKind::State => Layer::State(st.take().unwrap_or_else(empty_state)),
            LayerKind::Writer => Layer::Writer(Plan::empty()),
            LayerKind::Commit => Layer::Commit,
        })
        .collect()
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
        tower: build_tower(mode, st),
        depth: 0,
        thrown: 0,
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
                    return Ok(finish(m.tower, Some(value), None));
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

/// Drain the tower into the outcome: the verse's visible state is the
/// topmost State layer, its plan the topmost Writer layer.
fn finish(tower: Vec<Layer>, value: Option<Value>, committed: Option<Plan>) -> VerseOutcome {
    let mut state = Value::Unit;
    let mut plan = Plan::empty();
    let (mut saw_state, mut saw_plan) = (false, false);
    for layer in tower {
        match layer {
            Layer::State(v) if !saw_state => {
                state = v;
                saw_state = true;
            }
            Layer::Writer(p) if !saw_plan => {
                plan = p;
                saw_plan = true;
            }
            _ => {}
        }
    }
    VerseOutcome { value, state, plan, committed }
}

/// Propagate a raised dissonance through the stack. A `User` error is
/// caught by the nearest `RecipeH` *on the same Except layer* — one
/// thrown on an inner (lifted) layer passes outer handlers untouched,
/// which is exactly the transformer semantics (M9). Everything else
/// aborts the verse — except `Silent`, which the driver reports as
/// "no correction".
fn unwind(m: &mut Continuation, d: Dissonance) -> Result<Control, Dissonance> {
    if matches!(d, Dissonance::User(_)) {
        while let Some(frame) = m.stack.pop() {
            match frame {
                Frame::RecipeH(name, handler, scope, channel) if channel == m.thrown => {
                    let err_value = match d {
                        Dissonance::User(v) => v,
                        _ => unreachable!(),
                    };
                    let scope = scope.bind(name, err_value);
                    return Ok(Control::Eval(handler, scope));
                }
                Frame::LiftK(depth) => m.depth = depth,
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

impl Continuation {
    /// The topmost layer of `kind` visible at the current lift depth —
    /// or `WrongMode`: legality is the tower's structure, not a mask.
    fn layer(&self, kind: LayerKind, op: OpKind) -> Result<usize, Dissonance> {
        (self.depth..self.tower.len())
            .find(|&i| self.tower[i].kind() == kind)
            .ok_or(Dissonance::WrongMode { mode: self.mode, op })
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
            m.layer(LayerKind::Maybe, OpKind::Nihil)?;
            return Err(Dissonance::Silent);
        }

        Expr::Clama(e) => {
            let ch = m.layer(LayerKind::Except, OpKind::Clama)?;
            m.stack.push(Frame::ClamaK(ch));
            Control::Eval(*e, scope)
        }

        Expr::Recipe(body, name, handler) => {
            let ch = m.layer(LayerKind::Except, OpKind::Recipe)?;
            m.stack.push(Frame::RecipeH(name, *handler, scope.clone(), ch));
            Control::Eval(*body, scope)
        }

        Expr::Lege => {
            let i = m.layer(LayerKind::State, OpKind::Lege)?;
            match &m.tower[i] {
                Layer::State(v) => Control::Value(v.clone()),
                _ => unreachable!(),
            }
        }

        Expr::Pone(e) => {
            let i = m.layer(LayerKind::State, OpKind::Pone)?;
            m.stack.push(Frame::PoneK(i));
            Control::Eval(*e, scope)
        }

        Expr::Mitte(e) => {
            let i = m.layer(LayerKind::Writer, OpKind::Mitte)?;
            m.stack.push(Frame::MitteK(i));
            Control::Eval(*e, scope)
        }

        Expr::Perage(e) => {
            m.layer(LayerKind::Commit, OpKind::Perage)?;
            m.stack.push(Frame::PerageK);
            Control::Eval(*e, scope)
        }

        Expr::Lift(e) => {
            // the transformer's lift (M9): peel the top layer so the
            // enclosed computation runs one level down the tower; the
            // value passes through unchanged
            if m.tower.len() - m.depth < 2 {
                return Err(Dissonance::WrongMode { mode: m.mode, op: OpKind::Lift });
            }
            m.stack.push(Frame::LiftK(m.depth));
            m.depth += 1;
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
            let tower = core::mem::take(&mut m.tower);
            Halt(finish(tower, Some(value), None))
        }

        Frame::ClamaK(channel) => {
            m.thrown = channel;
            return Err(Dissonance::User(value));
        }

        // the guarded computation finished without clamare: drop the handler
        Frame::RecipeH(_, _, _, _) => Continue(Control::Value(value)),

        Frame::PoneK(i) => {
            m.tower[i] = Layer::State(value);
            Continue(Control::Value(Value::Unit))
        }

        Frame::MitteK(i) => {
            let Layer::Writer(plan) = &mut m.tower[i] else { unreachable!() };
            match value {
                Value::Cmd(cmd) => {
                    plan.0.push(cmd);
                    Continue(Control::Value(Value::Unit))
                }
                Value::Plan(p) => {
                    plan.append(p);
                    Continue(Control::Value(Value::Unit))
                }
                other => {
                    return Err(Dissonance::Dissonant { want: "cmd", got: other.ty() })
                }
            }
        }

        Frame::PerageK => match value {
            Value::Plan(p) => {
                if p.0.is_empty() {
                    // committing before the field was computed: the plan
                    // is empty — the real mass would jerk on garbage
                    return Err(Dissonance::Premature);
                }
                let tower = core::mem::take(&mut m.tower);
                Halt(finish(tower, Some(Value::Unit), Some(p)))
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

        Frame::LiftK(depth) => {
            m.depth = depth;
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
