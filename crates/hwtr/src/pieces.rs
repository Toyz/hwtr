//! The front end's 3D pieces on screen (0x80083ac0 and what it calls): each
//! model from `SCREENS.SCR` placed by the game's own matrix chain, in its
//! fixed point, then projected as the GTE does with the front end's view
//! (diag(1, -1/2, -1), H 554, centre (320, 120)) and lit by its one light.
//!
//! The chain: a camera 866 units back; a piece's place pulled toward it in
//! proportion so its x and y land where the front end's coordinates say
//! whatever its depth; its scale; its turn about x, y and z; each object's
//! own placement; the view.

use hwtr_data::scr::{Object, Scr};
use hwtr_game::front::TrackDraw;
use hwtr_game::front::screen::PieceDraw;
use hwtr_game::math::Tables;
use hwtr_render::Vtx;
use hwtr_render::mesh::Vram;

type M = [[i32; 3]; 3];

/// The view (0x8011a7dc in the front end).
const VIEW: M = [[4096, 0, 0], [0, -2048, 0], [0, 0, -4096]];
/// The projection: H and the screen's centre.
const H: f32 = 554.0;
const CENTRE: (f32, f32) = (320.0, 120.0);
/// The camera's distance (0x80083710).
const BACK: i32 = 0x36_2000;
/// The front end's light (0x80015584) and colour matrix (0x800bd070: half
/// the first light, each channel), and the back colour (32 << 4).
const LIGHT: [i32; 3] = [0, -4096, -7094];
const LIGHT_COLOUR: i32 = 2048;
const BACK_COLOUR: i32 = 512;
/// Beyond this the original drops a quad (half of 0x1518, against the sum
/// of its four depths over 8).
const FAR: i32 = 0x1518 >> 1;

fn sat(v: i64) -> i32 {
    v.clamp(-0x8000, 0x7fff) as i32
}

/// The GTE's matrix product, each element shifted down 12 and saturated.
fn mul(a: &M, b: &M) -> M {
    std::array::from_fn(|i| std::array::from_fn(|j| sat((0..3).map(|k| a[i][k] as i64 * b[k][j] as i64).sum::<i64>() >> 12)))
}

/// ApplyMatrixLV: a long vector through a matrix, down 12.
fn apply(m: &M, v: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| ((0..3).map(|k| m[i][k] as i64 * v[k] as i64).sum::<i64>() >> 12) as i32)
}

/// A turn in 4.12 radians as 4096ths of a turn (0x80083ac0's `* 2/pi`).
fn turn_angle(r: i32) -> usize {
    let x = r.wrapping_shl(12);
    let hi = ((x as i64 * 0xa2f9_6525u32 as i32 as i64) >> 32) as i32;
    ((hi.wrapping_add(x) >> 14).wrapping_sub(x >> 31) & 4095) as usize
}

/// libgte's RotMatrixX, RotMatrixY and RotMatrixZ from identity.
fn rot(tables: &Tables, axis: usize, a: usize) -> M {
    let (s, c) = tables.rcossin[a & 4095];
    let (s, c) = (s as i32, c as i32);
    match axis {
        0 => [[4096, 0, 0], [0, c, -s], [0, s, c]],
        1 => [[c, 0, s], [0, 4096, 0], [-s, 0, c]],
        _ => [[c, -s, 0], [s, c, 0], [0, 0, 4096]],
    }
}

/// Where the textures of `SCREENS.GLM` are: each image's texture page and
/// palette (as `Vtx::mode`) and where in the page it starts.
pub struct Textures(pub Vec<(u32, u8, u8)>);

impl Textures {
    /// The images into `vram` (0x80028184): image `i` at the place the table
    /// at 0x800bddbc gives, its palette at (640, 450 + i).
    pub fn load(glm: &[u8], byte: &dyn Fn(u32) -> u8, vram: &mut Vram) -> Textures {
        let mut out = Vec::new();
        for (i, b) in hwtr_data::scr::glm_images(glm).into_iter().enumerate() {
            let Ok(tim) = hwtr_data::Tim::parse(b) else { continue };
            let half = |a: u32| u16::from_le_bytes([byte(a), byte(a + 1)]);
            let (x, y) = (half(0x800b_ddbc + 4 * i as u32), half(0x800b_ddbe + 4 * i as u32));
            let clut = (640u16, 450 + i as u16);
            vram.load(x, y, tim.rect.w, tim.rect.h, &tim.data);
            if let Some((r, colours)) = &tim.clut {
                vram.load(clut.0, clut.1, r.w, 1, &colours[..r.w as usize]);
            }
            let page = (x as u32 / 64) | ((y as u32 / 256) << 4) | (1 << 7);
            let mode = ((clut.1 as u32) << 6 | (clut.0 as u32) >> 4) | page << 16;
            // 0x800280d4: u moves by the image's place in its page (two
            // pixels a halfword), v by its row.
            out.push((mode, ((x & 63) * 2) as u8, (y & 255) as u8));
        }
        Textures(out)
    }
}

