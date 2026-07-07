//! Officium — a Turing-complete functional language where musical
//! structure is the semantics. The eight Gregorian modes are the monad
//! families, a verse is a monadic pipeline, and the fugue is the Reader
//! environment held by the organ.
//!
//! `no_std + alloc`; the `std` feature only enables hosted test helpers.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod builtins;
pub mod env;
pub mod ir;
pub mod machine;
pub mod types;

pub use env::{Env, Fugue, Program, Versus};
pub use ir::Expr;
pub use machine::{run_verse, resume, Continuation, VerseOutcome};
pub use types::{
    Dissonance, Final, GravityCmd, Mode, OpKind, Plan, Scalar, Sym, Value, Vec3,
};

use alloc::boxed::Box;

/// The single seam to the outside world (§10 of the spec). The kernel
/// implements this over its drivers; the host implements it with mocks.
/// `gravity_sink` is the ONLY effect escape in the whole system.
pub trait Platform {
    /// Apply a committed `Plan` to the gravity hardware.
    fn gravity_sink(&mut self, plan: &Plan) -> Result<(), Dissonance>;
    /// Microseconds since boot (for `at` timestamps and diagnostics).
    fn now(&self) -> u64;
    /// Diagnostics/log line from the interpreter driver.
    fn say(&mut self, line: &str);
}

/// Convenience: box an expression (the IR is a boxed tree).
pub fn b(e: Expr) -> Box<Expr> {
    Box::new(e)
}
