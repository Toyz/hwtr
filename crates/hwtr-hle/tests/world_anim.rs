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
            triggered: false,
            left: 0,
            trigger: 0,
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

/// A trigger firing (0x8006a200) and the animations it starts running on
/// (0x8007f670, 0x8007f17c), against the original: a made-up trigger of
/// any flags naming two of the desert's animations (made triggered), the
/// animations' time left and owner, the sounds asked for, then every
/// animation's time over a few steps.
#[test]
fn triggers_start_animations_as_the_original() {
    use hwtr_game::collision::scp::Trigger;
    use hwtr_game::world_anim::Fired;
    use std::cell::RefCell;
    use std::rc::Rc;
    const TRIGGERS: u32 = 0x800d_0fd4;
    const SOUNDS: u32 = 0x800d_0fd8;
    const CLOCK: u32 = 0x800d_1000;
    const SCRATCH: u32 = 0x801f_8000;
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/DESERT1BIG/DESERT1WLD");
    let Ok(file) = std::fs::read(path) else { return };
    let world = hwtr_data::world::World::parse(&file).expect("the world parses");
    let base: Vec<ObjectAnim> = world
        .anims
        .iter()
        .map(|a| ObjectAnim {
            object: a.object.unwrap_or(0),
            period: a.period,
            keys: a.keys.iter().map(|k| (k.pos, k.quat)).collect(),
            time: 0,
            triggered: false,
            left: 0,
            trigger: 0,
        })
        .collect();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    m.hook(0x8003_6270, move |cpu, bus| {
        let sound = bus.read(cpu.r[29] + 16, 4).unwrap() as u8;
        log.borrow_mut().push(Fired::Sound { anim: usize::MAX, sound });
        0
    });
    let log = heard.clone();
    m.hook(0x8001_57f8, move |cpu, _| {
        if cpu.r[4] == 27 {
            log.borrow_mut().push(Fired::Effect27);
        }
        0
    });
    let now = Rc::new(RefCell::new(0u32));
    let clock = now.clone();
    m.hook(0x8006_122c, move |_, _| *clock.borrow());
    m.hook(0x8006_1238, |_, _| 0);
    let start = m.bus.ram.clone();
    let mut rng = common::Rng(0x7a1_6e2);
    let count = base.len();
    let mut started = 0;
    for round in 0..400 {
        m.bus.ram.copy_from_slice(&start);
        let (runs, _) = {
            let ram = Ram(&mut m.bus.ram);
            (ram.i32(0x800d_0ffc) as u32, 0)
        };
        let mut ours = WorldAnims::new(base.clone());
        let t = Trigger {
            flags: [2, 6, 0x12, 0x16, 0x2a, 0x3, 0x4, 0x22, 0x0e][rng.below(9) as usize],
            anims: [rng.below(count as u32) as u16, rng.below(count as u32) as u16],
            frame: rng.below(12) as u16,
            sounds: [rng.below(60) as u8, rng.below(60) as u8],
            params: [rng.below(8) as u8, rng.below(8) as u8],
        };
        let trigger = rng.below(3) as u16;
        let player = rng.below(2) == 0;
        let begun = rng.below(3) as u32 * 50;
        {
            let mut ram = Ram(&mut m.bus.ram);
            for k in 0..count as u32 {
                let run = runs + 116 * k;
                ram.set_i32(run + 0x10, 0);
                ram.set_u8(run + 0x14, 0);
                ram.set_i32(run + 0x18, 0);
                ram.set_i32(run + 0x1c, 0);
            }
            for &a in &t.anims {
                ram.set_u8(runs + 116 * a as u32 + 0x14, 1);
                ours.set_triggered(a as usize);
            }
            // One of them perhaps still running.
            if begun != 0 {
                ram.set_i32(runs + 116 * t.anims[0] as u32 + 0x18, begun as i32);
                ours.anims[t.anims[0] as usize].left = begun;
            }
            let at = SCRATCH + 12 * trigger as u32;
            ram.set_i16(at, t.flags as i16);
            ram.set_i16(at + 2, t.anims[0] as i16);
            ram.set_i16(at + 4, t.anims[1] as i16);
            ram.set_i16(at + 6, t.frame as i16);
            for k in 0..2u32 {
                ram.set_u8(at + 8 + k, t.sounds[k as usize]);
                ram.set_u8(at + 10 + k, t.params[k as usize]);
            }
            ram.set_i32(TRIGGERS, SCRATCH as i32);
            ram.set_i32(SOUNDS, (SCRATCH + 0x100) as i32);
            for k in 0..8 {
                ram.set_u8(SCRATCH + 0x100 + k, 0xff);
            }
            ram.set_i32(CLOCK, 0);
            ram.set_i32(SCRATCH + 0x200 + 4, player as i32);
        }
        heard.borrow_mut().clear();
        *now.borrow_mut() = 0;
        m.call(0x8006_a200, &[trigger as u32, SCRATCH + 0x200]).unwrap();
        let fired = ours.fire(&t, trigger, player);
        let what = format!("round {round}: {t:?} trigger {trigger} player {player}");
        let theirs: Vec<Fired> = heard.borrow().clone();
        let ours_heard: Vec<Fired> = fired
            .iter()
            .map(|f| match *f {
                Fired::Sound { sound, .. } => Fired::Sound { anim: usize::MAX, sound },
                f => f,
            })
            .collect();
        assert_eq!(theirs, ours_heard, "{what}: sounds");
        for step in 0..6u32 {
            {
                let ram = Ram(&mut m.bus.ram);
                for (k, a) in ours.anims.iter().enumerate() {
                    let run = runs + 116 * k as u32;
                    assert_eq!(
                        (ram.i32(run + 0x10) as u32, ram.i32(run + 0x18) as u32),
                        (a.time, a.left),
                        "{what}: animation {k} after {step} steps"
                    );
                    if a.left != 0 {
                        assert_eq!(ram.i32(run + 0x1c) as u16, a.trigger, "{what}: animation {k}'s trigger");
                    }
                }
            }
            let next = 17 + 40 * step;
            *now.borrow_mut() += next;
            m.call(0x8007_f17c, &[]).unwrap();
            let _ = ours.step(*now.borrow());
        }
        started += fired.len();
    }
    assert!(started > 100, "{started} started");
}
