//! Stunts: a player's car in the air is watched turning about its axes, and
//! on landing the turns and the time aloft are named and rewarded with
//! points and turbos (0x8003cb74, 0x80080148).

use super::Car;
use crate::math::{Vec3, column, div_fx, dot, fx};
use crate::rand::Rand;

/// How many stunts the tables describe: 0 to 3 time aloft, 4 to 12 flips,
/// 13 to 29 spins, 30 to 34 rolls, 35 to 48 combinations.
pub const STUNTS: usize = 49;

/// What each stunt is worth (0x800befe8 priority, 0x800bf01c turbos,
/// 0x800bf050 points), by stunt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StuntTable {
    /// Which stunt names a landing when several are done; one entry more
    /// than the stunts, the first read for "none".
    pub priority: [u8; STUNTS + 1],
    pub turbos: [u8; STUNTS],
    pub points: [i32; STUNTS],
    /// Each stunt's names, as text ids, one picked at random (0x800bf114:
    /// a count and up to six).
    pub names: [(u8, [u16; 6]); STUNTS],
}

impl Default for StuntTable {
    fn default() -> StuntTable {
        StuntTable { priority: [0; STUNTS + 1], turbos: [0; STUNTS], points: [0; STUNTS], names: [(0, [0; 6]); STUNTS] }
    }
}

impl StuntTable {
    pub const PRIORITY: u32 = 0x800b_efe7;
    pub const TURBOS: u32 = 0x800b_f01c;
    pub const POINTS: u32 = 0x800b_f050;
    pub const NAMES: u32 = 0x800b_f114;

    /// The tables from a byte reader over the executable or RAM.
    pub fn read(byte: &impl Fn(u32) -> u8) -> StuntTable {
        let word = |a: u32| i32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        StuntTable {
            priority: std::array::from_fn(|k| byte(Self::PRIORITY + k as u32)),
            turbos: std::array::from_fn(|k| byte(Self::TURBOS + k as u32)),
            points: std::array::from_fn(|k| word(Self::POINTS + 4 * k as u32)),
            names: std::array::from_fn(|k| {
                let at = Self::NAMES + 14 * k as u32;
                (byte(at), std::array::from_fn(|i| u16::from_le_bytes([byte(at + 2 + 2 * i as u32), byte(at + 3 + 2 * i as u32)])))
            }),
        }
    }

    /// 0x80080d6c: one of the stunt's names, at random (the text id).
    pub fn name(&self, stunt: u16, rand: &mut Rand) -> Option<u16> {
        let (count, names) = self.names.get(stunt as usize)?;
        names.get(rand.below(*count as u32) as usize).copied()
    }

    fn priority(&self, stunt: Option<u16>) -> u8 {
        self.priority[stunt.map_or(0, |s| s as usize + 1)]
    }
}

/// A landing's reward: the stunt that names it, its points and turbos, and
/// (once announced) the name shown, as a text id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Award {
    pub stunt: u16,
    pub points: i32,
    pub turbos: u8,
    pub name: Option<u16>,
}

/// One axis's turn on landing, in degrees: its turn until it last reversed
/// (`peak`) and since (`rest`), each rounded 45 degrees outward.
#[derive(Clone, Copy, Debug)]
struct Turn {
    peak: i32,
    rest: i32,
}

impl Turn {
    fn rounded(self) -> (i32, i32) {
        let out = |v: i32| if v < 0 { v - 45 } else { v + 45 };
        (out(self.peak), out(self.rest))
    }
}

/// What one axis scored: the stunt (if any), its tier for combinations
/// (0 when none, or when turned both ways), and the points and turbos.
#[derive(Default)]
struct Axis {
    stunt: Option<u16>,
    tier: u8,
    points: i32,
    turbos: u32,
}

impl Axis {
    fn add(&mut self, t: &StuntTable, stunt: u16) {
        self.points = self.points.wrapping_add(t.points[stunt as usize]);
        self.turbos += t.turbos[stunt as usize] as u32;
    }

    /// Scores `turned` degrees by `tiers` (from the most: its least, the
    /// stunt and the tier), unless it was turned both ways (`both`), which
    /// counts the both-ways stunt again instead.
    fn tiers(&mut self, t: &StuntTable, turned: i32, both: bool, tiers: &[(i32, u16, u8)]) {
        let Some(&(_, stunt, tier)) = tiers.iter().find(|&&(from, ..)| turned >= from) else { return };
        if !both {
            self.stunt = Some(stunt);
            self.tier = tier;
        }
        let counted = self.stunt.expect("a stunt");
        self.add(t, counted);
    }
}

