//! Collision: the track's zones and planes, the bodies' collision points,
//! and the contacts between them.

pub mod camera;
pub mod create;
pub mod fences;
pub mod ground;
pub mod object;
pub mod pairs;
pub mod scp;
pub mod walls;
pub mod wheels;
pub mod world;

pub use object::{CollisionObject, Kind, RefSet};
pub use scp::{Plane, Scp, Section, Zone};
pub use world::{Collision, ObjectId};
