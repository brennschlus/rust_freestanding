//! Parser for the plain surface form (§7) — the machine-canonical form.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod lexer;
mod parser;

pub use parser::parse;
