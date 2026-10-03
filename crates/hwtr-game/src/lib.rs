//! Hot Wheels Turbo Racing, ported.
//!
//! Each module here is a rewrite of a part of `CCCPSX.EXE`, named after what
//! it does, with the original function addresses in its docs. The tests run
//! the original functions in `hwtr-cpu` and hold the port to the same
//! results.

pub mod fsm;
pub mod math;
