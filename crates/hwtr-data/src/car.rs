//! Cars: the `.BMF` container and the model format inside it.
//!
//! A car's BMF holds five parts: the full model, the lowest-detail model, the
//! medium-detail model, the handling block (CWH) and the effect points (FXP).
//! `<CAR>.CAR` is a copy of part 0 for the front end. See
//! `docs/formats/car.md`.

use std::fmt;

#[derive(Debug)]
pub struct CarError(pub String);

impl fmt::Display for CarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "car: {}", self.0)
    }
}

impl std::error::Error for CarError {}

type Result<T> = std::result::Result<T, CarError>;

fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(CarError(m.into()))
}

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| CarError(format!("short at {at:#x}")))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| CarError(format!("short at {at:#x}")))
}

/// Splits a BMF into its parts: `u32 count; u32 table_bytes; u32 size[count]`,
/// then the parts back to back.
pub fn bmf_parts(b: &[u8]) -> Result<Vec<&[u8]>> {
    let n = u32_at(b, 0)? as usize;
    if n == 0 || n > 64 || u32_at(b, 4)? as usize != 4 * n {
        return err(format!("BMF header: {n} parts"));
    }
    let mut at = 8 + 4 * n;
    let mut parts = Vec::with_capacity(n);
    for i in 0..n {
        let size = u32_at(b, 8 + 4 * i)? as usize;
        parts.push(b.get(at..at + size).ok_or_else(|| CarError(format!("part {i} past the end")))?);
        at += size;
    }
    if at != b.len() {
        return err(format!("parts end at {at:#x}, file at {:#x}", b.len()));
    }
    Ok(parts)
}

/// The parts of a car BMF, by what they are.
pub struct CarBmf<'a> {
    /// Full detail, low detail, medium detail.
    pub models: [&'a [u8]; 3],
    pub cwh: &'a [u8],
    pub fxp: &'a [u8],
}