/// 0x80080148: names and rewards a landing from the turns about the car's x
/// axis (flips), y axis (rolls) and z axis (spins), and the milliseconds
/// aloft. One stunt alone (or the time aloft) takes the stunt the table
/// ranks highest; two or more make a combination, by their tiers.
pub fn award(t: &StuntTable, flip: (i32, i32), roll: (i32, i32), spin: (i32, i32), aloft: u32) -> Option<Award> {
    let mut air = Axis::default();
    let aloft_stunt = match aloft {
        0..1000 => None,
        1000..2000 => Some(0),
        2000..3000 => Some(1),
        3000..4000 => Some(2),
        _ => Some(3),
    };
    if let Some(s) = aloft_stunt {
        air.stunt = Some(s);
        air.turbos = t.turbos[s as usize] as u32;
        air.points = if s == 3 { ((aloft / 1000) as i32).wrapping_mul(t.points[3]) } else { t.points[s as usize] };
    }

    // Flips keep their sign: forward and backward score apart.
    let mut x = Axis::default();
    let (a, b) = Turn { peak: flip.0, rest: flip.1 }.rounded();
    let both = a.signum() != b.signum() && a.abs() >= 360 && b.abs() >= 360;
    let turned = if both {
        x.stunt = Some(12);
        x.add(t, 12);
        a.abs() + b.abs()
    } else if a.abs() < b.abs() {
        b
    } else {
        a
    };
    if turned >= 361 {
        x.tiers(t, turned, both, &[(1441, 11, 4), (1081, 10, 3), (721, 9, 2), (361, 8, 1)]);
    } else if turned < -360 {
        x.tiers(t, -turned, both, &[(1441, 7, 4), (1081, 6, 3), (721, 5, 2), (361, 4, 1)]);
    }

    // Rolls count either way.
    let mut y = Axis::default();
    let (a, b) = Turn { peak: roll.0, rest: roll.1 }.rounded();
    let (aa, ba) = (a.abs(), b.abs());
    let both = a.signum() != b.signum() && aa >= 360 && ba >= 360;
    let turned = if both {
        y.stunt = Some(34);
        y.add(t, 34);
        aa + ba
    } else {
        aa.max(ba)
    };
    y.tiers(t, turned, both, &[(1441, 33, 4), (1081, 32, 3), (721, 31, 2), (361, 30, 1)]);

    // Spins too, up to eight turns; both ways from two turns each.
    let mut z = Axis::default();
    let (a, b) = Turn { peak: spin.0, rest: spin.1 }.rounded();
    let (aa, ba) = (a.abs(), b.abs());
    let opposite = a.signum() != b.signum();
    let both = if opposite && aa >= 720 && ba >= 720 {
        Some(29)
    } else if opposite && aa >= 360 && ba >= 360 {
        Some(28)
    } else {
        None
    };
    let turned = match both {
        Some(s) => {
            z.stunt = Some(s);
            z.add(t, s);
            aa + ba
        }
        None => aa.max(ba),
    };
    z.tiers(
        t,
        turned,
        both.is_some(),
        &[
            (2881, 27, 8),
            (2701, 26, 7),
            (2521, 25, 7),
            (2341, 24, 6),
            (2161, 23, 6),
            (1981, 22, 5),
            (1801, 21, 5),
            (1621, 20, 4),
            (1441, 19, 4),
            (1261, 18, 3),
            (1081, 17, 3),
            (901, 16, 2),
            (721, 15, 2),
            (541, 14, 1),
            (361, 13, 1),
        ],
    );

    let points = [&air, &x, &y, &z].iter().fold(0i32, |s, a| s.wrapping_add(a.points));
    let turbos = [&air, &x, &y, &z].iter().map(|a| a.turbos).sum::<u32>();
    let several = (x.tier != 0) as u8 + (y.tier != 0) as u8 + (z.tier != 0) as u8;
    let award = if several >= 2 {
        let tier = |a: &Axis| if a.tier >= 3 { 2 } else { a.tier };
        let combination = match tier(&z) * 9 + tier(&y) * 3 + tier(&x) {
            4 => 35,
            5 => 37,
            7 => 36,
            8 => 38,
            10 => 43,
            11 => 45,
            12 => 39,
            15 => 40,
            19 => 44,
            20 => 46,
            21 => 41,
            24 => 42,
            26 => 48,
            13 | 14 | 16 | 17 | 22 | 23 | 25 => 47,
            _ => return None,
        };
        Award {
            stunt: combination,
            points: points.wrapping_add(t.points[combination as usize]),
            turbos: (turbos + t.turbos[combination as usize] as u32) as u8,
            name: None,
        }
    } else {
        let stunts = [air.stunt, x.stunt, y.stunt, z.stunt];
        let mut best = 0;
        for k in 1..4 {
            if stunts[k].is_some() && t.priority(stunts[best]) < t.priority(stunts[k]) {
                best = k;
            }
        }
        Award { stunt: stunts[best]?, points, turbos: turbos as u8, name: None }
    };
    Some(Award { turbos: award.turbos.min(10), ..award })
}

