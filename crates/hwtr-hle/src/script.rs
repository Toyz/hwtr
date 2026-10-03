//! Scripted input for headless runs: buttons held and stick positions over
//! frame ranges.

use crate::Hle;

/// A button's bit by name.
pub fn button(name: &str) -> u16 {
    match name.to_lowercase().as_str() {
        "select" => 1 << 0,
        "l3" => 1 << 1,
        "r3" => 1 << 2,
        "start" => 1 << 3,
        "up" => 1 << 4,
        "right" => 1 << 5,
        "down" => 1 << 6,
        "left" => 1 << 7,
        "l2" => 1 << 8,
        "r2" => 1 << 9,
        "l1" => 1 << 10,
        "r1" => 1 << 11,
        "triangle" => 1 << 12,
        "circle" => 1 << 13,
        "cross" | "x" => 1 << 14,
        "square" => 1 << 15,
        _ => 0,
    }
}

#[derive(Default)]
pub struct Script {
    /// (first frame, buttons, frames held).
    pub presses: Vec<(u64, u16, u64)>,
    /// (first frame, left stick x, y, frames held).
    pub sticks: Vec<(u64, u8, u8, u64)>,
    /// A DualShock in analog mode rather than a digital pad.
    pub analog: bool,
}

impl Script {
    /// `FRAME:BUTTONS[:LENGTH],...`, buttons by name joined with `+`;
    /// LENGTH defaults to 4.
    pub fn press(&mut self, spec: &str) {
        for p in spec.split(',') {
            let mut it = p.split(':');
            let at = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let bits = it.next().unwrap_or("").split('+').map(button).fold(0, |a, b| a | b);
            let len = it.next().and_then(|s| s.parse().ok()).unwrap_or(4);
            self.presses.push((at, bits, len));
        }
    }

    /// `FRAME:LX,LY[:LENGTH];...`, the left stick (128 at rest); LENGTH
    /// defaults to 60. Implies an analog pad.
    pub fn stick(&mut self, spec: &str) {
        for p in spec.split(';') {
            let mut it = p.split(':');
            let at = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let xy: Vec<u8> = it.next().unwrap_or("128,128").split(',').filter_map(|v| v.parse().ok()).collect();
            let len = it.next().and_then(|s| s.parse().ok()).unwrap_or(60);
            self.sticks.push((at, *xy.first().unwrap_or(&128), *xy.get(1).unwrap_or(&128), len));
        }
        self.analog = true;
    }

    /// Sets the pad for frame `f`.
    pub fn apply(&self, hle: &mut Hle, f: u64) {
        if self.analog {
            let (lx, ly) =
                self.sticks.iter().rev().find(|s| f >= s.0 && f < s.0 + s.3).map_or((128, 128), |s| (s.1, s.2));
            hle.sticks = Some([lx, ly, 128, 128]);
        }
        hle.pad = self.presses.iter().filter(|p| f >= p.0 && f < p.0 + p.2).fold(0, |a, p| a | p.1);
    }
}