/// One quad ready to draw: its depth key (larger is drawn first) and two
/// triangles.
struct Drawn {
    key: i32,
    tris: [Vtx; 6],
}

/// The track map's tilt (0x800859ac): -1.21 radians about x.
const TRACK_TILT: i32 = ((-0x4_6000i64 * 71) >> 12) as i32;

/// The triangles of `pieces` and the track map, back to front.
pub fn triangles(pieces: &[PieceDraw], track: Option<&TrackDraw>, scr: &Scr, textures: &Textures, tables: &Tables) -> Vec<Vtx> {
    let mut drawn = Vec::new();
    if let Some(t) = track
        && let Some(first) = scr.model(&t.model)
    {
        // Scaled, then placed (the place scaled too), tilted, turned.
        let mut m = [[t.scale, 0, 0], [0, t.scale, 0], [0, 0, t.scale]];
        let at = apply(&m, t.pos);
        let tr = [at[0], at[1], at[2] - BACK];
        m = mul(&m, &rot(tables, 0, turn_angle(TRACK_TILT)));
        m = mul(&m, &rot(tables, 2, turn_angle(t.spin)));
        object(scr, first, &m, tr, textures, &mut drawn);
    }
    for p in pieces {
        let Some(first) = scr.model(&p.model) else { continue };
        let k = (BACK - p.pos[2]) / 866;
        let s = hwtr_game::math::fx(p.scale, k);
        let (x, y) = (hwtr_game::math::fx(p.pos[0], k), hwtr_game::math::fx(p.pos[1], k));
        let mut m = [[s, 0, 0], [0, s, 0], [0, 0, s]];
        for axis in 0..3 {
            m = mul(&m, &rot(tables, axis, turn_angle(p.turn[axis])));
        }
        let t = [x, y, p.pos[2] - BACK];
        object(scr, first, &m, t, textures, &mut drawn);
    }
    drawn.sort_by_key(|d| std::cmp::Reverse(d.key));
    drawn.into_iter().flat_map(|d| d.tris).collect()
}

/// 0x80028404: an object under the placement (`m`, `t`), its children
/// under its own, its siblings under the same.
fn object(scr: &Scr, at: usize, m: &M, t: [i32; 3], textures: &Textures, out: &mut Vec<Drawn>) {
    let Some(o) = scr.objects.get(at) else { return };
    let own = o.rot.map(|r| r.map(i32::from));
    let r = mul(m, &own);
    let pos = apply(m, o.pos.map(|v| v << 12));
    let tt = [pos[0] + t[0], pos[1] + t[1], pos[2] + t[2]];
    quads(o, &r, tt, textures, out);
    if let Some(c) = o.child {
        object(scr, c, &r, tt, textures, out);
    }
    if let Some(n) = o.next {
        object(scr, n, m, t, textures, out);
    }
}

/// set_object_matrix and 0x80010000: the view applied, each quad projected,
/// culled if it faces away (unless drawn from both sides) or is too far,
/// lit per corner.
fn quads(o: &Object, r: &M, t: [i32; 3], textures: &Textures, out: &mut Vec<Drawn>) {
    let rv = mul(&VIEW, r);
    let tr = apply(&VIEW, t).map(|v| v >> 12);
    // The light in the model's frame (0x80014e5c turns it by the object).
    let light = apply(&rv, LIGHT);
    for q in &o.quads {
        // RTPS: TR in, the rotated vertex added, down 12, saturated.
        let project = |v: [i16; 3]| -> [i32; 3] {
            std::array::from_fn(|i| sat(((tr[i] as i64) << 12) + (0..3).map(|k| rv[i][k] as i64 * v[k] as i64).sum::<i64>() >> 12))
        };
        let ps = q.corners.map(|c| project(c.pos));
        if ps.iter().any(|p| p[2] <= 0) {
            continue;
        }
        let screen = ps.map(|p| (CENTRE.0 + H * p[0] as f32 / p[2] as f32, CENTRE.1 + H * p[1] as f32 / p[2] as f32));
        let (a, b, c) = (screen[0], screen[1], screen[2]);
        let nclip = (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1);
        if !q.both_sides && nclip > 0.0 {
            continue;
        }
        let depth: i32 = ps.iter().map(|p| p[2].min(0xffff)).sum();
        if depth == 0 || FAR - (depth >> 3) < 0 {
            continue;
        }
        let Some(&(mode, du, dv)) = textures.0.get(q.texture).or(textures.0.first()) else { continue };
        let colour = |n: [i16; 3]| {
            let lum = (((0..3).map(|k| light[k] as i64 * n[k] as i64).sum::<i64>() >> 12) as i32).clamp(0, 0xfff);
            let ir = (BACK_COLOUR + ((LIGHT_COLOUR * lum) >> 12)).clamp(0, 0xfff);
            let ch = |c: u8| ((c as i32 * ir) >> 12).clamp(0, 255) as u32;
            ch(q.colour[0]) | ch(q.colour[1]) << 8 | ch(q.colour[2]) << 16
        };
        let us = q.corners.map(|c| c.uv[0].wrapping_add(du));
        let vs = q.corners.map(|c| c.uv[1].wrapping_add(dv));
        let window = Vtx::window_of(us, vs);
        let v = |i: usize| Vtx {
            pos: [screen[i].0, screen[i].1, 0.0],
            colour: colour(q.corners[i].normal),
            uv: us[i] as u32 | (vs[i] as u32) << 8,
            mode,
            window,
        };
        // Corners 0, 1, 3, 2 go round the quad.
        out.push(Drawn { key: ((depth >> 3) & !3) + q.depth_bias, tris: [v(0), v(1), v(3), v(1), v(2), v(3)] });
    }
}

