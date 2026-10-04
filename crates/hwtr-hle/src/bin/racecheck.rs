//! racecheck: holds the port's race step to the original's, one step at a
//! time.
//!
//! Runs the original from a save state. Before each frame the native race
//! is taken from the original's RAM (the cars and the collision world);
//! when the frame runs one race step, the native player's car takes the
//! controls the original worked out that step, the native race runs its
//! step (the player's car update and the collision update), and the
//! player's car is compared with the original's. Each step starts again
//! from the original, so every difference reported is one step's: what the
//! port's step does not yet do.
//!
//! ```text
//! racecheck STATE FRAMES [--press F:BUTTONS[:LEN],...] [--stick F:LX,LY[:LEN];...] [--analog] [--all]
//! ```
//!
//! `--all` reports every differing step, not just the first few.

#![forbid(unsafe_code)]

use hwtr_hle::original::InMemory;
use std::cell::RefCell;
use std::rc::Rc;

use hwtr_game::body::Body;

use hwtr_game::car::Car;
use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE, CARS};
use hwtr_game::collision::world::Step;
use hwtr_hle::original::world::STEP;
use hwtr_game::rand::Rand;
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::Ram;
use hwtr_hle::Hle;
use hwtr_hle::script::Script;

fn cars(ram: &mut [u8]) -> Vec<Car> {
    let ram = Ram(ram);
    (0..ram.i32(CAR_COUNT) as u32).map(|k| Car::read(&ram, CARS + k * CAR_SIZE)).collect()
}

