//! The original's memory as world: where it keeps it, read into and written
//! from the port's types.

use super::object::read_list;
use super::{InMemory, Ram};
use hwtr_game::collision::object::{CollisionObject, RefSet};
use hwtr_game::collision::pairs::Pair;
use hwtr_game::collision::scp::Scp;
use hwtr_game::collision::walls::Contact;
use hwtr_game::collision::world::{Collision, ObjectId};
use hwtr_game::laps::Course;

/// The race's sounds shut (6875(gp), set at the race's end by 0x800364cc).
pub const HUSHED: u32 = 0x800d_2623;
/// The race's options (collision_load copies the setup's +0x1c).
pub const OPTIONS: u32 = 0x800d_2678;
/// Each player's wait before its next contact jolt (0x8005fd4c).
pub const JOLT_WAIT: u32 = 0x800d_0e2c;
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
pub const PAIRS: u32 = 0x8012_c96c;
pub const PAIR_COUNT: u32 = 0x800d_2676;
pub const SEPARATIONS: u32 = 0x8012_cc2c;
pub const DEPTHS: u32 = 0x8012_cbec;
/// gameflow_load's lap rules: the laps, the checkpoints, flags 2, 4, 8.
pub const LAPS: u32 = 0x800d_0e40;
pub const CHECKPOINTS: u32 = 0x800d_0e44;
pub const QUIET: u32 = 0x800d_0e4e;
pub const ENDLESS: u32 = 0x800d_0e4d;
pub const FLYING: u32 = 0x800d_0e4c;
/// collision_load's lap length, checkpoint count and checkpoint starts.
pub const LAP_LENGTH: u32 = 0x800d_267c;
pub const START_COUNT: u32 = 0x800d_2680;
pub const STARTS: u32 = 0x8012_dcac;

/// The race's lap rules and the track's checkpoints as the original keeps
/// them.
pub fn course(ram: &Ram) -> Course {
    Course {
        laps: ram.u8(LAPS),
        checkpoints: ram.u8(CHECKPOINTS),
        quiet: ram.flag(QUIET),
        endless: ram.flag(ENDLESS),
        flying: ram.flag(FLYING),
        lap_length: ram.i32(LAP_LENGTH),
        starts: (0..ram.i32(START_COUNT) as u32).map(|k| ram.i32(STARTS + 4 * k)).collect(),
    }
}

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
            course: course(ram),
            lap_events: Vec::new(),
            pickups_touched: Vec::new(),
            players_touched: false,
            sparks: Vec::new(),
            prop_draws: Vec::new(),
            volume_fx: Vec::new(),
            hits: Vec::new(),
            triggers_hit: Vec::new(),
            hushed: ram.u8(HUSHED) == 1,
            options: ram.i32(OPTIONS) as u32,
            flying: super::object::flying(ram, &|a| addresses.iter().position(|&x| x == a)),
            jolts: Vec::new(),
            jolt_wait: [ram.i32(JOLT_WAIT), ram.i32(JOLT_WAIT + 4)],
            knocked: Vec::new(),
            pairs: (0..ram.i16(PAIR_COUNT) as u16 as u32)
                .map(|k| {
                    let at = PAIRS + k * CONTACT_SIZE;
                    Pair {
                        a: id(ram.i32(at) as u32),
                        b: id(ram.i32(at + 4) as u32),
                        point: ram.vec3(at + 8),
                        normal: ram.vec3(at + 0x18),
                    }
                })
                .collect(),
            separations: (0..32u32)
                .flat_map(|a| (0..128u32).map(move |b| (a, b)))
                .filter_map(|(a, b)| {
                    let v = ram.u8(SEPARATIONS + a * 128 + b);
                    (v != 0).then_some(((a as u16, b as u16), v))
                })
                .collect(),
            depths: std::array::from_fn(|k| ram.i32(DEPTHS + 4 * k as u32)),
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
    for a in 0..32u32 {
        for b in 0..128u32 {
            let v = world.separations.get(&(a as u16, b as u16)).copied().unwrap_or(0);
            ram.set_u8(SEPARATIONS + a * 128 + b, v);
        }
    }
    for (k, d) in world.depths.iter().enumerate() {
        ram.set_i32(DEPTHS + 4 * k as u32, *d);
    }
}
