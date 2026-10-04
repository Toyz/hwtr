//! The original's memory as pad: where it keeps it, read into and written
//! from the port's types.

use hwtr_game::pad::PadReader;
use super::{InMemory, Ram};

pub const LEVELS: u32 = 0x8011_b2b8;
pub const PORT_SIZE: u32 = 0x62;
pub const HELD: u32 = 0x10;
pub const EASED: u32 = 0x16;
/// libpad's receive buffer for port 1 (port 2 34 bytes on).
pub const PAD_BUFFER: u32 = 0x8011_b388;
/// The player's button mapping (0x74 bytes a configuration).
pub const MAPPING: u32 = 0x8011_b3d8;

/// Where a port's levels are.
pub fn port(port: u32) -> u32 {
    LEVELS + port * PORT_SIZE
}

impl InMemory for PadReader {
    fn read(ram: &Ram, at: u32) -> PadReader {
        PadReader {
            levels: [0, 1, 2, 3, 4, 5, 6, 7].map(|k| ram.i16(at + 2 * k) as u16),
            held: [0, 1, 2, 3, 4].map(|k| ram.u8(at + HELD + k)),
            eased: ram.i16(at + EASED),
        }
    }

    fn write(&self, ram: &mut Ram, at: u32) {
        for (k, l) in self.levels.iter().enumerate() {
            ram.set_i16(at + 2 * k as u32, *l as i16);
        }
        for (k, h) in self.held.iter().enumerate() {
            ram.set_u8(at + HELD + k as u32, *h);
        }
        ram.set_i16(at + EASED, self.eased);
    }
}
