//! Tracks and cars as triangles, and the VRAM image their textures live in.

use hwtr_data::world::{Vertex, World};
use rrt::glam;
use rrt::kit::Pack;

/// The PlayStation's video memory: 1024 x 512 halfwords.
pub struct Vram {
    pub words: Vec<u16>,
}

pub const VRAM_W: usize = 1024;
pub const VRAM_H: usize = 512;

impl Default for Vram {
    fn default() -> Self {
        Vram::new()
    }
}

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
#[derive(Clone, Copy, Debug)]
pub struct Vtx {
    pub pos: [f32; 3],
    /// r | g << 8 | b << 16
    pub colour: u32,
    /// u | v << 8
    pub uv: u32,
    /// clut | tpage << 16
    pub mode: u32,
    /// The texels the polygon may read: u from, v from, u to, v to (each a
    /// byte, inclusive).
    pub window: u32,
}

impl Vtx {
    /// The whole texture page.
    pub const WHOLE: u32 = 0xffff_0000;
    /// A mode bit: a semi-transparent polygon.
    pub const SEMI: u32 = 1 << 31;
    /// A mode bit (the page's unused bit 14): an untextured polygon, its
    /// colour as it is (a POLY_F4 or POLY_G4), not modulating a texel.
    pub const FLAT: u32 = 1 << 30;

    /// The texels of a sprite whose corners have `u` and `v`: the
    /// PlayStation never reaches its far edges, so neither may sampling.
    pub fn window_of(u: [u8; 4], v: [u8; 4]) -> u32 {
        let span = |c: [u8; 4]| {
            let (lo, hi) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
            (lo as u32, if hi > lo { hi as u32 - 1 } else { hi as u32 })
        };
        let ((u0, u1), (v0, v1)) = (span(u), span(v));
        u0 | v0 << 8 | u1 << 16 | v1 << 24
    }
}

impl Pack for Vtx {
    /// Each field little-endian, in order, matching the layout the pipeline
    /// declares.
    const SIZE: usize = 28;

