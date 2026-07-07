//! The Core IR (§5). Untyped; values are tagged at runtime.

use alloc::boxed::Box;
use crate::types::{Mode, Sym, Value};

#[derive(Debug, Clone)]
pub enum Expr {
    // λ-core
    Var(Sym),
    Lam(Sym, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    Lit(Value),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Stretto: the subject re-enters over itself — recursion.
    /// `Fix f` unrolls lazily as `f (\x -> Fix f x)` through the trampoline.
    Fix(Box<Expr>),

    // monadic core (mode-tagged by the verse header at lowering time)
    /// `pure e` — a cadence *value*; does not halt by itself.
    Pure(Mode, Box<Expr>),
    /// `amen e` — cadence to the tonic: yield `e` and halt the verse.
    Return(Mode, Box<Expr>),
    /// `x <- m; k` — one verse line.
    Bind(Mode, Box<Expr>, Sym, Box<Expr>),

    // Reader (the fugue)
    /// The value this verse is a function of.
    Arg,
    /// Dominus: read a pedal constant / environment entry.
    Ask(Sym),
    /// Overload resolution by genus (§4.2). The resolved overload is
    /// evaluated in the *current* lexical scope, so its free variables
    /// (the subject parameter, pedals) see the verse's bindings.
    Resolve(Sym, Box<Expr>),

    // per-monad ops (legality enforced by Mode::allows)
    Nihil,
    Clama(Box<Expr>),
    /// `recipe m x h` — run m; if it `clama`s, bind the error to x and
    /// run the handler h.
    Recipe(Box<Expr>, Sym, Box<Expr>),
    Lege,
    Pone(Box<Expr>),
    /// Append a GravityCmd to the growing Plan (pure).
    Mitte(Box<Expr>),
    /// Commit a Plan to the world — the only impure primitive.
    Perage(Box<Expr>),
    /// Authentic→plagal lift. v1: legality marker (plagal only), value
    /// passes through unchanged (§6.3).
    Lift(Box<Expr>),
}
