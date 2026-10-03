//! The world model: `<TRACK>.WLD` (the track), `<TRACK>.DLW` (the same track
//! mirrored), and `<TRACK>.WLB` (the sky, objects only). One format, loaded by
//! `world_model_load` (0x800246d4), which adds the file's address to every
//! offset field.
//!
//! The track is a grid of 1024-unit cells, each with its own vertices,
//! vertex colours and polygons; objects are separate meshes placed with a
//! matrix, in a child/sibling tree. See `docs/formats/world.md`.

use std::fmt;

#[derive(Debug)]
pub struct WorldError(pub String);

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "world: {}", self.0)
    }
}

impl std::error::Error for WorldError {}

type Result<T> = std::result::Result<T, WorldError>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vertex {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

/// A cell polygon. Triangles repeat their third vertex (`v[2] == v[3]`).
/// The perimeter order is a, b, c, d; the GPU gets a, b, d, c.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Poly {
    /// Vertex indices into the cell's vertices, perimeter order.
    pub v: [u8; 4],
    /// Colour indices into the cell's colours.
    pub c: [u8; 4],
    /// Texture coordinates (u, v) per vertex a, b, c, d.
    pub uv: [[u8; 2]; 4],
    pub clut: u16,
    pub tpage: u8,
    /// Bit 0 double-sided, bit 1 no bias when back-facing, bits 3-7 depth
    /// bias in units of 4 ordering-table entries.
    pub flags: u8,
}

impl Poly {
    pub fn is_triangle(&self) -> bool {
        self.v[2] == self.v[3]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cell {
    pub verts: Vec<Vertex>,
    /// RGB, one per colour index.
    pub colours: Vec<[u8; 3]>,
    pub polys: Vec<Poly>,
}

/// One quad of an object's mesh (76 bytes).
#[derive(Clone, Copy, Debug)]
pub struct MeshQuad {
    pub v: [Vertex; 4],
    pub uv: [[u8; 2]; 4],
    /// RGB per vertex.
    pub colour: [[u8; 3]; 4],
    pub flat: u32,
    pub clut: u16,
    pub tpage: u16,
    /// Bit 0 double-sided; bits 16-31 depth bias.
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub struct Object {
    /// Rotation, 4.12.
    pub rot: [[i16; 3]; 3],
    /// Position in world units (the game keeps it as 20.12).
    pub pos: [i32; 3],
    /// Index of the child object, drawn with this object's matrix.
    pub child: Option<usize>,
    /// Index of the next sibling, drawn with the parent's matrix.
    pub sibling: Option<usize>,
    pub mesh: Vec<MeshQuad>,
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub struct ClutAnim {
    pub first: u8,
    pub last: u8,
    /// Frames per step; the sign is the direction.
    pub speed: i16,
    pub x: u16,
    pub y: u16,
    pub colours: [u16; 16],
}

#[derive(Clone, Debug)]
pub struct Pickup {
    /// Position, 20.12.
    pub pos: [i32; 3],
    pub unknown_0c: u32,
    pub object: Option<usize>,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct World {
    pub grid_w: u32,
    pub grid_h: u32,
    pub origin: [i32; 2],
    pub max: [i32; 2],
    /// Row-major, `cy * grid_w + cx`.
    pub cells: Vec<Cell>,
    pub objects: Vec<Object>,
    /// Objects the table at +44 lists (the visible set the loader flags).
    pub listed: Vec<usize>,
    pub clut_anims: Vec<ClutAnim>,
    pub pickups: Vec<Pickup>,
    pub counts: Counts,
    pub flags: u32,
    /// RGB, when `flags` bit 0 is set.
    pub background: [u8; 3],
}

/// Tables read so far only as counts.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    pub dyn84: u32,
    pub rec40: u32,
    pub anim: u32,
    pub rec24b: u32,
}

pub const CELL: i32 = 1024;
const OBJECT: usize = 52;
const QUAD: usize = 76;

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&self, at: usize, n: usize) -> Result<&[u8]> {
        self.0.get(at..at + n).ok_or_else(|| WorldError(format!("{n} bytes at {at:#x} past the end")))
    }
    fn u8(&self, at: usize) -> Result<u8> {
        Ok(self.bytes(at, 1)?[0])
    }
    fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(at, 2)?.try_into().unwrap()))
    }
    fn i16(&self, at: usize) -> Result<i16> {
        Ok(self.u16(at)? as i16)
    }
    fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(at, 4)?.try_into().unwrap()))
    }
    fn i32(&self, at: usize) -> Result<i32> {
        Ok(self.u32(at)? as i32)
    }
    fn vertex(&self, at: usize) -> Result<Vertex> {
        Ok(Vertex { x: self.i16(at)?, y: self.i16(at + 2)?, z: self.i16(at + 4)? })
    }
    /// An offset field that must point at a whole table.
    fn table(&self, at: usize, count: u32, size: usize) -> Result<usize> {
        let off = self.u32(at)? as usize;
        self.bytes(off, count as usize * size)?;
        Ok(off)
    }
}

