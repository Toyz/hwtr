//! The original's memory as world: where it keeps it, read into and written
//! from the port's types.

use hwtr_game::collision::world::{Collision, ObjectId};
use super::object::read_list;
use hwtr_game::collision::object::{CollisionObject, RefSet};
use hwtr_game::collision::scp::Scp;
use hwtr_game::collision::walls::Contact;
use super::{InMemory, Ram};

pub const SCP: u32 = 0x800d_2658;
pub const ALL: u32 = 0x800d_265c;
pub const MOVING: u32 = 0x800d_2660;
pub const PLAYERS: u32 = 0x800d_2664;
pub const CARS: u32 = 0x800d_2668;
pub const WALLS: u32 = 0x800d_266c;
pub const COMPUTERS: u32 = 0x800d_2670;
pub const STEP: u32 = 0x800d_2654;
pub const CONTACTS: u32 = 0x8012_c6ec;
pub const CONTACT_COUNT: u32 = 0x800d_2674;
pub const CONTACT_SIZE: u32 = 40;

/// The SCP's bytes as loaded, and where.
pub fn scp_bytes(ram: &Ram) -> (u32, Vec<u8>) {
    let at = ram.i32(SCP) as u32;
    let sizes = [20, 24, 12, 20, 12, 32];
    let len = 240 + (0..6).map(|k| ram.i32(at + 4 * k) as usize * sizes[k as usize]).sum::<usize>();
    (at, (0..len as u32).map(|k| ram.u8(at + k)).collect())
}

/// The collision world as the original has it, with the objects'
/// addresses.
pub fn collision(ram: &Ram) -> (Collision, Vec<u32>) {
    {
        let (scp_at, bytes) = scp_bytes(ram);
        let scp = Scp::parse(&bytes).expect("the loaded SCP parses");
        let all = read_list(ram, ALL);
        let addresses: Vec<u32> = all.iter().collect();
        let id = |a: u32| -> ObjectId { addresses.iter().position(|&x| x == a).expect("object in the list") };
        let objects = addresses.iter().map(|&o| CollisionObject::read(ram, o)).collect();
        let set =
            |head: u32| RefSet { entries: read_list(ram, head).entries.iter().map(|&(a, n)| (id(a), n)).collect() };
        let members = (0..scp.zones.len() as u32).map(|z| set(scp_at + 240 + 20 * z + 16)).collect();
        let world = Collision {
            scp,
            objects,
            members,
            all: set(ALL),
            moving: set(MOVING),
            players: set(PLAYERS),
            cars: set(CARS),
            walls: set(WALLS),
            computers: set(COMPUTERS),
            step: ram.i32(STEP) as u32,
            contacts: (0..ram.i16(CONTACT_COUNT) as u16 as u32)
                .map(|k| {
                    let at = CONTACTS + k * CONTACT_SIZE;
                    Contact {
                        object: id(ram.i32(at) as u32),
                        point: ram.vec3(at + 4),
                        normal: ram.vec3(at + 0x14),
                        surface: ram.u8(at + 0x24),
                    }
                })
                .collect(),
        };
        (world, addresses)
    }

}

/// Writes the world's objects back over the original's (the lists are
/// compared, not written).
pub fn write_collision(world: &Collision, ram: &mut Ram, addresses: &[u32]) {
    for (obj, &at) in world.objects.iter().zip(addresses) {
        obj.write(ram, at);
    }
    ram.set_i32(STEP, world.step as i32);
}
