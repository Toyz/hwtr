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
    /// The low byte of the buttons: Select, L3, R3, Start, and the d-pad.
    fn low(&self) -> u32 {
        (self.buttons & 0xff) as u32
    }

    /// The high byte: the shoulders and the face buttons.
    fn high(&self) -> u32 {
        (self.buttons >> 8) as u32
    }
}

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
            handbrake: l(8),
            action_9: l(9),
            action_10: l(10),
        }
    }
}
