//! Organ trait + render: IR -> note events (§8, §9).
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod degree_op;
mod render;

pub use render::{
    mock_organ, pitch_hz, play, render_program, render_verse, Events, MockOrgan, Note, Organ,
    Pitch, Registration, Ticks, TICK_US,
};