/// The car previews' tilt (0x80084bcc): -1.39 radians about x.
const CAR_TILT: i32 = ((-0x5_0000i64 * 71) >> 12) as i32;

/// A car preview's model, its skin's palette and page (as `Vtx::mode`),
/// and how far the front end shifts it to centre it (the car's handling
/// block A at +0x80 and +0x84 in `CWHS.BMF`: across and along).
pub struct PreviewCar {
    pub model: hwtr_data::car::Model,
    pub mode: u32,
    pub centre: [i32; 2],
}

/// 0x80084bcc and car_draw (0x800286d4): a car preview's triangles, back
/// to front. The chain: the camera; the size; the place (sized too); the
/// tilt; the spin about the car's up axis; the shift to its centre; the
/// view. car_draw doubles the translation (cars are drawn at half their
/// model's scale) and turns each wheel about its axle as it rolls.
pub fn car_triangles(d: &hwtr_game::front::CarDraw, car: &PreviewCar, tables: &Tables) -> Vec<Vtx> {
    let s = d.scale;
    let mut m: M = [[s, 0, 0], [0, s, 0], [0, 0, s]];
    let at = apply(&m, d.pos);
    let mut t = [at[0], at[1], at[2] - BACK];
    m = mul(&m, &rot(tables, 0, turn_angle(CAR_TILT)));
    m = mul(&m, &rot(tables, 2, turn_angle(d.angle)));
    let shift = apply(&m, [-car.centre[0], -car.centre[1], 0]);
    for k in 0..3 {
        t[k] += shift[k];
    }
    let rv = mul(&VIEW, &m);
    let tr = apply(&VIEW, t).map(|v| (v >> 12) * 2);
    let wheel = rot(tables, 0, turn_angle(-d.wheels));
    let root = car.model.root.pos;
    let mut drawn = Vec::new();
    let nodes = std::iter::once((&car.model.root, None)).chain(car.model.children.iter().map(|c| (c, Some(c.pos))));
    for (node, offset) in nodes {
        let place = |v: [i32; 3]| -> [i32; 3] {
            let v = match offset {
                Some(o) => {
                    let r = apply(&wheel, v.map(|c| c << 12)).map(|c| c >> 12);
                    [r[0] + o[0], r[1] + o[1], r[2] + o[2]]
                }
                None => v,
            };
            [v[0] + root[0], v[1] + root[1], v[2] + root[2]]
        };
        for f in &node.faces {
            let corner = |k: usize| {
                let v = node.verts[f.v[k] as usize];
                let p = place([v.x as i32, v.y as i32, v.z as i32]);
                let q: [i32; 3] = std::array::from_fn(|i| {
                    sat((((tr[i] as i64) << 12) + (0..3).map(|k| rv[i][k] as i64 * p[k] as i64).sum::<i64>()) >> 12)
                });
                q
            };
            let ps = [corner(0), corner(1), corner(2), corner(3)];
            if ps.iter().any(|p| p[2] <= 0) {
                continue;
            }
            let screen = ps.map(|p| (CENTRE.0 + H * p[0] as f32 / p[2] as f32, CENTRE.1 + H * p[1] as f32 / p[2] as f32));
            let (a, b, c) = (screen[0], screen[1], screen[2]);
            let nclip = (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1);
            if nclip > 0.0 {
                continue;
            }
            let depth = ps.iter().map(|p| p[2].min(0xffff)).sum::<i32>() >> 2;
            let v = |k: usize| Vtx {
                pos: [screen[k].0, screen[k].1, 0.0],
                colour: 0x80_80_80,
                uv: f.uv[k][0] as u32 | (f.uv[k][1] as u32) << 8,
                mode: car.mode,
                window: Vtx::WHOLE,
            };
            let tris = if f.v[2] != f.v[3] { vec![v(0), v(1), v(3), v(1), v(2), v(3)] } else { vec![v(0), v(1), v(3)] };
            drawn.push((depth + f.z_bias as i32, tris));
        }
    }
    drawn.sort_by_key(|d| std::cmp::Reverse(d.0));
    drawn.into_iter().flat_map(|d| d.1).collect()
}
