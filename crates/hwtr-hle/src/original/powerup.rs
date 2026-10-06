//! The original's memory as power-ups: the loaded power-ups' list
//! (0x800d2698), the pickups (0x800d0e9c, 0x800d0ea0 of them), each car's
//! held list (0x8012ff04), the unlock record, and the race's clock.

use super::Ram;
use hwtr_game::powerup::{Held, PUP_SIZE, Pickup, PowerUp, PowerUps};

/// The race's time, ms (0x8006122c reads it).
pub const CLOCK: u32 = 0x800d_0e34;
/// The list of loaded power-ups.
pub const DEFS: u32 = 0x800d_2698;
/// The pickups (24 bytes each) and how many.
pub const PICKUPS: u32 = 0x800d_0e9c;
pub const PICKUP_COUNT: u32 = 0x800d_0ea0;
pub const PICKUP_SIZE: u32 = 24;
/// Each car's list of held power-ups (12 bytes each: the power-up, since
/// when, its kind, whether it never runs out).
pub const HELD: u32 = 0x8012_ff04;
/// The track's two cars to unlock, and the record of those taken: the
/// first car for players 1 and 2, then the second.
pub const UNLOCKABLE: u32 = 0x800d_0ea6;
pub const UNLOCKED: u32 = 0x800d_0eaa;

/// A list's items, head first (each node: the item, the previous node, the
/// next, a count).
pub fn list(ram: &Ram, header: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut node = ram.i32(header) as u32;
    while node != 0 && out.len() < 256 {
        out.push(ram.i32(node) as u32);
        node = ram.i32(node + 8) as u32;
    }
    out
}

fn text(ram: &Ram, at: u32) -> String {
    (0..32).map(|k| ram.u8(at + k)).take_while(|&c| c != 0).map(char::from).collect()
}

/// The loaded power-ups as the list holds them (newest first), with where
/// each is.
pub fn defs(ram: &Ram) -> Vec<(u32, PowerUp)> {
    list(ram, ram.i32(DEFS) as u32)
        .into_iter()
        .map(|a| {
            let bytes: Vec<u8> = (0..PUP_SIZE as u32).map(|k| ram.u8(a + k)).collect();
            (a, PowerUp::parse(&bytes).expect("a loaded power-up"))
        })
        .collect()
}

/// Where pickup `k` is.
pub fn pickup_at(ram: &Ram, k: u32) -> u32 {
    ram.i32(PICKUPS) as u32 + PICKUP_SIZE * k
}

/// Pickup `k` (its place and object are the world's, left out).
pub fn pickup(ram: &Ram, k: u32) -> Pickup {
    let at = pickup_at(ram, k);
    Pickup {
        name: text(ram, ram.i32(at) as u32),
        index: ram.u8(at + 4) as u16,
        number: ram.i32(at + 8) as u8,
        taken_at: ram.i32(at + 12) as u32,
        out: ram.u8(at + 16) != 0,
        pos: [0; 3],
        object: None,
    }
}

/// The race's power-ups for `cars` cars. Each car's held list is turned
/// oldest first, the order the port keeps.
pub fn read(ram: &Ram, cars: usize) -> PowerUps {
    let defs = defs(ram);
    let held = (0..cars as u32)
        .map(|slot| {
            let mut held: Vec<Held> = list(ram, ram.i32(HELD + 4 * slot) as u32)
                .into_iter()
                .map(|e| Held {
                    power_up: defs.iter().position(|&(a, _)| a == ram.i32(e) as u32).expect("a loaded power-up"),
                    since: ram.i32(e + 4) as u32,
                    instant: ram.u8(e + 9) != 0,
                })
                .collect();
            held.reverse();
            held
        })
        .collect();
    let note = |a: u32| match ram.i16(a) {
        -1 => None,
        v => Some(v as u8),
    };
    PowerUps {
        pickups: (0..ram.i32(PICKUP_COUNT) as u32).map(|k| pickup(ram, k)).collect(),
        defs: defs.into_iter().map(|(_, d)| d).collect(),
        held,
        unlockable: [ram.i16(UNLOCKABLE) as u8, ram.i16(UNLOCKABLE + 2) as u8],
        unlocked: [[note(UNLOCKED), note(UNLOCKED + 4)], [note(UNLOCKED + 2), note(UNLOCKED + 6)]],
        changed: false,
    }
}

/// Pickup `k`'s state written back: out or not, and when taken.
pub fn write_pickup(ram: &mut Ram, k: u32, p: &Pickup) {
    let at = pickup_at(ram, k);
    ram.set_i32(at + 12, p.taken_at as i32);
    ram.set_u8(at + 16, p.out as u8);
}