/// The lines of two values' pretty debug forms that differ, with the path
/// of struct fields above each.
fn diff<T: std::fmt::Debug>(a: &T, b: &T) -> Vec<String> {
    let (a, b) = (format!("{a:#?}"), format!("{b:#?}"));
    let mut path: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (la, lb) in a.lines().zip(b.lines()) {
        let depth = la.len() - la.trim_start().len();
        path.truncate(depth / 4);
        if la != lb {
            out.push(format!("{}: original {} port {}", path.join("."), la.trim(), lb.trim()));
        }
        if let Some(name) = la.trim().strip_suffix(" {").or(la.trim().strip_suffix(": [")) {
            path.push(name.trim_end_matches(':').to_string());
        } else if la.trim().ends_with('{') || la.trim().ends_with('[') || la.trim().ends_with('(') {
            path.push(la.trim().trim_end_matches(['{', '[', '(', ' ', ':']).to_string());
        }
    }
    out
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let state = args.next().expect("racecheck STATE FRAMES [input]");
    let frames: u64 = args.next().and_then(|s| s.parse().ok()).expect("a frame count");
    let mut script = Script::default();
    let mut all = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--analog" => script.analog = true,
            "--press" => script.press(&args.next().unwrap_or_default()),
            "--stick" => script.stick(&args.next().unwrap_or_default()),
            "--all" => all = true,
            other => panic!("unknown argument {other}"),
        }
    }
    let cue = rrt::disc::Image::find(std::path::Path::new("work/disc")).expect("cue");
    let disc = Rc::new(rrt::disc::Image::open(&cue).expect("disc"));
    let mut hle = Hle::new(disc).expect("hle");
    hle.load(&std::fs::read(&state).expect("state file")).expect("state");
    hle.m.step_limit = 30_000_000;
    let tables = hwtr_hle::original::tables(&hle.m.bus.ram);
    tracing::info!("friction table {:?}", &tables.surface_friction[..12]);
    let tuning = hwtr_hle::original::car::tuning(&Ram(&mut hle.m.bus.ram));
    let dt = (hwtr_game::race::STEP_MS << 12) / 1000;
    // The player's body as each impulse the original applies finds it.
    let entries: Rc<RefCell<Vec<Body>>> = Rc::default();
    let exits: Rc<RefCell<Vec<Body>>> = Rc::default();
    let log = entries.clone();
    let exit_log = exits.clone();
    hle.m.check(0x8006_dc08, move |cpu, bus| {
        let ram = &bus.ram;
        let at = |a: u32| (a & 0x1f_ffff) as usize;
        let word = |a: u32| u32::from_le_bytes(ram[at(a)..at(a) + 4].try_into().unwrap());
        let body = word(word(cpu.r[4]) + 0x64);
        if body == CARS + 0x30 {
            let mut copy = ram.clone();
            log.borrow_mut().push(Body::read(&Ram(&mut copy), body));
            tracing::warn!("  original impulse: friction {:#x}, surface {}", cpu.r[5], ram[at(cpu.r[4] + 0x24)]);
        }
        let exit_log = exit_log.clone();
        Box::new(move |_, bus| {
            if body == CARS + 0x30 {
                let mut copy = bus.ram.clone();
                exit_log.borrow_mut().push(Body::read(&Ram(&mut copy), body));
            }
            Ok(())
        })
    });
    let (mut steps, mut same, mut reported) = (0u32, 0u32, 0u32);
    for f in 0..frames {
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut hle.m.bus.ram));
        let mut native = cars(&mut hle.m.bus.ram);
        let (before, seed, time, clock) = {
            let ram = Ram(&mut hle.m.bus.ram);
            (ram.i32(STEP), ram.i32(SEED) as u32, ram.i32(0x800d_0e34) as u32, ram.i32(0x800d_240c) as u32)
        };
        entries.borrow_mut().clear();
        exits.borrow_mut().clear();
        script.apply(&mut hle, f);
        hle.frame().expect("frame");
        let ran = Ram(&mut hle.m.bus.ram).i32(STEP).wrapping_sub(before);
        if ran != 1 {
            continue;
        }
        steps += 1;
        let original = cars(&mut hle.m.bus.ram);
        let (o, n) = (&original[0], &mut native[0]);
        (n.steer, n.accel, n.brake, n.stick, n.handbrake) = (o.steer, o.accel, o.brake, o.stick, o.handbrake);
        (n.reset_held, n.turbo_held) = (o.reset_held, o.turbo_held);
        // The places are ranked over every car, computer cars too, after
        // the cars' update (not checked here).
        n.laps.place = o.laps.place;
        // cars_update for the player's car: its timers, then car_update
        // (a reset needs the whole race, and is only reported).
        n.run_timers(25);
        if n.wants_reset() {
            tracing::warn!("frame {f}: a reset (not checked here)");
        }
        let zone = world
            .objects
            .iter()
            .find(|o| o.car == Some(0))
            .map_or((None, false), |o| (o.zones.iter().next(), o.zones.entries.len() == 1));
        // The step's clocks, both as the frame begins (the vertical blank
        // advances the system clock at its end).
        let mut rand = Rand { seed };
        let scoring = Ram(&mut hle.m.bus.ram).u8(0x800d_2640) != 0;
        let mut drive = hwtr_game::car::update::Drive {
            tables: &tables,
            tuning: &tuning,
            rand: &mut rand,
            time,
            dt,
            scoring,
            endless_turbo: false,
        };
        native[0].update(&mut drive, zone);
        world.contacts.clear();
        world.step = world.step.wrapping_add(1);
        world.update_points(&mut native);
        let mut step = Step { tuning: &tuning, rand: &mut rand, time, clock };
        world.stages(&tables, &mut native, &mut step);
        let at_impulse = native[0].body.clone();
        world.contact_impulses(&tables, &mut native, &mut step);
        let d = diff(&original[0], &native[0]);
        if d.is_empty() {
            same += 1;
            continue;
        }
        if reported < 3 || all {
            let (contacts, pairs) = {
                let ram = Ram(&mut hle.m.bus.ram);
                (ram.i16(0x800d_2674) as u16, ram.i16(0x800d_2676) as u16)
            };
            let (original_world, _) = hwtr_hle::original::world::collision(&Ram(&mut hle.m.bus.ram));
            tracing::warn!(
                "frame {f} (step {steps}): {} fields differ; contacts: original {contacts} {:?}, port {} {:?}; car pairs {pairs}",
                d.len(),
                original_world.contacts.iter().map(|c| (c.object, c.surface)).collect::<Vec<_>>(),
                world.contacts.len(),
                world.contacts.iter().map(|c| (c.object, c.surface)).collect::<Vec<_>>(),
            );
            tracing::warn!("  the original's impulses on the player: {}", entries.borrow().len());
            if let Some(entry) = entries.borrow().first() {
                for line in diff(entry, &at_impulse) {
                    tracing::warn!("  at the first impulse: {line}");
                }
                if let [c] = world.contacts.as_slice() {
                    let mut b = entry.clone();
                    let friction = hwtr_game::collision::walls::contact_friction(&tables, c.surface, false);
                    b.impulse(&tables, c.point, c.normal, 0x800, friction);
                    tracing::warn!("  entry + port impulse vs original: {:?}", diff(&original[0].body, &b));
                    tracing::warn!("  entry + port impulse vs native: {:?}", diff(&native[0].body, &b));
                    if let Some(exit) = exits.borrow().first() {
                        tracing::warn!("  entry + port impulse vs original exit: {:?}", diff(exit, &b));
                        tracing::warn!("  original exit vs original final: {:?}", diff(&original[0].body, exit));
                        let d = |x: &Body| hwtr_game::math::sub(x.momentum, entry.momentum);
                        let mut nf = entry.clone();
                        nf.impulse(&tables, c.point, c.normal, 0x800, 0);
                        tracing::warn!(
                            "  momentum change: original {:?}, port {:?}, port without friction {:?}; normal {:?}, vel {:?}, spin {:?}",
                            d(exit), d(&b), d(&nf), c.normal, entry.vel, entry.spin
                        );
                    }
                }
            }
            for (k, (a, b)) in original_world.contacts.iter().zip(&world.contacts).enumerate() {
                if a != b {
                    tracing::warn!("  contact {k}: original {a:?}, port {b:?}");
                }
            }
            for line in d.iter().take(40) {
                tracing::warn!("  {line}");
            }
        }
        reported += 1;
    }
    tracing::info!("{steps} steps: {same} the same, {reported} different");
}
