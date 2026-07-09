//! Core value and error types: modes, capability masks, values, plans,
//! and `Dissonance`.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::ir::Expr;
use crate::machine::Continuation;

/// Interned-enough for v1: symbols are plain strings.
pub type Sym = String;
/// Runtime "kind" of a body (asteroid / comet / ...).
pub type Genus = String;
/// f64 hosted; the kernel target is soft-float, so f64 works there too
/// (arithmetic lowers to compiler builtins, no FPU state involved).
pub type Scalar = f64;

/// The four finals the eight modes group around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Final {
    Re,  // D — Protus
    Mi,  // E — Deuterus
    Fa,  // F — Tritus
    Sol, // G — Tetrardus
}

/// The eight church modes = the monad families. Authentic = base monad,
/// plagal ("reaches below the final") = its transformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dorian,         // Protus authentic  — Maybe
    Hypodorian,     // Protus plagal     — MaybeT
    Phrygian,       // Deuterus authentic— Either
    Hypophrygian,   // Deuterus plagal   — ExceptT
    Lydian,         // Tritus authentic  — State
    Hypolydian,     // Tritus plagal     — StateT
    Mixolydian,     // Tetrardus authentic — IO (commit)
    Hypomixolydian, // Tetrardus plagal    — Plan (Writer)
}

/// Mode-gated operations (`pure`/`bind`/`ask`/`arg`/`if`/λ/app are legal
/// everywhere and are not listed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Nihil,
    Clama,
    Recipe,
    Lege,
    Pone,
    Mitte,
    Perage,
    Lift,
}

impl Mode {
    pub fn final_(&self) -> Final {
        match self {
            Mode::Dorian | Mode::Hypodorian => Final::Re,
            Mode::Phrygian | Mode::Hypophrygian => Final::Mi,
            Mode::Lydian | Mode::Hypolydian => Final::Fa,
            Mode::Mixolydian | Mode::Hypomixolydian => Final::Sol,
        }
    }

    pub fn is_plagal(&self) -> bool {
        matches!(
            self,
            Mode::Hypodorian | Mode::Hypophrygian | Mode::Hypolydian | Mode::Hypomixolydian
        )
    }

    pub fn from_final(f: Final, plagal: bool) -> Mode {
        match (f, plagal) {
            (Final::Re, false) => Mode::Dorian,
            (Final::Re, true) => Mode::Hypodorian,
            (Final::Mi, false) => Mode::Phrygian,
            (Final::Mi, true) => Mode::Hypophrygian,
            (Final::Fa, false) => Mode::Lydian,
            (Final::Fa, true) => Mode::Hypolydian,
            (Final::Sol, false) => Mode::Mixolydian,
            (Final::Sol, true) => Mode::Hypomixolydian,
        }
    }

    /// The capability mask (§4.1). This is what makes "you cannot `pone`
    /// in a Dorian verse" true without separate monad types.
    pub fn allows(&self, op: OpKind) -> bool {
        use Mode::*;
        use OpKind::*;
        match self {
            Dorian => matches!(op, Nihil),
            Hypodorian => matches!(op, Nihil | Lift),
            Phrygian => matches!(op, Clama | Recipe),
            Hypophrygian => matches!(op, Clama | Recipe | Lift),
            Lydian => matches!(op, Lege | Pone),
            Hypolydian => matches!(op, Lege | Pone | Lift),
            Mixolydian => matches!(op, Perage),
            Hypomixolydian => matches!(op, Mitte),
        }
    }
}

// --- the physics side: plans of gravity commands ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: Scalar,
    pub y: Scalar,
    pub z: Scalar,
}

impl Vec3 {
    pub fn along_x(x: Scalar) -> Vec3 {
        Vec3 { x, y: 0.0, z: 0.0 }
    }
}

pub type BodyId = u64;
pub type Micros = u64;

#[derive(Debug, Clone, PartialEq)]
pub struct GravityCmd {
    pub target: BodyId,
    pub dv: Vec3,
    pub at: Micros,
}

/// A monoid of gravity commands under concatenation. Building one is
/// pure (Hypomixolydian); committing it (`perage`) is the sole impurity.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan(pub Vec<GravityCmd>);

impl Plan {
    pub fn empty() -> Plan {
        Plan(Vec::new())
    }
    pub fn append(&mut self, mut other: Plan) {
        self.0.append(&mut other.0);
    }
}

/// A computed deflection field — pure data, not yet a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub target: BodyId,
    pub dv: Vec3,
    pub at: Micros,
}

// --- runtime values ---

/// Lexical scope: a persistent linked list, cheap to capture in closures.
#[derive(Debug, Clone, Default)]
pub struct Scope(pub Option<Rc<ScopeNode>>);

