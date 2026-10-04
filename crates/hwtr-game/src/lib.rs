//! Hot Wheels Turbo Racing, ported.
//!
//! Each module here is a rewrite of a part of `CCCPSX.EXE`, named after what
//! it does, with the original function addresses in its docs. The tests run
//! the original functions in `hwtr-cpu` and hold the port to the same
//! results.

#![forbid(unsafe_code)]

pub mod ai;
pub mod body;
pub mod camera;
pub mod car;
pub mod collision;
pub mod fsm;
pub mod hud;
pub mod laps;
pub mod line;
pub mod math;
pub mod pad;
pub mod race;
pub mod rand;
