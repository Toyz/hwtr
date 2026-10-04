//! The controller as the game reads it, once a frame per port (0x8001bec0
//! and what it calls): the pad's buttons and sticks become levels of 0 to
//! 255 for the race's actions, through the player's button mapping.
//!
//! A digital pad's buttons press their actions fully, except steering, which
//! ramps up while held; in analog mode the sticks drive steering, the pedals
//! (the right stick) and the stick, through response curves, and the
//! d-pad, Cross and Square do nothing there.

use crate::car::Controls;

/// What kind of controller a port reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PadKind {
    /// 0x41: the digital pad.
    #[default]
    Digital,
    /// 0x73: a DualShock in analog mode.
    Analog,
}

/// A controller's state: its kind, the buttons held (bit 0 Select to bit 15
/// Square, as libpad orders them), and the sticks (0x80 at rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadState {
    pub kind: PadKind,
    pub buttons: u16,
    pub left: [u8; 2],
    pub right: [u8; 2],
}

impl Default for PadState {
    fn default() -> PadState {
        PadState { kind: PadKind::Digital, buttons: 0, left: [0x80; 2], right: [0x80; 2] }
    }
}

impl PadState {
    /// 0x8001aebc: whether the pad holds front-end action `action`: the
    /// d-pad and Start (14 to 17, 26) on the low byte, the face buttons and
    /// shoulders (18 to 25) on the high, any button (27) on either.
    pub fn holds(&self, map: &Mapping, action: u8) -> bool {
        let Some(&mask) = map.0.get(action as usize) else { return false };
        match action {
            14..=17 | 26 => self.low() & mask != 0,
            18..=25 => self.high() & mask != 0,
            27 => (self.high() | self.low()) & mask != 0,
            _ => false,
        }
    }

    /// The low byte of the buttons: Select, L3, R3, Start, and the d-pad.
    fn low(&self) -> u32 {
        (self.buttons & 0xff) as u32
    }

    /// The high byte: the shoulders and the face buttons.
    fn high(&self) -> u32 {
        (self.buttons >> 8) as u32
    }
}

/// The front-end actions: accept (Cross) and start (Start).
pub const ACCEPT: u8 = 18;
pub const START: u8 = 26;

/// A player's button mapping: each action's mask on its byte of the
/// buttons (the low byte for steering and the stick, actions 0, 1 and 4 to
/// 7, the high byte for the rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping(pub [u32; 29]);

impl Default for Mapping {
    /// `control_mapping_default` (0x800bdc08): Left and Right steer, Cross
    /// accelerates, Square brakes, the d-pad is the stick, L2 the handbrake,
    /// then R1, R2, Circle and Triangle.
    fn default() -> Mapping {
        Mapping([
            0x80, 0x20, 0x40, 0x80, 0x20, 0x80, 0x40, 0x10, 0x01, 0x08, 0x02, 0x20, 0x10, 0x08, 0x10, 0x40, 0x80, 0x20,
            0x40, 0x10, 0x80, 0x10, 0x04, 0x08, 0x01, 0x02, 0x08, 0xff, 0x01,
        ])
    }
}

/// A 7-point response curve (x, y), x rising, read by straight lines
/// between the points (0x8001d9ec).
type Curve = [(u8, u8); 7];

/// The curve for the left stick's steering and the right stick's pedals
/// (0x800bdbdc): a dead zone of 10, then gentle until 190.
const PEDAL_CURVE: Curve = [(0, 0), (10, 0), (50, 5), (100, 17), (190, 70), (245, 255), (255, 255)];

/// The curve for the stick actions (0x800bdbec).
const STICK_CURVE: Curve = [(0, 0), (50, 30), (100, 45), (175, 80), (220, 145), (245, 255), (255, 255)];

/// 0x8001d9ec: `x` on `curve`, 0 past its last point.
fn on_curve(curve: &Curve, x: u8) -> u8 {
    curve
        .windows(2)
        .find(|w| w[0].0 <= x && x <= w[1].0)
        .map_or(0, |w| {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            // 0x8001d970, the division by zero as the R3000A leaves it.
            let rise = (x as i32 - x0 as i32) * (y1 as i32 - y0 as i32);
            let run = x1 as i32 - x0 as i32;
            let step = if run == 0 { if rise >= 0 { -1 } else { 1 } } else { rise / run };
            (y0 as i32 + step).clamp(0, 255) as u8
        })
}

