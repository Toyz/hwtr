//! A car's contacts with the track: how long they have lasted, and whether
//! one was hard enough to wreck it.

use super::Car;
use crate::math::{Vec3, column, div_fx, dot, fx};

impl Car {
    /// 0x8003caec, at a step's first contact: contacts at most 100 ms apart
    /// (by the system clock, `clock`) run on, longer gaps start again; half
    /// a second of them ends a stunt.
    pub fn contact_timers(&mut self, clock: u32) {
        let gap = clock.wrapping_sub(self.contact_clock);
        self.contact_ms = if gap <= 100 { self.contact_ms.wrapping_add(gap) } else { 0 };
        self.contact_clock = clock;
        if self.airborne && self.contact_ms > 500 {
            self.airborne = false;
        }
    }

    /// 0x8007db14: whether hitting a surface with normal `n` wrecks the
    /// car. Its speed into the surface is set against a limit that rises
    /// with its toughness (its handling's skill, held to 0.25..4) and with
    /// how much the surface faces its roof: from -40 mph at 0.25 to 0 at 1,
    /// then to 35 mph at 4, and 7.5 mph on top.
    pub fn hard_impact(&self, n: Vec3) -> bool {
        let toughness = self.handling.skill.clamp(0x400, 0x4000);
        let rot = self.body.rot;
        let toughness = toughness.wrapping_add(dot(n, column(&rot, 2)).max(0));
        let into = dot(self.body.vel, n).wrapping_neg();
        let mph = div_fx(176 << 12, 10 << 12);
        let base = fx(0x7_8000, mph);
        let (low, from, rise, over) = if toughness < 0x1000 {
            (fx(-40 << 12, mph), toughness.wrapping_sub(0x400), fx(0, mph).wrapping_sub(fx(-40 << 12, mph)), 0xc00)
        } else {
            (fx(0, mph), toughness.wrapping_sub(0x1000), fx(35 << 12, mph).wrapping_sub(fx(0, mph)), 0x3000)
        };
        let limit = base.wrapping_add(low.wrapping_add(fx(from, div_fx(rise, over))));
        limit < into
    }
}
