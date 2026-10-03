//! A track as triangles and a VRAM image, ready for the GPU.

use hwtr_data::world::{Vertex, World};

/// The PlayStation's video memory: 1024 x 512 halfwords.
pub struct Vram {
    pub words: Vec<u16>,
}

pub const VRAM_W: usize = 1024;
pub const VRAM_H: usize = 512;

impl Vram {
    pub fn new() -> Vram {
        Vram { words: vec![0; VRAM_W * VRAM_H] }
    }

    /// LoadImage: a rectangle of halfwords at (x, y).
    pub fn load(&mut self, x: u16, y: u16, w: u16, h: u16, data: &[u16]) {
        for row in 0..h as usize {
            for col in 0..w as usize {
                let (vx, vy) = ((x as usize + col) % VRAM_W, (y as usize + row) % VRAM_H);
                self.words[vy * VRAM_W + vx] = data[row * w as usize + col];
            }
        }
    }
}

/// One vertex as the shader takes it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vtx {
    pub pos: [f32; 3],
    /// r | g << 8 | b << 16
    pub colour: u32,
    /// u | v << 8
    pub uv: u32,
    /// clut | tpage << 16
    pub mode: u32,
}

impl Vtx {
    pub fn bytes(v: &[Vtx]) -> &[u8] {
        // SAFETY: Vtx is repr(C), plain old data, 24 bytes with no padding.
        unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
    }
}

fn rgb(c: [u8; 3]) -> u32 {
    c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16
}

fn uv(t: [u8; 2]) -> u32 {
    t[0] as u32 | (t[1] as u32) << 8
}

fn pos(v: Vertex) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

/// Triangles for every cell polygon and every object quad, wound so that a
/// face is front where the game's NCLIP test says so: its right-hand normal
/// (corner 0 to 1 to 3) toward the viewer. Double-sided faces are emitted
/// again reversed, since the viewer culls back faces as the game does.
pub fn world_triangles(w: &World) -> Vec<Vtx> {
    let mut out = Vec::new();
    for cell in &w.cells {
        for p in &cell.polys {
            let corner = |i: usize| Vtx {
                pos: pos(cell.verts[p.v[i] as usize]),
                colour: rgb(cell.colours[p.c[i] as usize]),
                uv: uv(p.uv[i]),
                mode: p.clut as u32 | (p.tpage as u32) << 16,
            };
            let mut tri = vec![corner(0), corner(1), corner(3)];
            if !p.is_triangle() {
                tri.extend([corner(1), corner(2), corner(3)]);
            }
            if p.flags & 1 != 0 {
                let back: Vec<Vtx> = tri.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
                tri.extend(back);
            }
            out.extend(tri);
        }
    }
    // Objects: walk the trees from the objects nothing links to.
    let mut linked = vec![false; w.objects.len()];
    for o in &w.objects {
        for i in o.child.into_iter().chain(o.sibling) {
            linked[i] = true;
        }
    }
    let identity = (glam::Mat3::IDENTITY, glam::Vec3::ZERO);
    for root in (0..w.objects.len()).filter(|&i| !linked[i]) {
        object(w, root, identity, &mut out, 0);
    }
    out
}

fn object(w: &World, i: usize, parent: (glam::Mat3, glam::Vec3), out: &mut Vec<Vtx>, depth: u32) {
    if depth > 64 {
        return;
    }
    let o = &w.objects[i];
    let r = o.rot;
    let m = glam::Mat3::from_cols(
        glam::Vec3::new(r[0][0] as f32, r[1][0] as f32, r[2][0] as f32),
        glam::Vec3::new(r[0][1] as f32, r[1][1] as f32, r[2][1] as f32),
        glam::Vec3::new(r[0][2] as f32, r[1][2] as f32, r[2][2] as f32),
    ) / 4096.0;
    let mine = (parent.0 * m, parent.0 * glam::Vec3::new(o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32) + parent.1);
    for q in &o.mesh {
        let corner = |k: usize| {
            let v = q.v[k];
            let p = mine.0 * glam::Vec3::new(v.x as f32, v.y as f32, v.z as f32) + mine.1;
            Vtx {
                pos: p.to_array(),
                colour: rgb(q.colour[k]),
                uv: uv(q.uv[k]),
                mode: q.clut as u32 | (q.tpage as u32) << 16,
            }
        };
        out.extend([corner(0), corner(1), corner(3), corner(1), corner(2), corner(3)]);
        if q.flags & 1 != 0 {
            out.extend([corner(0), corner(3), corner(1), corner(1), corner(3), corner(2)]);
        }
    }
    if let Some(c) = o.child {
        object(w, c, mine, out, depth + 1);
    }
    if let Some(s) = o.sibling {
        object(w, s, parent, out, depth + 1);
    }
}

/// The middle of the track's grid, as a starting point.
pub fn centre(w: &World) -> glam::Vec3 {
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for c in &w.cells {
        for v in &c.verts {
            let p = glam::Vec3::from(pos(*v));
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo + hi) / 2.0
}

/// Where a race puts car `slot`'s skin and palette in VRAM: the image table
/// at 0x800bdd30, the CLUT at (384, 464 + slot).
pub const CAR_TEXTURE: [(u16, u16); 6] = [(960, 256), (704, 0), (768, 0), (832, 0), (896, 0), (640, 256)];

/// The car's skin uploaded for race slot `slot`, and the (clut, tpage) its
/// faces then use (8-bit page).
pub fn place_car_texture(vram: &mut Vram, tim: &hwtr_data::Tim, slot: usize) -> (u16, u16) {
    let (x, y) = CAR_TEXTURE[slot];
    vram.load(x, y, tim.rect.w, tim.rect.h, &tim.data);
    let cy = 464 + slot as u16;
    if let Some((r, colours)) = &tim.clut {
        vram.load(384, cy, r.w, 1, &colours[..r.w as usize]);
    }
    ((cy << 6) | (384 >> 4), (x / 64) | ((y / 256) << 4) | (1 << 7))
}

/// A car model's triangles at `pos` turned by `rot`. The game draws cars at
/// half their model's scale (inferred: at full scale the grid's cars would
/// overlap). Faces are lit at the base colour 0x80, a texel modulation of 1.
pub fn car_triangles(m: &hwtr_data::car::Model, clut: u16, tpage: u16, pos: glam::Vec3, rot: glam::Quat) -> Vec<Vtx> {
    let mut out = Vec::new();
    let root = glam::Vec3::new(m.root.pos[0] as f32, m.root.pos[1] as f32, m.root.pos[2] as f32);
    let nodes = std::iter::once((&m.root, glam::Vec3::ZERO))
        .chain(m.children.iter().map(|c| (c, glam::Vec3::new(c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32))));
    for (node, offset) in nodes {
        for f in &node.faces {
            let corner = |k: usize| {
                let v = node.verts[f.v[k] as usize];
                let local = (glam::Vec3::new(v.x as f32, v.y as f32, v.z as f32) + offset + root) * 0.5;
                Vtx {
                    pos: (rot * local + pos).to_array(),
                    colour: 0x80_80_80,
                    uv: uv(f.uv[k]),
                    mode: clut as u32 | (tpage as u32) << 16,
                }
            };
            out.extend([corner(0), corner(1), corner(3)]);
            if f.v[2] != f.v[3] {
                out.extend([corner(1), corner(2), corner(3)]);
            }
        }
    }
    out
}