/// One port's levels as the reads leave them (0x8011b2b8, 0x62 bytes a
/// port).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PadReader {
    /// Brake, accelerate, steer right, steer left, stick right, stick left,
    /// stick down, stick up (actions 3, 2, 1, 0, 4, 5, 6, 7), 0 to 255.
    pub levels: [u16; 8],
    /// Actions 8 to 12 held.
    pub held: [u8; 5],
    /// The pedals' level eased toward its target 7 a frame (accelerate, or
    /// failing that brake, or 0).
    pub eased: i16,
}

const BRAKE: usize = 0;
const ACCELERATE: usize = 1;
const STEER_RIGHT: usize = 2;
const STEER_LEFT: usize = 3;
const STICK_RIGHT: usize = 4;
const STICK_LEFT: usize = 5;
const STICK_DOWN: usize = 6;
const STICK_UP: usize = 7;

impl PadReader {
    /// 0x8001bec0: one frame's read of `pad`, `elapsed_ms` since the last.
    /// (The vibration timers it also runs are not yet ported.)
    pub fn read(&mut self, pad: &PadState, map: &Mapping, elapsed_ms: u32) {
        self.read_held(pad, map);
        match pad.kind {
            PadKind::Digital => self.read_digital(pad, map, elapsed_ms),
            PadKind::Analog => self.read_analog(pad),
        }
    }

    /// 0x8001bf1c: actions 8 to 12, on or off.
    fn read_held(&mut self, pad: &PadState, map: &Mapping) {
        for (k, held) in self.held.iter_mut().enumerate() {
            *held = (pad.high() & map.0[8 + k] != 0) as u8;
        }
    }

    /// 0x8001c480 and 0x8001c998: the pedals full on or off; steering
    /// ramps while held, 40 a frame until 75 and then 1.8 a millisecond (the
    /// other way's level dropped, left winning over right); the stick ramps
    /// 45 a frame.
    fn read_digital(&mut self, pad: &PadState, map: &Mapping, elapsed_ms: u32) {
        let (low, high) = (pad.low(), pad.high());
        let ramp = (elapsed_ms.wrapping_mul(1800) / 1000).min(255) as i32;
        self.levels[ACCELERATE] = if high & map.0[2] != 0 { 255 } else { 0 };
        self.levels[BRAKE] = if high & map.0[3] != 0 { 255 } else { 0 };
        for (action, this, other) in [(1, STEER_RIGHT, STEER_LEFT), (0, STEER_LEFT, STEER_RIGHT)] {
            if low & map.0[action] == 0 {
                self.levels[this] = 0;
                continue;
            }
            self.levels[other] = 0;
            let level = self.levels[this] as i16 as i32;
            let level = if level < 75 { level + 40 } else { level + ramp };
            self.levels[this] = level.min(255) as u16;
        }
        self.ease();
        for (action, at) in [(7, STICK_UP), (6, STICK_DOWN), (4, STICK_RIGHT), (5, STICK_LEFT)] {
            self.levels[at] =
                if low & map.0[action] != 0 { (self.levels[at] as i16 as i32 + 45).min(255) as u16 } else { 0 };
        }
    }

    /// 0x8001cee4 for a DualShock: each stick axis split into its two
    /// halves, each doubled and read through its curve.
    fn read_analog(&mut self, pad: &PadState) {
        let [lx, ly] = pad.left.map(|v| v as i32);
        let ry = pad.right[1] as i32;
        let high = |v: i32| ((v - 128).max(0) * 2) as u8;
        let low = |v: i32| ((127 - v).max(0) * 2) as u8;
        let pedal = |v: u8| on_curve(&PEDAL_CURVE, v) as u16;
        let stick = |v: u8| on_curve(&STICK_CURVE, v) as u16;
        self.levels = [
            pedal(high(ry)),
            pedal(low(ry)),
            pedal(high(lx)),
            pedal(low(lx)),
            stick(high(lx)),
            stick(low(lx)),
            stick(high(ly)),
            stick(low(ly)),
        ];
        self.ease();
    }

