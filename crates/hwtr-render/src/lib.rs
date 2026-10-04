//! The renderer the port and the viewer share: tracks and cars from the
//! disc's data, drawn with wgpu, textured through a copy of VRAM with the
//! PlayStation's texture pages and CLUTs.

#![forbid(unsafe_code)]

pub mod mesh;
pub mod renderer;
pub mod scene;

pub use mesh::{Vram, Vtx};
pub use renderer::Renderer;
pub use scene::Scene;
