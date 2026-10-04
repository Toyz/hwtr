//! A car's lamps: the glows behind its exhausts at high revs (0x80029fb0),
//! its headlights' beams (0x8002a81c), its tail lights' palette while it
//! brakes (0x80021f60), and its body darkening in the dark (0x8002ad48),
//! with the targets the frame's end sets (0x8002bb0c) and the glows'
//! strength by the revs (0x80049ecc).

use crate::effects::{CarPose, EffectQuad};
use crate::math::{Vec3, add, apply_matrix_lv, fx};
use crate::rand::Rand;

/// A car's lamps as its FXP gives them: each glow's place and the way it
/// stretches, each headlight's place and its beam's direction (half-scale
/// model units, 20.12).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lamps {
    pub glows: Vec<(Vec3, Vec3)>,
    pub headlights: Vec<(Vec3, Vec3)>,
}

/// A car's lights (its view state, cvs): the body's colour it fades to
/// (+0x1e9), the headlights' level it fades to and has (+0x1ea, +0x1eb),
/// whether that fade runs (+0x1f1), the brake lights (+0x1f2), whether the
/// headlights are drawn (+0x20 bit 0x20) and the glows' strength (+0x130,
/// 4.12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lights {
    pub body_target: u8,
    pub lamp_target: u8,
    pub lamp: u8,
    pub fading: bool,
    pub brake: bool,
    pub headlights: bool,
    pub glow: i32,
}

/// Which of a car's lights 0x8002ad48 fades.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fade {
    /// The model's colour, 5 a draw, to at most 128.
    Body,
    /// The headlights' level, 4 a draw, to at most 112.
    Lamps,
}

impl Lights {
    /// 0x800291b4: a player's car shows its headlights from the start if it
    /// has any; another car's never do.
    pub fn new(human: bool, lamps: &Lamps) -> Lights {
        Lights { headlights: human && !lamps.headlights.is_empty(), ..Lights::default() }
    }

    /// 0x8002bb0c, each frame's end for a player's car that is not
    /// wrecked, from its zone and brake bits (car +0x8): in the dark (1)
    /// the body fades to 48 and the headlights come up, under bit 2 the
    /// headlights come up and are shown, and with none of them the body
    /// fades back to 128 and the headlights go out; bit 4 is the brake.
    pub fn aim(&mut self, flags: i32, human: bool, wrecked: bool) {
        if wrecked || !human {
            return;
        }
        if flags & 1 != 0 {
            self.body_target = 48;
            self.lamp_target = 112;
        }
        if flags & 2 != 0 {
            self.lamp_target = 112;
            self.headlights = true;
        }
        self.brake = flags & 4 != 0;
        if flags == 0 {
            self.body_target = 128;
            self.lamp_target = 0;
        }
        self.fading = true;
    }

    /// 0x8002ad48 as the car draw runs it: the body's colour (`body`, the
    /// model's root colour, grey) or the headlights' level a step toward
    /// its target. Reaching the limit going up, or the target going down,
    /// clears the target; the headlights then stop fading, and once out
    /// are no longer shown. Nothing moves for a wreck or in the frozen
    /// results.
    pub fn fade(&mut self, which: Fade, body: &mut u32, wrecked: bool, frozen: bool) {
        let lamps = which == Fade::Lamps;
        let (target, current, step, limit) = match which {
            Fade::Body => (self.body_target, (*body & 0xff) as u8, 5i32, 128),
            Fade::Lamps => (self.lamp_target, self.lamp, 4, 112),
        };
        if target == current || wrecked {
            if lamps {
                self.fading = false;
            }
            return;
        }
        if frozen {
            return;
        }
        let up = target > current;
        let step = if up { step } else { -step };
        let next = current as i32 + step;
        let (mut target, mut current) = (target, current);
        let colour;
        if (up && next < limit) || (!up && (target as i32) < next) {
            current = current.wrapping_add(step as u8);
            colour = current;
        } else if up {
            colour = limit as u8;
            target = 0;
            if lamps {
                current = 112;
                self.fading = false;
            }
        } else {
            colour = target;
            target = 0;
            if lamps {
                self.headlights = false;
                current = 0;
                self.fading = false;
            }
        }
        let c = colour as u32;
        match which {
            Fade::Body => {
                self.body_target = target;
                *body = c << 16 | c << 8 | c;
            }
            Fade::Lamps => {
                self.lamp_target = target;
                self.lamp = current;
            }
        }
    }
}

