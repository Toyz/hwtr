//! The original's memory as cameras: 100 bytes a player from 0x80128f04,
//! read into and written from the port's type.

use hwtr_game::camera::{Camera, ViewMode};

use super::{InMemory, Ram};

pub const CAMERAS: u32 = 0x8012_8f04;
pub const CAMERA_SIZE: u32 = 100;
/// How many cameras the race has, and how many views each offers.
pub const COUNT: u32 = 0x800d_2633;
pub const VIEWS: u32 = 0x800d_2634;

const POS: u32 = 0x04;
const VEL: u32 = 0x14;
const ROT: u32 = 0x24;
const FOV: u32 = 0x44;
const MODE: u32 = 0x48;
const VIEW: u32 = 0x49;
const CAR: u32 = 0x4a;
const BUTTON: u32 = 0x4b;
const BUTTON_BEFORE: u32 = 0x4c;
const SNAP: u32 = 0x4d;
const SHAKE: u32 = 0x54;
const ZOOM: u32 = 0x58;
const INTRO: u32 = 0x60;
const FLYBY: u32 = 0x5c;

/// Player `k`'s camera.
pub fn at(k: u32) -> u32 {
    CAMERAS + k * CAMERA_SIZE
}

impl InMemory for Camera {
    fn read(ram: &Ram, at: u32) -> Camera {
        Camera {
            car: ram.u8(at + CAR),
            view: ram.u8(at + VIEW),
            mode: Some(match ram.u8(at + MODE) {
                0 => ViewMode::Mounted,
                1 => ViewMode::Chase,
                b => ViewMode::Other(b),
            }),
            pos: ram.vec3(at + POS),
            vel: ram.vec3(at + VEL),
            rot: std::array::from_fn(|i| std::array::from_fn(|j| ram.i16(at + ROT + 6 * i as u32 + 2 * j as u32))),
            fov: ram.i32(at + FOV),
            snap: ram.flag(at + SNAP),
            button: ram.flag(at + BUTTON),
            button_before: ram.flag(at + BUTTON_BEFORE),
            zoom: ram.i32(at + ZOOM),
            shake: ram.i32(at + SHAKE) as u32,
            intro_ms: ram.i32(at + INTRO),
            flyby: ram.flag(at + FLYBY),
        }
    }

    fn write(&self, ram: &mut Ram, at: u32) {
        ram.set_u8(at + CAR, self.car);
        ram.set_u8(at + VIEW, self.view);
        if let Some(mode) = self.mode {
            ram.set_u8(at + MODE, match mode {
                ViewMode::Mounted => 0,
                ViewMode::Chase => 1,
                ViewMode::Other(b) => b,
            });
        }
        ram.set_vec3(at + POS, self.pos);
        ram.set_vec3(at + VEL, self.vel);
        for (i, row) in self.rot.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                ram.set_i16(at + ROT + 6 * i as u32 + 2 * j as u32, *v);
            }
        }
        ram.set_i32(at + FOV, self.fov);
        ram.set_flag(at + SNAP, self.snap);
        ram.set_flag(at + BUTTON, self.button);
        ram.set_flag(at + BUTTON_BEFORE, self.button_before);
        ram.set_i32(at + ZOOM, self.zoom);
        ram.set_i32(at + SHAKE, self.shake as i32);
        ram.set_i32(at + INTRO, self.intro_ms);
        ram.set_flag(at + FLYBY, self.flyby);
    }
}
