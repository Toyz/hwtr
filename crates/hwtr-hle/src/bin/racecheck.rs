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

use std::rc::Rc;

use hwtr_game::car::Car;
use hwtr_game::car::layout::{CAR_COUNT, CAR_SIZE, CARS};
use hwtr_game::collision::Collision;
use hwtr_game::collision::world::layout::STEP;
use hwtr_game::math::Tables;
use hwtr_game::ram::Ram;
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
    let cue = hwtr_disc::Disc::find_cue(std::path::Path::new("work/disc")).expect("cue");
    let disc = Rc::new(hwtr_disc::Disc::open(&cue).expect("disc"));
    let mut hle = Hle::new(disc).expect("hle");
    hle.load(&std::fs::read(&state).expect("state file")).expect("state");
    hle.m.step_limit = 30_000_000;
    let tables = Tables::from_ram(&hle.m.bus.ram);
    let tuning = hwtr_game::car::Tuning::read(&Ram(&mut hle.m.bus.ram));
    let dt = (hwtr_game::race::STEP_MS << 12) / 1000;
    let (mut steps, mut same, mut reported) = (0u32, 0u32, 0u32);
    for f in 0..frames {
        let (mut world, _) = Collision::read(&Ram(&mut hle.m.bus.ram));
        let mut native = cars(&mut hle.m.bus.ram);
        let before = Ram(&mut hle.m.bus.ram).i32(STEP);
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
        (n.unknown_25, n.unknown_26, n.unknown_27) = (o.unknown_25, o.unknown_26, o.unknown_27);
        n.update(&tables, &tuning, dt);
        world.update(&tables, &mut native);
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
            let (original_world, _) = Collision::read(&Ram(&mut hle.m.bus.ram));
            tracing::warn!(
                "frame {f} (step {steps}): {} fields differ; contacts: original {contacts} {:?}, port {} {:?}; car pairs {pairs}",
                d.len(),
                original_world.contacts.iter().map(|c| (c.object, c.surface)).collect::<Vec<_>>(),
                world.contacts.len(),
                world.contacts.iter().map(|c| (c.object, c.surface)).collect::<Vec<_>>(),
            );
            for line in d.iter().take(40) {
                tracing::warn!("  {line}");
            }
        }
        reported += 1;
    }
    tracing::info!("{steps} steps: {same} the same, {reported} different");
}
