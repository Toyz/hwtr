//! The track's moving objects against the original: each of DESERT1's
//! animations posed at a spread of times (0x8007f2a4), the object's
//! rotation and position compared.

mod common;

use hwtr_game::math::Tables;
use hwtr_game::world_anim::{ObjectAnim, WorldAnims};
use hwtr_hle::original::Ram;

/// The world's header (0x800d2540), the animations' run-time records
/// (0x800d0ffc, 116 bytes each).
const WORLD: u32 = 0x800d_2540;
const RUNS: u32 = 0x800d_0ffc;

#[test]
fn objects_move_as_the_original_moves_them() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/DESERT1BIG/DESERT1WLD");
    let Ok(file) = std::fs::read(path) else { return };
    let world = hwtr_data::world::World::parse(&file).expect("the world parses");
    let t = Tables::from_exe(&exe);
    let anims: Vec<ObjectAnim> = world
        .anims
        .iter()
        .map(|a| ObjectAnim {
            object: a.object.unwrap_or(0),
            period: a.period,
            keys: a.keys.iter().map(|k| (k.pos, k.quat)).collect(),
            time: 0,
        })
        .collect();
    let mut ours = WorldAnims::new(anims);
    let mut compared = 0;
    for k in 0..ours.anims.len() {
        for time in [0u32, 1, 199, 777, 1500, 2999, 12_345, 29_000] {
            let (header, runs) = {
                let ram = Ram(&mut m.bus.ram);
                (ram.i32(WORLD) as u32, ram.i32(RUNS) as u32)
            };
            let run = runs + 116 * k as u32;
            let object = {
                let mut ram = Ram(&mut m.bus.ram);
                ram.set_i32(run + 0x10, time as i32);
                ram.set_i32(run + 0x20, -1);
                let anims_at = ram.i32(header + 0x44) as u32;
                ram.i32(anims_at + 24 * k as u32) as u32
            };
            m.call(0x8007_f2a4, &[k as u32]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            let rot: [[i16; 3]; 3] =
                std::array::from_fn(|r| std::array::from_fn(|c| ram.i32(object + 2 * (3 * r + c) as u32) as i16));
            let pos: [i32; 3] = std::array::from_fn(|i| ram.i32(object + 0x14 + 4 * i as u32));
            ours.anims[k].time = time;
            let (our_rot, our_pos) = ours.pose(&t, k).expect("a pose");
            assert_eq!((our_rot, our_pos), (rot, pos), "animation {k} at {time} ms");
            assert_ne!(pos, [0; 3], "animation {k} has a place");
            compared += 1;
        }
    }
    assert!(compared > 50, "{compared} compared");
}