/// 0x80049ecc's glow strength for a car, 4.12: none to half revs, then up
/// to 1 at the redline (twice the share of the band from idle, less one);
/// none for a wreck.
pub fn glow_level(engine: &crate::car::Engine, wrecked: bool) -> i32 {
    let n = engine.rpm.wrapping_sub(engine.idle);
    let d = engine.redline.wrapping_sub(engine.idle);
    let share = if (d.wrapping_add(0x8_0000) as u32) > 0x10_0000 {
        n.checked_div(d >> 12).unwrap_or(0)
    } else if d == 0 {
        0
    } else {
        let (q, r) = (n.wrapping_div(d), n.wrapping_rem(d));
        (q << 12).wrapping_add((r << 12).wrapping_div(d))
    };
    let level = fx(share, 8192).wrapping_sub(4096).max(0);
    if wrecked { 0 } else { level }
}

/// One lamp's draw: its corners (whole units, the model's axes) about
/// `origin` (20.12, where the lamp's matrix puts them).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LampDraw {
    pub origin: Vec3,
    pub quads: Vec<[[i16; 3]; 4]>,
}

/// The glow's prism (0x800bde14, 20.12): a triangle at the front and one
/// behind it, which the glow's stretch moves; its three sides (0x800bde5c).
const GLOW_VERTS: [Vec3; 6] = [
    [4302, 56572, -9216],
    [-6339, 56572, 9216],
    [-16981, 56572, -9216],
    [-4566, -66308, -3072],
    [-6339, -66308, 3072],
    [-8113, -66308, -3072],
];
const GLOW_SIDES: [[usize; 4]; 3] = [[2, 0, 5, 3], [1, 2, 5, 4], [0, 1, 4, 3]];
/// Car 36's (0x800bde68, 0x800bdef8): a hexagon and a smaller one behind,
/// six sides.
const HEX_VERTS: [Vec3; 12] = [
    [-19127, 56565, 82550],
    [-25618, 56565, 93793],
    [-38600, 56565, 93793],
    [-45092, 56565, 82550],
    [-38600, 56565, 71307],
    [-25618, 56565, 71307],
    [-29627, -66408, 82550],
    [-30868, -66408, 84700],
    [-33350, -66408, 84700],
    [-34591, -66408, 82550],
    [-33350, -66408, 80400],
    [-30868, -66408, 80400],
];
const HEX_SIDES: [[usize; 4]; 6] =
    [[0, 1, 7, 6], [1, 2, 8, 7], [2, 3, 9, 8], [3, 4, 10, 11], [5, 4, 10, 11], [0, 5, 11, 6]];
/// The glows' texels (0x800bddf4) and the headlights' (0x800bddd8).
const GLOW_UV: [[u8; 2]; 4] = [[0x7f, 0], [0x60, 0], [0x60, 0x1f], [0x7f, 0x1f]];
const BEAM_UV: [[u8; 2]; 4] = [[0x5f, 0x3f], [0x5f, 0x20], [0x40, 0x20], [0x40, 0x3f]];
/// The beam (0x800bdf10, 20.12): a small quad at the lamp and a wide one
/// ahead, which the beam's direction moves; three faces (0x800bdf70).
const BEAM_VERTS: [Vec3; 8] = [
    [-6260, -148763, -6159],
    [6260, -148763, -6159],
    [-101448, 109901, -31066],
    [101448, 109901, -31066],
    [-1276, -147174, 10344],
    [2189, -147174, 10344],
    [-71895, 148763, 6159],
    [77307, 148763, 6159],
];
const BEAM_FACES: [[usize; 4]; 3] = [[0, 2, 6, 4], [4, 6, 7, 5], [5, 7, 3, 1]];