impl Car {
    /// The angular velocity in the car's own axes.
    fn own_spin(&self) -> Vec3 {
        let rot = self.body.rot;
        [0, 1, 2].map(|j| dot(column(&rot, j), self.body.spin))
    }

    /// 0x8003cb74, for player one's car, a step of `dt` seconds (4.12): when
    /// it leaves the ground the watch starts; in the air the time aloft
    /// runs and each axis's turn is summed, its turn so far kept each time
    /// it reverses. On a level landing the turns are awarded: points (when
    /// the race counts them, `scoring`), turbos, and an announcement (the
    /// award returned, with its name, for the HUD) and sounds.
    pub fn watch_stunt(&mut self, dt: i32, scoring: bool, t: &StuntTable, rand: &mut Rand) -> Option<Award> {
        let ms = fx(dt, 1000 << 12) >> 12;
        if !self.airborne {
            if self.grounded != 0 {
                return None;
            }
            self.airborne = true;
            self.air_ms = 0;
            self.stunt_spin = self.own_spin();
            self.stunt_turn = [0; 3];
            self.stunt_peak = [0; 3];
            return None;
        }
        let mut landed = None;
        if self.grounded_level == 0 {
            if self.grounded != 0 {
                self.airborne = false;
            }
            self.air_ms = self.air_ms.wrapping_add(ms as u32);
            let spin = self.own_spin();
            let axes = spin.iter().zip(&self.stunt_spin).zip(self.stunt_turn.iter_mut().zip(&mut self.stunt_peak));
            for ((&now, &before), (turn, peak)) in axes {
                if (now ^ before) < 0 && peak.wrapping_abs() < turn.wrapping_abs() {
                    *peak = *turn;
                }
                *turn = turn.wrapping_add(fx(now, dt));
            }
            self.stunt_spin = spin;
        } else {
            self.airborne = false;
            self.air_ms = self.air_ms.wrapping_add(ms as u32);
            let degrees = div_fx(180 << 12, 0x3244);
            let deg = |v: i32| fx(v, degrees) >> 12;
            let turn = |k: usize| {
                let peak = deg(self.stunt_peak[k]);
                (peak, deg(self.stunt_turn[k]).wrapping_sub(peak))
            };
            if let Some(mut a) = award(t, turn(0), turn(1), turn(2), self.air_ms)
                && a.points != 0
            {
                if self.flags & 1 != 0 && (a.points as u32 >= 1001 || rand.below(8) == 0) {
                    let line = if rand.below(2) == 0 { 2 } else { 1 };
                    tracing::trace!("stunt commentary {line}: not yet ported");
                }
                if !scoring {
                    a.points = 0;
                }
                // The announcement (0x80064dec): its name drawn now, shown
                // by the HUD.
                a.name = t.name(a.stunt, rand);
                self.stunt_points = self.stunt_points.wrapping_add(a.points);
                let added = self.add_turbos(a.turbos);
                tracing::trace!("stunt {} ({} turbos added): message and sound not yet ported", a.stunt, added);
                landed = Some(a);
            }
        }
        self.air_total_ms = self.air_total_ms.wrapping_add(ms as u32);
        landed
    }

    /// 0x8003c850: adds turbos, at most 10 in hand; how many were added.
    /// Reaching ten before the turbo was ever used plays a hint, once.
    pub fn add_turbos(&mut self, turbos: u8) -> u8 {
        let total = self.turbos.wrapping_add(turbos);
        self.turbos = total;
        let mut added = turbos;
        if total >= 11 {
            self.turbos = 10;
            added = turbos.wrapping_add(10).wrapping_sub(total);
        }
        if self.turbos == 10 && !self.turbo_hint {
            tracing::trace!("the ten-turbos line: not yet ported");
            self.turbo_hint = true;
        }
        added
    }
}
