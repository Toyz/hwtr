//! The front end's 3D pieces, `SCREENS.SCR` (loaded by 0x80028184): the
//! dialog box, arrows, button icons and track maps, each a tree of objects
//! made of textured quads; and their textures, `SCREENS.GLM`.
//!
//! ```text
//! u32 names        how many models
//! u32 objects      how many objects
//! u32 at_objects   52 bytes each
//! u32 at_models    one u32 a model: the offset of its first object
//! u32 at_names     16 bytes a model
//!
//! object:
//!   i16 rot[3][3], pad      4.12
//!   i32 pos[3]              integer units (the game shifts them up 12)
//!   u32 child, next         offsets of the objects drawn under it and
//!                           beside it (0 for none)
//!   u32 quads, at_quads     76 bytes each
//!
//! quad:
//!   { i16 x, y, z; u8 u, v } [4]
//!   { i16 nx, ny, nz, pad } [4]
//!   u8 r, g, b, pad
//!   i16 texture, pad        index into SCREENS.GLM's images (<0 is 0)
//!   u32 flags               bit 0: drawn from both sides; >>16: depth bias
//! ```
//!
//! Each quad's corners in order 0, 1, 3, 2 go round it.

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Corner {
    pub pos: [i16; 3],
    pub uv: [u8; 2],
    pub normal: [i16; 3],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quad {
    pub corners: [Corner; 4],
    pub colour: [u8; 3],
    pub texture: usize,
    pub both_sides: bool,
    pub depth_bias: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Object {
    pub rot: [[i16; 3]; 3],
    pub pos: [i32; 3],
    /// Objects (by index) drawn under this one, with its placement, and
    /// beside it, with its parent's.
    pub child: Option<usize>,
    pub next: Option<usize>,
    pub quads: Vec<Quad>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scr {
    /// Each model's name and its first object.
    pub models: Vec<(String, usize)>,
    pub objects: Vec<Object>,
}

const OBJECT_SIZE: usize = 52;
const QUAD_SIZE: usize = 76;

impl Scr {
    pub fn parse(b: &[u8]) -> Option<Scr> {
        let (names, count) = (u32_at(b, 0)? as usize, u32_at(b, 4)? as usize);
        let (at_objects, at_models, at_names) = (u32_at(b, 8)? as usize, u32_at(b, 12)? as usize, u32_at(b, 16)? as usize);
        let index = |off: u32| -> Option<usize> {
            let off = off as usize;
            (off >= at_objects && (off - at_objects).is_multiple_of(OBJECT_SIZE)).then(|| (off - at_objects) / OBJECT_SIZE)
        };
        let objects = (0..count)
            .map(|k| {
                let o = at_objects + OBJECT_SIZE * k;
                let rot = std::array::from_fn(|r| std::array::from_fn(|c| u16_at(b, o + 6 * r + 2 * c).unwrap_or(0) as i16));
                let pos = std::array::from_fn(|i| u32_at(b, o + 0x14 + 4 * i).unwrap_or(0) as i32);
                let link = |at: usize| u32_at(b, o + at).filter(|&v| v != 0).and_then(index);
                let n = u32_at(b, o + 0x28)? as usize;
                let at = u32_at(b, o + 0x2c)? as usize;
                let quads = (0..n)
                    .map(|q| {
                        let p = at + QUAD_SIZE * q;
                        let corners = std::array::from_fn(|c| {
                            let v = p + 8 * c;
                            let nn = p + 32 + 8 * c;
                            Corner {
                                pos: std::array::from_fn(|i| u16_at(b, v + 2 * i).unwrap_or(0) as i16),
                                uv: [b.get(v + 6).copied().unwrap_or(0), b.get(v + 7).copied().unwrap_or(0)],
                                normal: std::array::from_fn(|i| u16_at(b, nn + 2 * i).unwrap_or(0) as i16),
                            }
                        });
                        let flags = u32_at(b, p + 72)?;
                        Some(Quad {
                            corners,
                            colour: [*b.get(p + 64)?, *b.get(p + 65)?, *b.get(p + 66)?],
                            texture: (u16_at(b, p + 68)? as i16).max(0) as usize,
                            both_sides: flags & 1 != 0,
                            depth_bias: ((flags as i32) >> 16) << 4,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(Object { rot, pos, child: link(0x20), next: link(0x24), quads })
            })
            .collect::<Option<Vec<_>>>()?;
        let models = (0..names)
            .map(|k| {
                let n = b.get(at_names + 16 * k..at_names + 16 * k + 16)?;
                let name = n.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
                Some((name, index(u32_at(b, at_models + 4 * k)?)?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Scr { models, objects })
    }

    /// The model called `name` (as the game's lookup, 0x80027f54, which
    /// ignores case).
    pub fn model(&self, name: &str) -> Option<usize> {
        self.models.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, o)| o)
    }
}

/// `SCREENS.GLM`: `{u32 len; TIM}` repeated; image `i` goes at `PLACES[i]`
/// (0x800bddbc) whatever its own position says, its palette at
/// (640, 450 + i).
pub fn glm_images(b: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(len) = u32_at(b, at) {
        let Some(tim) = b.get(at + 4..at + 4 + len as usize) else { break };
        out.push(tim);
        at += 4 + (len as usize & !3);
    }
    out
}
