//! racecheck: holds the port's race step to the original's, one step at a
//! time.
//!
//! Runs the original from a save state. A frame ends at the vertical blank
//! wherever the game is, so a race step can straddle two frames: only a
//! frame that ran one whole step is checked. The native race is taken from
//! the original's memory as its cars' update (0x8004064c) begins; the
//! native player's car takes the controls the original worked out that
//! step, the native race runs its step (the player's car update, its way
//! watched, and the collision update), and the player's car is compared
//! with the original's as its collision update (0x8004de6c) returns. Each
//! step starts again from the original, so every difference reported is
//! one step's: what the port's step does not yet do. A step with a reset
//! is counted, not checked. On a difference it also reports the car as
//! the collision began and each collision stage run from the original's
//! world against the original's result.
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
use hwtr_game::collision::Collision;
use hwtr_game::collision::world::Step;
use hwtr_game::rand::Rand;
use hwtr_hle::Hle;
use hwtr_hle::original::Ram;
use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE, CARS};
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::world::STEP;
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
    // The other cars as the original's collision step finds them (its
    // computer cars have moved by then).
    let at_collision: Rc<RefCell<Vec<Car>>> = Rc::default();
    // A frame ends at the vertical blank wherever the game is, so a step
    // may straddle two frames: only a frame that ran a whole step (the
    // cars' update, 0x8004064c, through the collision's return) is
    // checked.
    // The step's start is the memory as the cars' update begins, its end
    // as the collision returns: the rest of the step (the places, the
    // camera, the clock) is not the port's to run here.
    let whole: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let start: Rc<RefCell<Vec<u8>>> = Rc::default();
    let end: Rc<RefCell<Vec<u8>>> = Rc::default();
    let snap = at_collision.clone();
    let (log, keep) = (whole.clone(), end.clone());
    hle.m.check_through_interrupts(0x8004_de6c, move |_, bus| {
        let mut copy = bus.ram.clone();
        *snap.borrow_mut() = cars(&mut copy);
        let (log, keep) = (log.clone(), keep.clone());
        Box::new(move |_, bus| {
            log.borrow_mut().push("collided");
            *keep.borrow_mut() = bus.ram.clone();
            Ok(())
        })
    });
    let (log, keep) = (whole.clone(), start.clone());
    hle.m.check_through_interrupts(0x8004_064c, move |_, bus| {
        log.borrow_mut().push("cars");
        *keep.borrow_mut() = bus.ram.clone();
        Box::new(|_, _| Ok(()))
    });
    // The world as each of the collision's stages finds it: the zones
    // (0x800515e0), the wheels (0x80051bc0), the ground (0x800536b4) and
    // the walls (0x80054964).
    let at_stage: Rc<RefCell<Vec<(u32, Collision, Vec<Car>)>>> = Rc::default();
    for addr in [0x8005_15e0u32, 0x8005_1bc0, 0x8005_36b4, 0x8005_4964] {
        let log = at_stage.clone();
        hle.m.check_through_interrupts(addr, move |_, bus| {
            let mut copy = bus.ram.clone();
            let (world, _) = hwtr_hle::original::world::collision(&Ram(&mut copy));
            log.borrow_mut().push((addr, world, cars(&mut copy)));
            Box::new(|_, _| Ok(()))
        });
    }
    // How often the collision step and its stages ran this frame.
    let calls: Rc<RefCell<Vec<u32>>> = Rc::default();
    for addr in [0x8005_148cu32] {
        let log = calls.clone();
        hle.m.check_through_interrupts(addr, move |_, _| {
            log.borrow_mut().push(addr);
            Box::new(|_, _| Ok(()))
        });
    }
    let (mut steps, mut same, mut reported, mut resets) = (0u32, 0u32, 0u32, 0u32);
    for f in 0..frames {
        let before = Ram(&mut hle.m.bus.ram).i32(STEP);
        entries.borrow_mut().clear();
        exits.borrow_mut().clear();
        at_stage.borrow_mut().clear();
        whole.borrow_mut().clear();
        calls.borrow_mut().clear();
        script.apply(&mut hle, f);
        hle.frame().expect("frame");
        let ran = Ram(&mut hle.m.bus.ram).i32(STEP).wrapping_sub(before);
        if ran != 1 || *whole.borrow() != ["cars", "collided"] {
            continue;
        }
        steps += 1;
        let mut start_ram = start.borrow().clone();
        let mut end_ram = end.borrow().clone();
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut start_ram));
        let mut native = cars(&mut start_ram);
        let (seed, time, clock) = {
            let ram = Ram(&mut start_ram);
            (ram.i32(SEED) as u32, ram.i32(0x800d_0e34) as u32, ram.i32(0x800d_240c) as u32)
        };
        let original = cars(&mut end_ram);
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
            tracing::info!("frame {f}: a reset (not checked here)");
            resets += 1;
            continue;
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
        // cars_update goes on, after the places, to watch the player's way
        // (0x8005c9b4).
        hwtr_game::laps::watch_way(&world.scp, &mut native[0], zone.0, 25);
        for (n, o) in native.iter_mut().zip(at_collision.borrow().iter()).skip(1) {
            *n = o.clone();
        }
        world.contacts.clear();
        world.pairs.clear();
        world.step = world.step.wrapping_add(1);
        world.update_points(&mut native);
        world.find_pairs(&tables, &mut native);
        // The player's car as the collision step finds it, and the world
        // before its stages, for the report.
        let mut before_collision = native[0].clone();
        before_collision.sounds = hwtr_game::effects::Pending(None);
        before_collision.lines = hwtr_game::effects::Pending(None);
        let before_stages = (world.clone(), native.clone());
        let mut step = Step { tuning: &tuning, rand: &mut rand, time, clock };
        world.stages(&tables, &mut native, &mut step);
        let at_impulse = native[0].body.clone();
        world.contact_impulses(&tables, &mut native, &mut step);
        world.pair_impulses(&tables, step.tuning, &mut native, step.rand);
        // The wheels' roll (+0x80) turns at the frame's end, in the pose
        // pass (0x80049ecc), not in the step: the original's is taken.
        for (n, o) in native[0].wheels.iter_mut().zip(&original[0].wheels) {
            n.angle = o.angle;
        }
        // The port's events for the race to take: not in the original's
        // memory.
        {
            use hwtr_game::effects::Pending;
            let n = &mut native[0];
            (n.crashed, n.turbo_fired, n.flame_out) = (Pending(None), Pending(None), Pending(None));
            (n.wreck_draws, n.sounds, n.lines) = (Pending(None), Pending(None), Pending(None));
        }
        let d = diff(&original[0], &native[0]);
        if d.is_empty() {
            same += 1;
            continue;
        }
        if reported < 3 || all {
            let (contacts, pairs) = {
                let ram = Ram(&mut end_ram);
                (ram.i16(0x800d_2674) as u16, ram.i16(0x800d_2676) as u16)
            };
            let (original_world, _) = hwtr_hle::original::world::collision(&Ram(&mut end_ram));
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
                            d(exit),
                            d(&b),
                            d(&nf),
                            c.normal,
                            entry.vel,
                            entry.spin
                        );
                    }
                }
            }
            for line in diff(&at_collision.borrow()[0], &before_collision).iter().take(60) {
                tracing::warn!("  at the collision step: {line}");
            }
            // Each stage from the original's world as the stage before
            // left it, against the original's after it.
            let stages = at_stage.borrow();
            tracing::warn!(
                "  calls this frame: {:x?}, stages {:x?}",
                calls.borrow(),
                stages.iter().map(|s| s.0).collect::<Vec<_>>()
            );
            let player = before_stages.0.objects.iter().position(|o| o.car == Some(0));
            for pair in stages.windows(2) {
                let ((addr, w0, c0), (_, w1, c1)) = (&pair[0], &pair[1]);
                let (mut w, mut c) = (w0.clone(), c0.clone());
                match addr {
                    0x8005_15e0 => w.track_zones(&c),
                    0x8005_1bc0 => w.wheels(&tables, &mut c),
                    0x8005_36b4 => w.ground(&tables, &mut c),
                    _ => continue,
                }
                if let Some(k) = player {
                    for line in diff(&w1.objects[k], &w.objects[k]).iter().take(10) {
                        tracing::warn!("  stage {addr:#x}, object {k}: {line}");
                    }
                }
                for line in diff(&c1[0], &c[0]).iter().take(10) {
                    tracing::warn!("  stage {addr:#x}, car: {line}");
                }
            }
            if let Some((_, w0, c0)) = stages.first() {
                for (k, (a, b)) in w0.objects.iter().zip(&before_stages.0.objects).enumerate() {
                    for line in diff(a, b).iter().take(4) {
                        tracing::warn!("  before the stages, object {k}: {line}");
                    }
                }
                for line in diff(&w0.members, &before_stages.0.members).iter().take(4) {
                    tracing::warn!("  before the stages, members: {line}");
                }
                for line in diff(&c0[0], &before_stages.1[0]).iter().take(10) {
                    tracing::warn!("  before the stages, car: {line}");
                }
            }
            // The player's object: the zones its points are in.
            if let Some(k) = world.objects.iter().position(|o| o.car == Some(0)) {
                let (a, b) = (&original_world.objects[k], &world.objects[k]);
                if a.point_zones != b.point_zones || a.zones != b.zones {
                    tracing::warn!("  object {k} zones: original {:?} {:?}", a.point_zones, a.zones);
                    tracing::warn!("  object {k} zones: port     {:?} {:?}", b.point_zones, b.zones);
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
    tracing::info!("{steps} steps: {same} the same, {reported} different, {resets} resets not checked");
}