    /// The eased pedal level: toward accelerate's level if pressed, else
    /// brake's, else down to 0, 7 a frame.
    fn ease(&mut self) {
        let mut e = self.eased as i32;
        let target = [self.levels[ACCELERATE], self.levels[BRAKE]].into_iter().find(|&l| l != 0);
        match target.map(|l| l as i16 as i32) {
            Some(t) if e < t => e = (e + 7).min(t),
            Some(t) if t < e => e = (e - 7).max(t),
            Some(_) => {}
            None => e = (e - 7).max(0),
        }
        self.eased = e.clamp(0, 255) as i16;
    }

    /// 0x8001b170: action `action`'s level, 0 to 255.
    pub fn level(&self, action: u8) -> u8 {
        let at = match action {
            0 => STEER_LEFT,
            1 => STEER_RIGHT,
            2 => ACCELERATE,
            3 => BRAKE,
            4..=7 => action as usize,
            8..=12 => return self.held[action as usize - 8],
            _ => return 0,
        };
        self.levels[at] as u8
    }

    /// The race's actions from these levels.
    pub fn controls(&self) -> Controls {
        let l = |a: u8| self.level(a);
        Controls {
            steer_left: l(0),
            steer_right: l(1),
            accelerate: l(2),
            brake: l(3),
            stick_across: [l(4), l(5)],
            stick_along: [l(6), l(7)],
            handbrake: l(8) != 0,
            reset: l(9) != 0,
            turbo: l(10) != 0,
            view: l(11) != 0,
        }
    }
}

impl Mapping {
    /// The controls screen's first line, "Vibration" (the mapping's last
    /// word, 0x8001d578): the motors run only while it is on.
    pub fn vibration(&self) -> bool {
        self.0[28] != 0
    }
}

/// A DualShock's motors as the game drives them (each port's actuator
/// bytes, 0x8011b2b8 +0x18 and +0x19): the small one on or off, the large
/// one's power.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motors {
    pub small: bool,
    pub large: u8,
    /// When the small one was turned on, ms on the system clock.
    small_from: u32,
}

impl Motors {
    /// 0x8001b7b4: the road's feel: `roughness` (0 is smooth) and `speed`,
    /// 0 to 255. Rough ground at more than a crawl runs the large motor at
    /// the roughness (at least 96) times the speed (at least 64), over 256.
    pub fn rumble(&mut self, on: bool, roughness: u8, speed: u8) {
        if !on || roughness == 0 || speed < 11 {
            return;
        }
        let r = roughness.max(96) as u32;
        let power = if speed < 64 { r << 6 } else { r * speed as u32 };
        self.large = ((power & 0xffff) >> 8).min(255) as u8;
    }

    /// 0x8001b934: a jolt of `level`: the large motor at two and a half
    /// times it (below 3 nothing, below 40 as 40), and from 110 the small
    /// one too, for half a second from `now`.
    pub fn jolt(&mut self, on: bool, level: u8, now: u32) {
        if !on {
            return;
        }
        let s = match level {
            0..3 => 0,
            3..40 => 40,
            l => l,
        } as u32;
        self.large = (s * 2 + s / 2).min(255) as u8;
        if level >= 110 {
            self.small = true;
            self.small_from = now;
        }
    }

    /// 0x8001ccc4, at each read of the pad `elapsed_ms` after the last: the
    /// large motor winds down a step each 8 ms (at most 125 a read);
    /// the small one stops after half a second. Both stop at once when the
    /// race is not `running` (the game's state 0, 2 or 3).
    pub fn fade(&mut self, on: bool, elapsed_ms: u32, running: bool, now: u32) {
        if !on {
            return;
        }
        let step = (elapsed_ms >> 3).min(125) as i32;
        if self.large != 0 {
            self.large = if running { (self.large as i32 - step).max(0) as u8 } else { 0 };
        }
        if self.small && (!running || now.wrapping_sub(self.small_from) >= 500) {
            self.small = false;
        }
    }
}