fn index_of(off: u32, base: usize, size: usize, n: usize) -> Result<Option<usize>> {
    if off == 0 {
        return Ok(None);
    }
    let off = off as usize;
    if off < base || (off - base) % size != 0 || (off - base) / size >= n {
        return Err(WorldError(format!("object link {off:#x} is not an object")));
    }
    Ok(Some((off - base) / size))
}

impl World {
    pub fn parse(b: &[u8]) -> Result<World> {
        let r = Reader(b);
        let (grid_w, grid_h) = (r.u32(0)?, r.u32(4)?);
        let n_cells = (grid_w * grid_h) as usize;
        let mut cells = Vec::with_capacity(n_cells);
        let mut total = 0u32;
        for i in 0..n_cells {
            let at = 104 + i * 24;
            let (np, nv, nc) = (r.u32(at)?, r.u32(at + 8)?, r.u32(at + 16)?);
            let polys_at = r.table(at + 4, np, 20)?;
            let verts_at = r.table(at + 12, nv, 8)?;
            let cols_at = r.table(at + 20, nc, 4)?;
            if nv > 256 || nc > 256 {
                return Err(WorldError(format!("cell {i}: {nv} vertices, {nc} colours")));
            }
            let mut verts = Vec::with_capacity(nv as usize);
            for k in 0..nv as usize {
                let p = verts_at + k * 8;
                if r.u16(p + 6)? != 0xdead {
                    return Err(WorldError(format!("cell {i} vertex {k}: pad is not 0xdead")));
                }
                verts.push(r.vertex(p)?);
            }
            let colours = (0..nc as usize)
                .map(|k| r.bytes(cols_at + k * 4, 3).map(|c| [c[0], c[1], c[2]]))
                .collect::<Result<Vec<_>>>()?;
            let mut polys = Vec::with_capacity(np as usize);
            for k in 0..np as usize {
                let p = polys_at + k * 20;
                let v: [u8; 4] = r.bytes(p, 4)?.try_into().unwrap();
                let c: [u8; 4] = r.bytes(p + 4, 4)?.try_into().unwrap();
                if v.iter().any(|&x| x as u32 >= nv) || c.iter().any(|&x| x as u32 >= nc) {
                    return Err(WorldError(format!("cell {i} poly {k}: index out of range")));
                }
                let uv = |at: usize| -> Result<[u8; 2]> { Ok([r.u8(at)?, r.u8(at + 1)?]) };
                polys.push(Poly {
                    v,
                    c,
                    // Corner c's (u, v) is at +16, d's at +18: the packet build at
                    // 0x80031a34 puts the word's high half in the GPU's third
                    // slot (corner d) and its low half in the fourth (c).
                    uv: [uv(p + 8)?, uv(p + 12)?, uv(p + 16)?, uv(p + 18)?],
                    clut: r.u16(p + 10)?,
                    tpage: r.u8(p + 14)?,
                    flags: r.u8(p + 15)?,
                });
            }
            total += np;
            cells.push(Cell { verts, colours, polys });
        }
        if total != r.u32(24)? {
            return Err(WorldError(format!("cells hold {total} polygons, header says {}", r.u32(24)?)));
        }

        let n_obj = r.u32(36)?;
        let obj_base = r.table(40, n_obj, OBJECT)?;
        let mut objects = Vec::with_capacity(n_obj as usize);
        for k in 0..n_obj as usize {
            let o = obj_base + k * OBJECT;
            let mut rot = [[0i16; 3]; 3];
            for (i, row) in rot.iter_mut().enumerate() {
                for (j, x) in row.iter_mut().enumerate() {
                    *x = r.i16(o + 2 * (3 * i + j))?;
                }
            }
            let nq = r.u32(o + 40)?;
            let mesh_at = r.table(o + 44, nq, QUAD)?;
            let mut mesh = Vec::with_capacity(nq as usize);
            for q in 0..nq as usize {
                let m = mesh_at + q * QUAD;
                let mut v = [Vertex::default(); 4];
                let mut uv = [[0u8; 2]; 4];
                let mut colour = [[0u8; 3]; 4];
                for i in 0..4 {
                    v[i] = r.vertex(m + 8 * i)?;
                    uv[i] = [r.u8(m + 8 * i + 6)?, r.u8(m + 8 * i + 7)?];
                    let c = r.bytes(m + 32 + 8 * i, 3)?;
                    colour[i] = [c[0], c[1], c[2]];
                }
                mesh.push(MeshQuad {
                    v,
                    uv,
                    colour,
                    flat: r.u32(m + 64)?,
                    clut: r.u16(m + 68)?,
                    tpage: r.u16(m + 70)?,
                    flags: r.u32(m + 72)?,
                });
            }
            objects.push(Object {
                rot,
                pos: [r.i32(o + 20)?, r.i32(o + 24)?, r.i32(o + 28)?],
                child: index_of(r.u32(o + 32)?, obj_base, OBJECT, n_obj as usize)?,
                sibling: index_of(r.u32(o + 36)?, obj_base, OBJECT, n_obj as usize)?,
                mesh,
                flags: r.u32(o + 48)?,
            });
        }
        let n_listed = r.u32(32)?;
        let listed_at = r.table(44, n_listed, 4)?;
        let listed = (0..n_listed as usize)
            .map(|k| {
                index_of(r.u32(listed_at + 4 * k)?, obj_base, OBJECT, n_obj as usize)?
                    .ok_or_else(|| WorldError("null in the object list".into()))
            })
            .collect::<Result<Vec<_>>>()?;

        let counts = Counts { dyn84: r.u32(48)?, rec40: r.u32(56)?, anim: r.u32(64)?, rec24b: r.u32(88)? };
        r.table(52, counts.dyn84, 84)?;
        r.table(60, counts.rec40, 40)?;
        r.table(68, counts.anim, 24)?;
        r.table(92, counts.rec24b, 24)?;

        let n_pick = r.u32(72)?;
        let pick_at = r.table(76, n_pick, 32)?;
        let pickups = (0..n_pick as usize)
            .map(|k| {
                let p = pick_at + 32 * k;
                let name = r.bytes(p + 20, 12)?;
                let end = name.iter().position(|&c| c == 0 || c == 0xcd).unwrap_or(12);
                Ok(Pickup {
                    pos: [r.i32(p)?, r.i32(p + 4)?, r.i32(p + 8)?],
                    unknown_0c: r.u32(p + 12)?,
                    object: index_of(r.u32(p + 16)?, obj_base, OBJECT, n_obj as usize)?,
                    name: String::from_utf8_lossy(&name[..end]).into_owned(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let n_ca = r.u32(80)?;
        let ca_at = r.table(84, n_ca, 44)?;
        let clut_anims = (0..n_ca as usize)
            .map(|k| {
                let p = ca_at + 44 * k;
                let mut colours = [0u16; 16];
                for (i, c) in colours.iter_mut().enumerate() {
                    *c = r.u16(p + 12 + 2 * i)?;
                }
                Ok(ClutAnim {
                    first: r.u8(p)?,
                    last: r.u8(p + 1)?,
                    speed: r.i16(p + 2)?,
                    x: r.u16(p + 4)?,
                    y: r.u16(p + 6)?,
                    colours,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let bg = r.u32(100)?;
        Ok(World {
            grid_w,
            grid_h,
            origin: [r.i32(8)?, r.i32(12)?],
            max: [r.i32(16)?, r.i32(20)?],
            cells,
            objects,
            listed,
            clut_anims,
            pickups,
            counts,
            flags: r.u32(96)?,
            background: [bg as u8, (bg >> 8) as u8, (bg >> 16) as u8],
        })
    }

    pub fn poly_count(&self) -> usize {
        self.cells.iter().map(|c| c.polys.len()).sum()
    }
}

/// A GLM or GLB texture file: rectangles of VRAM, each `{x, y, w, h}` and
/// `w * h` halfwords, uploaded with LoadImage by `glm_load` (0x80024604).
pub fn read_vram_blocks(b: &[u8]) -> Result<Vec<(u16, u16, u16, u16, Vec<u16>)>> {
    let r = Reader(b);
    let mut out = Vec::new();
    let mut at = 0;
    while at < b.len() {
        let (x, y, w, h) = (r.u16(at)?, r.u16(at + 2)?, r.u16(at + 4)?, r.u16(at + 6)?);
        let n = w as usize * h as usize;
        let data = (0..n).map(|i| r.u16(at + 8 + 2 * i)).collect::<Result<Vec<_>>>()?;
        out.push((x, y, w, h, data));
        at += 8 + 2 * n;
    }
    Ok(out)
}