#[derive(Debug)]
pub struct ScopeNode {
    pub name: Sym,
    pub value: Value,
    pub next: Scope,
}

impl Scope {
    pub fn bind(&self, name: Sym, value: Value) -> Scope {
        Scope(Some(Rc::new(ScopeNode {
            name,
            value,
            next: self.clone(),
        })))
    }

    pub fn lookup(&self, name: &str) -> Option<&Value> {
        let mut cur = self;
        while let Some(node) = &cur.0 {
            if node.name == name {
                return Some(&node.value);
            }
            cur = &node.next;
        }
        None
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Unit,
    Num(Scalar),
    Bool(bool),
    Genus(Genus),
    Str(String),
    Closure {
        param: Sym,
        body: Rc<Expr>,
        env: Scope,
    },
    /// A builtin, possibly partially applied (curried).
    Builtin { name: Sym, args: Vec<Value> },
    Cmd(GravityCmd),
    Plan(Plan),
    Field(Field),
    Record(Rc<BTreeMap<Sym, Value>>),
}

impl Value {
    pub fn ty(&self) -> &'static str {
        match self {
            Value::Unit => "unit",
            Value::Num(_) => "num",
            Value::Bool(_) => "bool",
            Value::Genus(_) => "genus",
            Value::Str(_) => "str",
            Value::Closure { .. } => "closure",
            Value::Builtin { .. } => "builtin",
            Value::Cmd(_) => "cmd",
            Value::Plan(_) => "plan",
            Value::Field(_) => "field",
            Value::Record(_) => "record",
        }
    }
}

/// Structural equality where it makes sense; closures/builtins never
/// compare equal (used by golden tests, not by the language).
impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Unit, Unit) => true,
            (Num(a), Num(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Genus(a), Genus(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (Cmd(a), Cmd(b)) => a == b,
            (Plan(a), Plan(b)) => a == b,
            (Field(a), Field(b)) => a == b,
            (Record(a), Record(b)) => a == b,
            _ => false,
        }
    }
}

/// Hints for runtime type-clash diagnostics (pre-checker).
pub type TyHint = &'static str;

/// Everything that can go wrong — or merely signal — during the office.
/// `Silent` and `OutOfFuel` are control signals, not failures.
pub enum Dissonance {
    /// Op illegal in this mode (capability mask violation).
    WrongMode { mode: Mode, op: OpKind },
    /// No real/tonal answer for (name, genus).
    Unresolved { name: Sym, genus: Genus },
    /// Protus "no correction" short-circuit (not an error).
    Silent,
    /// Deuterus `clama`.
    User(Value),
    /// Runtime type clash.
    Dissonant { want: TyHint, got: TyHint },
    /// `perage` before its Plan was computed (empty plan committed).
    Premature,
    /// `perage` of a non-Plan value.
    Unconsecrated,
    /// Unbound variable (not in scope, builtins, or pedals).
    UnboundVar { name: Sym },
    /// Static check failure (§7.4): a rhyme that does not unify, or any
    /// type/mode clash the checker caught before the verse was sung.
    Discors { versus: Sym, msg: String },
    /// Step budget exhausted; resumable, not a failure.
    OutOfFuel(Box<Continuation>),
    /// Parser diagnostics.
    Parse { line: u32, msg: String },
}

impl fmt::Debug for Dissonance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dissonance::WrongMode { mode, op } => {
                write!(f, "WrongMode {{ mode: {:?}, op: {:?} }}", mode, op)
            }
            Dissonance::Unresolved { name, genus } => {
                write!(f, "Unresolved {{ name: {}, genus: {} }}", name, genus)
            }
            Dissonance::Silent => write!(f, "Silent"),
            Dissonance::User(v) => write!(f, "User({:?})", v),
            Dissonance::Dissonant { want, got } => {
                write!(f, "Dissonant {{ want: {}, got: {} }}", want, got)
            }
            Dissonance::Premature => write!(f, "Premature"),
            Dissonance::Unconsecrated => write!(f, "Unconsecrated"),
            Dissonance::UnboundVar { name } => write!(f, "UnboundVar {{ name: {} }}", name),
            Dissonance::Discors { versus, msg } => {
                write!(f, "Discors {{ versus: {}, msg: {} }}", versus, msg)
            }
            Dissonance::OutOfFuel(_) => write!(f, "OutOfFuel(<continuation>)"),
            Dissonance::Parse { line, msg } => {
                write!(f, "Parse {{ line: {}, msg: {} }}", line, msg)
            }
        }
    }
}

impl Dissonance {
    /// Control signals are not failures (§11).
    pub fn is_failure(&self) -> bool {
        !matches!(self, Dissonance::Silent | Dissonance::OutOfFuel(_))
    }
}