/// 0x80029fb0: a car's glows, drawn when shown and not wrecked, while its
/// glow strength is above none. Each flickers by a random draw (unless
/// the game is paused or the results frozen, `still`), stretching the
/// back of its prism by its FXP vector; car 36 has a hexagon eight times
/// the size.
pub fn glows(
    lights: &Lights,
    lamps: &Lamps,
    car_id: u8,
    rand: &mut Rand,
    pose: &CarPose,
    still: bool,
) -> Vec<LampDraw> {
    let mut out = Vec::new();
    for &(pos, size) in &lamps.glows {
        let mut s = fx(0xf000, lights.glow);
        if s == 0 {
            continue;
        }
        let flicker = if still { 4096 } else { ((rand.below(1375) as i32) << 12) / 1000 + 512 };
        s = fx(fx(s, flicker), 4096);
        let hex = car_id == 36;
        let (scale, offset): (i32, Vec3) = if hex {
            (0x8000, [fx(0x80e8, 0x8000), fx(-61487, 0x8000), fx(-82550, 0x8000)])
        } else {
            (4096, [8192, -61440, 6144])
        };
        let origin = add(pose.at, apply_matrix_lv(&pose.rot, add(pos, offset)));
        let stretch = size.map(|c| fx(s, c));
        let (verts, sides, fixed): (&[Vec3], &[[usize; 4]], usize) =
            if hex { (&HEX_VERTS, &HEX_SIDES, 5) } else { (&GLOW_VERTS, &GLOW_SIDES, 2) };
        let quads = sides
            .iter()
            .map(|side| {
                side.map(|i| {
                    let v = verts[i];
                    std::array::from_fn(|c| {
                        let p = fx(scale, v[c]);
                        (if i > fixed { p.wrapping_add(stretch[c]) } else { p } >> 12) as i16
                    })
                })
            })
            .collect();
        out.push(LampDraw { origin, quads });
    }
    out
}

/// 0x8002a81c: a car's headlight beams, drawn when shown, not wrecked,
/// and the headlights are, at their level: each beam's far end pushed out
/// along the lamp's direction (170 times it), then widened.
pub fn beams(lights: &Lights, lamps: &Lamps, pose: &CarPose) -> Vec<LampDraw> {
    if !lights.headlights {
        return Vec::new();
    }
    let s = fx(4096, fx(0xa_a000, 4096));
    lamps
        .headlights
        .iter()
        .map(|&(pos, dir)| {
            let origin = add(pose.at, apply_matrix_lv(&pose.rot, [pos[0], pos[1].wrapping_add(0x2_8000), pos[2]]));
            let t = dir.map(|c| fx(s, c));
            let quads = BEAM_FACES
                .iter()
                .map(|face| {
                    face.map(|i| {
                        let v = BEAM_VERTS[i];
                        if matches!(i, 2 | 3 | 6 | 7) {
                            let k = [14336, 6144, 6144];
                            std::array::from_fn(|c| (fx(k[c], t[c].wrapping_add(v[c])) >> 12) as i16)
                        } else {
                            v.map(|c| (c >> 12) as i16)
                        }
                    })
                })
                .collect();
            LampDraw { origin, quads }
        })
        .collect()
}

/// A lamp's quads to draw, placed by `pose`'s turn: the glows' (sheet 4,
/// grey) or the beams' (sheet 17, at the headlights' `level`), blended.
pub fn quads(t: &crate::math::Tables, draws: &[LampDraw], pose: &CarPose, beam: Option<u8>) -> Vec<EffectQuad> {
    let (clut, tpage) = t.sprite_slots[if beam.is_some() { 17 } else { 4 }];
    let (uv, colour) = match beam {
        Some(level) => (BEAM_UV, [level; 3]),
        None => (GLOW_UV, [128; 3]),
    };
    draws
        .iter()
        .flat_map(|d| {
            d.quads.iter().map(move |q| EffectQuad {
                corners: q.map(|v| add(d.origin, apply_matrix_lv(&pose.rot, v.map(|c| (c as i32) << 12)))),
                uv,
                clut,
                tpage,
                colour,
                semi: true,
            })
        })
        .collect()
}

/// 0x80021f60: the first seven colours of a car's palette, as its skin
/// has them while it brakes, else each at half (0x80021b88 keeps both).
pub fn tail_lights(skin: &[u16; 7], brake: bool) -> [u16; 7] {
    if brake { *skin } else { skin.map(|c| (c >> 1) & 0x3def | 0x8000) }
}
