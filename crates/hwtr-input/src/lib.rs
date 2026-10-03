//! The pad as the game sees it.
//!
//! [`Pad`] holds the buttons in the PlayStation controller's bit order (the
//! order the pad sends them, inverted so 1 is pressed) and the two analog
//! sticks as bytes, 0x80 at rest. [`Input`] fills it every frame from the
//! first connected gamepad and from the keyboard.

use gilrs::{Axis, Button, Gilrs};

/// Button bits as the PlayStation pad reports them (after inversion).
pub mod buttons {
    pub const SELECT: u16 = 1 << 0;
    pub const L3: u16 = 1 << 1;
    pub const R3: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const RIGHT: u16 = 1 << 5;
    pub const DOWN: u16 = 1 << 6;
    pub const LEFT: u16 = 1 << 7;
    pub const L2: u16 = 1 << 8;
    pub const R2: u16 = 1 << 9;
    pub const L1: u16 = 1 << 10;
    pub const R1: u16 = 1 << 11;
    pub const TRIANGLE: u16 = 1 << 12;
    pub const CIRCLE: u16 = 1 << 13;
    pub const CROSS: u16 = 1 << 14;
    pub const SQUARE: u16 = 1 << 15;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pad {
    pub buttons: u16,
    /// Left stick x, y and right stick x, y: 0 left/up, 0x80 centre, 0xff right/down.
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
}

impl Default for Pad {
    fn default() -> Pad {
        Pad { buttons: 0, lx: 0x80, ly: 0x80, rx: 0x80, ry: 0x80 }
    }
}

impl Pad {
    pub fn held(&self, b: u16) -> bool {
        self.buttons & b != 0
    }
}

const GAMEPAD: [(Button, u16); 16] = [
    (Button::DPadUp, buttons::UP),
    (Button::DPadDown, buttons::DOWN),
    (Button::DPadLeft, buttons::LEFT),
    (Button::DPadRight, buttons::RIGHT),
    (Button::South, buttons::CROSS),
    (Button::East, buttons::CIRCLE),
    (Button::West, buttons::SQUARE),
    (Button::North, buttons::TRIANGLE),
    (Button::LeftTrigger, buttons::L1),
    (Button::RightTrigger, buttons::R1),
    (Button::LeftTrigger2, buttons::L2),
    (Button::RightTrigger2, buttons::R2),
    (Button::Start, buttons::START),
    (Button::Select, buttons::SELECT),
    (Button::LeftThumb, buttons::L3),
    (Button::RightThumb, buttons::R3),
];

/// A stick axis from gilrs (-1 left or down, 1 right or up) as the pad's
/// byte (0 left or up). `flip` turns gilrs' up-positive into down-positive.
pub fn axis_byte(v: f32, flip: bool) -> u8 {
    let v = if flip { -v } else { v }.clamp(-1.0, 1.0);
    // 128 is centre: 128 steps below it, 127 above.
    let scale = if v < 0.0 { 128.0 } else { 127.0 };
    (128.0 + v * scale).round() as u8
}

/// Keys the keyboard maps to pad buttons, by a name the window layer
/// resolves: arrows, Z X A S for the face buttons, Q W for L1 R1, 1 2 for L2
/// R2, Enter for Start, Shift for Select.
#[derive(Clone, Copy, Debug, Default)]
pub struct Keyboard {
    pub buttons: u16,
}

pub struct Input {
    gilrs: Option<Gilrs>,
    active: Option<gilrs::GamepadId>,
    rumble: Option<Rumble>,
    pub keyboard: Keyboard,
}

impl Input {
    pub fn new() -> Input {
        // No gilrs filters: its dead zone rescales the stick; the game has
        // its own.
        let gilrs = gilrs::GilrsBuilder::new()
            .with_default_filters(false)
            .build()
            .map_err(|e| tracing::warn!("no gamepad support: {e}"))
            .ok();
        Input { gilrs, active: None, rumble: None, keyboard: Keyboard::default() }
    }

