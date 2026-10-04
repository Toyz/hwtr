//! An R3000A + GTE interpreter for checking the port against the original.
//!
//! It runs single functions of `CCCPSX.EXE`, not the game: [`Machine::call`]
//! sets up arguments and a return address, runs until the function returns,
//! and leaves memory and registers for the test to compare with what the
//! Rust port computes from the same inputs.

#![forbid(unsafe_code)]

pub mod bus;
pub mod cpu;
pub mod gte;
pub mod machine;
pub mod state;

pub use bus::{Bus, Device};
pub use cpu::{Cpu, Fault};
pub use gte::Gte;
pub use machine::Machine;
