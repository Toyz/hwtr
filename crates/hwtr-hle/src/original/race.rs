//! The original's memory as race: where it keeps it, read into and written
//! from the port's types.

use hwtr_game::race::{Driver, Entrant, RaceSetup};
use super::Ram;

pub const SETUP: u32 = 0x8013_8c94;
pub const ENTRANTS: u32 = 0x24;
pub const ENTRANT_SIZE: u32 = 13;
/// The players' names for the results (0x8009b5b8 copies them here), 12
/// bytes each.
pub const NAMES: u32 = 0x8013_99b0;

fn name(ram: &Ram, at: u32, len: u32) -> String {
    (0..len).map(|k| ram.u8(at + k)).take_while(|&b| b != 0).map(char::from).collect()
}

/// The race the original set up at `at` (usually [`SETUP`]).
pub fn setup(ram: &Ram, at: u32) -> RaceSetup {
    {
        let count = ram.u8(at + 0x18) as u32;
        let cars = (0..count)
            .map(|k| {
                let e = at + ENTRANTS + k * ENTRANT_SIZE;
                Entrant {
                    name: name(ram, e, 9),
                    driver: Driver::from_byte(ram.u8(e + 9)),
                    car_id: ram.u8(e + 10),
                    player: ram.u8(e + 11),
                    grid: ram.u8(e + 12),
                }
            })
            .collect();
        RaceSetup {
            flags: ram.i32(at) as u32,
            track: name(ram, at + 4, 19),
            track_number: ram.u8(at + 0x17),
            laps: ram.u8(at + 0x19),
            checkpoints: ram.u8(at + 0x1a),
            options: ram.i32(at + 0x1c) as u32,
            time_limit: ram.i32(at + 0x20) as u32,
            cars,
            difficulty: ram.u8(at + 0x72),
            best_line: Some(name(ram, at + 13, 10)).filter(|n| !n.is_empty()),
            names: [name(ram, NAMES, 12), name(ram, NAMES + 12, 12)],
        }
    }
}