    /// Whether a gamepad is connected (so the pad is an analog one).
    pub fn has_gamepad(&self) -> bool {
        self.active.is_some()
    }

    /// Names of the connected gamepads.
    pub fn gamepads(&self) -> Vec<String> {
        self.gilrs.as_ref().map_or(Vec::new(), |g| {
            g.gamepads().filter(|(_, p)| p.is_connected()).map(|(_, p)| p.name().to_string()).collect()
        })
    }

    /// Reads the pad: the gamepad that last sent an event (the first connected
    /// one until then), merged with the keyboard.
    pub fn read(&mut self) -> Pad {
        let mut pad = Pad { buttons: self.keyboard.buttons, ..Pad::default() };
        let Some(g) = self.gilrs.as_mut() else { return pad };
        while let Some(ev) = g.next_event() {
            self.active = Some(ev.id);
        }
        let id = self.active.filter(|id| g.connected_gamepad(*id).is_some()).or_else(|| {
            let first = g.gamepads().find(|(_, p)| p.is_connected()).map(|(id, _)| id);
            self.active = first;
            first
        });
        let Some(p) = id.and_then(|id| g.connected_gamepad(id)) else { return pad };
        for (b, bit) in GAMEPAD {
            if p.is_pressed(b) {
                pad.buttons |= bit;
            }
        }
        pad.lx = axis_byte(p.value(Axis::LeftStickX), false);
        pad.ly = axis_byte(p.value(Axis::LeftStickY), true);
        pad.rx = axis_byte(p.value(Axis::RightStickX), false);
        pad.ry = axis_byte(p.value(Axis::RightStickY), true);
        pad
    }

    /// Runs the pad's motors: `large` 0-255 for the big motor, `small` on or
    /// off, as the DualShock takes them.
    pub fn rumble(&mut self, large: u8, small: bool) {
        if self.rumble.is_none() {
            self.rumble = self.gilrs.as_mut().and_then(Rumble::new);
        }
        if let Some(r) = &self.rumble {
            r.set(large, small);
        }
    }
}

impl Default for Input {
    fn default() -> Self {
        Input::new()
    }
}

struct Rumble {
    strong: gilrs::ff::Effect,
    weak: gilrs::ff::Effect,
}

impl Rumble {
    fn new(gilrs: &mut Gilrs) -> Option<Rumble> {
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
        let id = gilrs.gamepads().find(|(_, p)| p.is_connected() && p.is_ff_supported()).map(|(id, _)| id)?;
        let effect = |kind: BaseEffectType, gilrs: &mut Gilrs| {
            EffectBuilder::new()
                .add_effect(BaseEffect {
                    kind,
                    scheduling: Replay { play_for: Ticks::from_ms(1000), ..Default::default() },
                    ..Default::default()
                })
                .gamepads(&[id])
                .finish(gilrs)
                .map_err(|e| tracing::warn!("rumble: {e}"))
                .ok()
        };
        let strong = effect(BaseEffectType::Strong { magnitude: u16::MAX }, gilrs)?;
        let weak = effect(BaseEffectType::Weak { magnitude: u16::MAX }, gilrs)?;
        Some(Rumble { strong, weak })
    }

    fn set(&self, large: u8, small: bool) {
        let r = if large > 0 {
            self.strong.set_gain(f32::from(large) / 255.0).and_then(|_| self.strong.play())
        } else {
            self.strong.stop()
        };
        let r = r.and_then(|_| if small { self.weak.play() } else { self.weak.stop() });
        if let Err(e) = r {
            tracing::warn!("rumble: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_bytes() {
        assert_eq!(axis_byte(0.0, false), 128);
        assert_eq!(axis_byte(1.0, false), 255);
        assert_eq!(axis_byte(-1.0, false), 0);
        assert_eq!(axis_byte(1.0, true), 0, "stick up is 0");
    }
}