impl<'a> CarBmf<'a> {
    pub fn parse(b: &'a [u8]) -> Result<CarBmf<'a>> {
        let p = bmf_parts(b)?;
        if p.len() != 5 {
            return err(format!("a car BMF has 5 parts, this has {}", p.len()));
        }
        // Parts 1 and 2 are the low and medium models; the game maps detail
        // level 1 to part 2 and level 2 to part 1.
        Ok(CarBmf { models: [p[0], p[2], p[1]], cwh: p[3], fxp: p[4] })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SVector {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

/// A model face, a quad (triangles repeat C as D). Corners A, B, C, D go
/// around the edge; the GPU gets A, B, D, C.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub v: [u8; 4],
    pub n: [u8; 4],
    /// (u, v) of corners A, B, C, D. Stored in the order D, A, C, B.
    pub uv: [[u8; 2]; 4],
    pub flags: u16,
    pub z_bias: u16,
}

#[derive(Clone, Debug)]
pub struct Node {
    /// Rotation, 4.12 (identity in every car on the disc).
    pub rot: [[i16; 3]; 3],
    /// Translation in model units.
    pub pos: [i32; 3],
    pub verts: Vec<SVector>,
    pub normals: Vec<SVector>,
    pub faces: Vec<Face>,
    pub rgb: [u8; 3],
}

#[derive(Clone, Debug)]
pub struct Model {
    pub root: Node,
    /// The wheels.
    pub children: Vec<Node>,
    /// Per child: s32 position twice, radius, width (half model scale).
    pub wheels: Vec<[i32; 8]>,
}

const NODE: usize = 72;

fn read_node(b: &[u8], at: usize) -> Result<Node> {
    let mut rot = [[0i16; 3]; 3];
    for (i, row) in rot.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = u16_at(b, at + 2 * (3 * i + j))? as i16;
        }
    }
    let pos = [u32_at(b, at + 0x14)? as i32, u32_at(b, at + 0x18)? as i32, u32_at(b, at + 0x1c)? as i32];
    let (nv, nn) = (u16_at(b, at + 0x28)? as usize, u16_at(b, at + 0x2a)? as usize);
    let (vo, no) = (u32_at(b, at + 0x2c)? as usize, u32_at(b, at + 0x30)? as usize);
    let (nf, fo) = (u32_at(b, at + 0x34)? as usize, u32_at(b, at + 0x38)? as usize);
    let sv = |p: usize| -> Result<SVector> {
        Ok(SVector { x: u16_at(b, p)? as i16, y: u16_at(b, p + 2)? as i16, z: u16_at(b, p + 4)? as i16 })
    };
    let verts = (0..nv).map(|i| sv(vo + 8 * i)).collect::<Result<Vec<_>>>()?;
    let normals = (0..nn).map(|i| sv(no + 8 * i)).collect::<Result<Vec<_>>>()?;
    let mut faces = Vec::with_capacity(nf);
    for i in 0..nf {
        let f = fo + 20 * i;
        let s = b.get(f..f + 20).ok_or_else(|| CarError(format!("face {i} past the end")))?;
        let v = [s[0], s[1], s[2], s[3]];
        let n = [s[4], s[5], s[6], s[7]];
        if v.iter().any(|&x| x as usize >= nv) || n.iter().any(|&x| x as usize >= nn) {
            return err(format!("face {i}: index out of range"));
        }
        faces.push(Face {
            v,
            n,
            uv: [[s[10], s[11]], [s[14], s[15]], [s[12], s[13]], [s[8], s[9]]],
            flags: u16::from_le_bytes([s[16], s[17]]),
            z_bias: u16::from_le_bytes([s[18], s[19]]),
        });
    }
    let rgb = u32_at(b, at + 0x44)?;
    Ok(Node { rot, pos, verts, normals, faces, rgb: [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8] })
}

impl Model {
    pub fn parse(b: &[u8]) -> Result<Model> {
        let n_sub = u32_at(b, 0)? as usize;
        if n_sub > 16 || u32_at(b, 4)? != 0x14 {
            return err(format!("model header: {n_sub} children, root at {:#x}", u32_at(b, 4)?));
        }
        let root = read_node(b, 0x14)?;
        let child_at = u32_at(b, 8)? as usize;
        let wheel_at = u32_at(b, 12)? as usize;
        let children = (0..n_sub).map(|i| read_node(b, child_at + NODE * i)).collect::<Result<Vec<_>>>()?;
        let wheels = (0..n_sub)
            .map(|i| {
                let mut w = [0i32; 8];
                for (k, x) in w.iter_mut().enumerate() {
                    *x = u32_at(b, wheel_at + 32 * i + 4 * k)? as i32;
                }
                Ok(w)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Model { root, children, wheels })
    }

    pub fn face_count(&self) -> usize {
        self.root.faces.len() + self.children.iter().map(|c| c.faces.len()).sum::<usize>()
    }
}

/// The CWH handling block: magic 0x0b9757a5, then 308 bytes and 64 bytes the
/// game copies into the car's state.
pub const CWH_MAGIC: u32 = 0x0b97_57a5;

pub fn cwh_blocks(b: &[u8]) -> Result<(&[u8], &[u8])> {
    if u32_at(b, 0)? != CWH_MAGIC || b.len() != 376 {
        return err("not a CWH block");
    }
    Ok((&b[4..312], &b[312..376]))
}

/// The FXP counts (a, b, c), checked against the part's size: 4 + 40a + 92b.
pub fn fxp_counts(b: &[u8]) -> Result<(u8, u8, u8)> {
    let (a, bb, c) = (*b.first().ok_or(CarError("empty FXP".into()))?, b[1], b[2]);
    if b.len() != 4 + 40 * a as usize + 92 * bb as usize {
        return err(format!("FXP of {} bytes with counts {a} {bb} {c}", b.len()));
    }
    Ok((a, bb, c))
}

/// Unpacks a DECALS.BMF part: u16 words, 0xffff then a count of zero pixels,
/// anything else a pixel; 192 x 64 pixels.
pub fn unpack_decal(b: &[u8]) -> Result<Vec<u16>> {
    const PIXELS: usize = 192 * 64;
    let mut out = Vec::with_capacity(PIXELS);
    let mut at = 0;
    while out.len() < PIXELS {
        let w = u16_at(b, at)?;
        at += 2;
        if w == 0xffff {
            let n = u16_at(b, at)? as usize;
            at += 2;
            out.extend(std::iter::repeat_n(0, n));
        } else {
            out.push(w);
        }
    }
    if out.len() != PIXELS {
        return err(format!("decal unpacks to {} pixels", out.len()));
    }
    Ok(out)
}
