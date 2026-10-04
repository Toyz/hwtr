//! Collision: the track's zones and planes, the bodies' collision points,
//! and the contacts between them.

pub mod scp;

pub use scp::{Plane, Scp, Section, Zone};