    fn pack(&self, out: &mut Vec<u8>) {
        self.pos.pack(out);
        [self.colour, self.uv, self.mode, self.window].pack(out);
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
    world_triangles_apart(w, &[]).0
}

/// The same, with the objects `skip` (and their children) left out, and
/// where each of those hangs (its parent's pose), in that order: the
/// pickups and the moving objects, drawn as they go.
pub fn world_triangles_apart(w: &World, skip: &[usize]) -> (Vec<Vtx>, Vec<Pose>) {
    let mut out = Vec::new();
    for cell in &w.cells {
        for p in &cell.polys {
            let corner = |i: usize| Vtx {
                pos: pos(cell.verts[p.v[i] as usize]),
                colour: rgb(cell.colours[p.c[i] as usize]),
                uv: uv(p.uv[i]),
                mode: p.clut as u32 | (p.tpage as u32) << 16,
                window: Vtx::WHOLE,
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
    let mut skipped = Vec::new();
    for root in (0..w.objects.len()).filter(|&i| !linked[i]) {
        object(w, root, identity, &mut out, 0, skip, &mut skipped);
    }
    let parents = skip
        .iter()
        .map(|&i| skipped.iter().find(|(k, _)| *k == i).map_or((glam::Mat3::IDENTITY, glam::Vec3::ZERO), |&(_, p)| p))
        .collect();
    (out, parents)
}

/// A pose: rotation and position.
pub type Pose = (glam::Mat3, glam::Vec3);

/// Object `i` and its children, without its siblings, under `parent`, the
/// object at rotation `rot` (4.12) and position `pos` (world units).
pub fn object_posed(w: &World, i: usize, parent: Pose, rot: [[i16; 3]; 3], pos: glam::Vec3) -> Vec<Vtx> {
    let mut out = Vec::new();
    let mine = object_mesh_at(w, i, parent, rot, pos, &mut out);
    if let Some(c) = w.objects[i].child {
        object(w, c, mine, &mut out, 1, &[], &mut Vec::new());
    }
    out
}

fn object(
    w: &World,
    i: usize,
    parent: Pose,
    out: &mut Vec<Vtx>,
    depth: u32,
    skip: &[usize],
    skipped: &mut Vec<(usize, Pose)>,
) {
    if depth > 64 {
        return;
    }
    if skip.contains(&i) {
        skipped.push((i, parent));
    } else {
        let mine = object_mesh(w, i, parent, out);
        if let Some(c) = w.objects[i].child {
            object(w, c, mine, out, depth + 1, skip, skipped);
        }
    }
    if let Some(s) = w.objects[i].sibling {
        object(w, s, parent, out, depth + 1, skip, skipped);
    }
}

/// Object `i`'s own quads under `parent`, at its own rotation and place;
/// its pose.
fn object_mesh(w: &World, i: usize, parent: Pose, out: &mut Vec<Vtx>) -> Pose {
    let o = &w.objects[i];
    object_mesh_at(w, i, parent, o.rot, glam::Vec3::new(o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32), out)
}

/// Object `i`'s own quads under `parent`, at rotation `r` (4.12) and place
/// `pos` (world units); its pose.
fn object_mesh_at(w: &World, i: usize, parent: Pose, r: [[i16; 3]; 3], pos: glam::Vec3, out: &mut Vec<Vtx>) -> Pose {
    let o = &w.objects[i];
    let m = glam::Mat3::from_cols(
        glam::Vec3::new(r[0][0] as f32, r[1][0] as f32, r[2][0] as f32),
        glam::Vec3::new(r[0][1] as f32, r[1][1] as f32, r[2][1] as f32),
        glam::Vec3::new(r[0][2] as f32, r[1][2] as f32, r[2][2] as f32),
    ) / 4096.0;
    let mine = (parent.0 * m, parent.0 * pos + parent.1);
    for q in &o.mesh {
        let corner = |k: usize| {
            let v = q.v[k];
            let p = mine.0 * glam::Vec3::new(v.x as f32, v.y as f32, v.z as f32) + mine.1;
            Vtx {
                pos: p.to_array(),
                colour: rgb(q.colour[k]),
                uv: uv(q.uv[k]),
                mode: q.clut as u32 | (q.tpage as u32) << 16,
                window: Vtx::WHOLE,
            }
        };
        out.extend([corner(0), corner(1), corner(3), corner(1), corner(2), corner(3)]);
        if q.flags & 1 != 0 {
            out.extend([corner(0), corner(3), corner(1), corner(1), corner(3), corner(2)]);
        }
    }
    mine
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

/// Where a race puts car `slot`'s shadow image (0x800bdd48).
pub const SHADOW_TEXTURE: [(u16, u16); 6] = [(384, 256), (400, 256), (416, 256), (432, 256), (448, 256), (464, 256)];

/// Car `slot`'s shadow uploaded: the image at its place, the CLUT at (384,
/// 482 + slot).
pub fn place_shadow_texture(vram: &mut Vram, tim: &hwtr_data::Tim, slot: usize) {
    let (x, y) = SHADOW_TEXTURE[slot];
    vram.load(x, y, tim.rect.w, tim.rect.h, &tim.data);
    if let Some((r, colours)) = &tim.clut {
        vram.load(384, 482 + slot as u16, r.w, 1, &colours[..r.w as usize]);
    }
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
/// `wheels` turns and places each wheel node (rotation, translation in
/// model units, and whether it rides on the body) as the game last posed
/// it; a wheel without one sits where the model puts it. The body and the
/// wheels on it are drawn at `body_scale` about the root (the scale cheats,
/// 0x80022274); a wheel off the car is not.
pub fn car_triangles(
    m: &hwtr_data::car::Model,
    clut: u16,
    tpage: u16,
    pos: glam::Vec3,
    rot: glam::Mat3,
    root_rgb: u32,
    body_scale: f32,
    wheels: &[(glam::Mat3, glam::Vec3, bool)],
) -> Vec<Vtx> {
    let mut out = Vec::new();
    let root = glam::Vec3::new(m.root.pos[0] as f32, m.root.pos[1] as f32, m.root.pos[2] as f32);
    let nodes = std::iter::once((&m.root, glam::Vec3::ZERO))
        .chain(m.children.iter().map(|c| (c, glam::Vec3::new(c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32))));
    for (n, (node, offset)) in nodes.enumerate() {
        let colour = if n == 0 { root_rgb } else { 0x80_80_80 };
        for f in &node.faces {
            let corner = |k: usize| {
                let v = node.verts[f.v[k] as usize];
                let v = glam::Vec3::new(v.x as f32, v.y as f32, v.z as f32);
                let placed = match n.checked_sub(1).and_then(|w| wheels.get(w)) {
                    Some(&(turn, at, true)) => (turn * v + at) * body_scale,
                    Some(&(turn, at, false)) => turn * v + at,
                    None => (v + offset) * body_scale,
                };
                let local = (placed + root) * 0.5;
                Vtx {
                    pos: (rot * local + pos).to_array(),
                    colour,
                    uv: uv(f.uv[k]),
                    mode: clut as u32 | (tpage as u32) << 16,
                    window: Vtx::WHOLE,
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
