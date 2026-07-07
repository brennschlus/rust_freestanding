//! Builtins (§7.2): pure host functions callable from expressions,
//! sufficient for the meteor example, plus arithmetic and comparison
//! (the surface operators lower to these). All physics here is MOCK —
//! deterministic stand-ins the kernel can swap for real physics later.

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::types::{Dissonance, Field, GravityCmd, Plan, Scalar, Value, Vec3};

/// A body is "safe" when the field it feels stays under this threshold.
pub const SAFE_FORCE: Scalar = 1e-3;
/// MOCK Δv law: solve(f, r) = f(r) * SOLVE_K.
pub const SOLVE_K: Scalar = 0.5;

/// What a builtin call produces: a value, or a request to apply a
/// closure (only `solve` needs this) with a post-step to finish.
pub enum BuiltinResult {
    Done(Value),
    /// Apply `f` to `arg`; when the closure returns, run `post` on it.
    Apply { f: Value, arg: Value, post: PostOp },
}

#[derive(Debug, Clone, Copy)]
pub enum PostOp {
    /// solve: the closure returned the force; turn it into a Δv.
    SolveDv,
}

pub fn apply_post(post: PostOp, v: Value) -> Result<Value, Dissonance> {
    match post {
        PostOp::SolveDv => match v {
            Value::Num(force) => Ok(Value::Num(force * SOLVE_K)),
            other => Err(Dissonance::Dissonant { want: "num", got: other.ty() }),
        },
    }
}

/// Number of arguments each builtin takes; `None` = not a builtin.
pub fn arity(name: &str) -> Option<usize> {
    Some(match name {
        "add" | "sub" | "mul" | "div" | "eq" | "le" | "lt" => 2,
        "dist" | "mass" | "genus" | "outgassing" => 1,
        "safe" => 2,
        "solve" => 2,
        "deflect" => 1,
        "target" | "dv_of" | "at" => 1,
        "cmd" => 3,
        "plan_of" => 1,
        "corpus" => 4,
        "not" => 1,
        _ => return None,
    })
}

fn num(v: &Value) -> Result<Scalar, Dissonance> {
    match v {
        Value::Num(n) => Ok(*n),
        other => Err(Dissonance::Dissonant { want: "num", got: other.ty() }),
    }
}

fn record_get(v: &Value, key: &str) -> Result<Value, Dissonance> {
    match v {
        Value::Record(map) => map
            .get(key)
            .cloned()
            .ok_or(Dissonance::UnboundVar { name: String::from(key) }),
        other => Err(Dissonance::Dissonant { want: "record", got: other.ty() }),
    }
}

fn field_of(v: &Value) -> Result<&Field, Dissonance> {
    match v {
        Value::Field(f) => Ok(f),
        other => Err(Dissonance::Dissonant { want: "field", got: other.ty() }),
    }
}

/// Build a body record: `corpus id genus mass dist`.
pub fn make_corpus(id: u64, genus: &str, mass: Scalar, dist: Scalar) -> Value {
    let mut m = BTreeMap::new();
    m.insert(String::from("id"), Value::Num(id as Scalar));
    m.insert(String::from("genus"), Value::Genus(String::from(genus)));
    m.insert(String::from("mass"), Value::Num(mass));
    m.insert(String::from("dist"), Value::Num(dist));
    Value::Record(Rc::new(m))
}

/// Call a fully-applied builtin.
pub fn call(name: &str, args: &[Value]) -> Result<BuiltinResult, Dissonance> {
    use BuiltinResult::Done;
    let r = match name {
        "add" => Done(Value::Num(num(&args[0])? + num(&args[1])?)),
        "sub" => Done(Value::Num(num(&args[0])? - num(&args[1])?)),
        "mul" => Done(Value::Num(num(&args[0])? * num(&args[1])?)),
        "div" => Done(Value::Num(num(&args[0])? / num(&args[1])?)),
        "eq" => Done(Value::Bool(args[0] == args[1])),
        "le" => Done(Value::Bool(num(&args[0])? <= num(&args[1])?)),
        "lt" => Done(Value::Bool(num(&args[0])? < num(&args[1])?)),
        "not" => match &args[0] {
            Value::Bool(b) => Done(Value::Bool(!b)),
            other => return Err(Dissonance::Dissonant { want: "bool", got: other.ty() }),
        },

        "dist" => Done(record_get(&args[0], "dist")?),
        "mass" => Done(record_get(&args[0], "mass")?),
        "genus" => Done(record_get(&args[0], "genus")?),
        "outgassing" => Done(match record_get(&args[0], "outgassing") {
            Ok(v) => v,
            Err(_) => Value::Num(0.1), // comets vent by default (mock)
        }),

        // safe(corpus, g): the field the body feels stays under threshold
        "safe" => {
            let m = num(&record_get(&args[0], "mass")?)?;
            let d = num(&record_get(&args[0], "dist")?)?;
            let g = num(&args[1])?;
            Done(Value::Bool(g * m / (d * d) < SAFE_FORCE))
        }

        // solve(f, r): apply the resolved force law to r, then Δv (mock)
        "solve" => BuiltinResult::Apply {
            f: args[0].clone(),
            arg: args[1].clone(),
            post: PostOp::SolveDv,
        },

        // deflect(dv) -> Field. MOCK: target/at are synthesized; real
        // physics would derive them from the trajectory solution.
        "deflect" => Done(Value::Field(Field {
            target: 1,
            dv: Vec3::along_x(num(&args[0])?),
            at: 0,
        })),

        "target" => Done(Value::Num(field_of(&args[0])?.target as Scalar)),
        "dv_of" => Done(Value::Num(field_of(&args[0])?.dv.x)),
        "at" => Done(Value::Num(field_of(&args[0])?.at as Scalar)),

        "cmd" => Done(Value::Cmd(GravityCmd {
            target: num(&args[0])? as u64,
            dv: Vec3::along_x(num(&args[1])?),
            at: num(&args[2])? as u64,
        })),

        "plan_of" => {
            let f = field_of(&args[0])?;
            Done(Value::Plan(Plan(vec![GravityCmd {
                target: f.target,
                dv: f.dv,
                at: f.at,
            }])))
        }

        "corpus" => {
            let id = num(&args[0])? as u64;
            let genus = match &args[1] {
                Value::Genus(g) => g.clone(),
                Value::Str(s) => s.clone(),
                other => {
                    return Err(Dissonance::Dissonant { want: "genus", got: other.ty() })
                }
            };
            Done(make_corpus(id, &genus, num(&args[2])?, num(&args[3])?))
        }

        _ => return Err(Dissonance::UnboundVar { name: String::from(name) }),
    };
    Ok(r)
}

/// A builtin value with no arguments applied yet.
pub fn value(name: &str) -> Option<Value> {
    arity(name).map(|_| Value::Builtin { name: String::from(name), args: Vec::new() })
}
