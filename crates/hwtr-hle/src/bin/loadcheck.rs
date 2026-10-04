//! loadcheck: holds the port's car building to the original's. Boots the
//! game, plays it into a race with the scripted input, and when `cars_load`
//! (0x8003beec) returns, builds each car the port's way (the race setup
//! from RAM, the handling from the disc, the grid from the SCP; a computer
//! car moved to its route's start and given its driver, from the AI as
//! `cars_load` found it) and compares it, field by field, with the car the
//! original built, and each computer car's driver with the original's.
//!
//! ```text
//! loadcheck [FRAMES] [--press F:BUTTONS[:LEN],...]
//! LOADCHECK_DUMP=DIR loadcheck   # also writes differing drivers in full
//! ```

#![forbid(unsafe_code)]

use hwtr_hle::original::InMemory;
use std::cell::RefCell;
use std::rc::Rc;

use hwtr_game::car::{Car, handling};
use hwtr_hle::Hle;
use hwtr_hle::original::Ram;
use hwtr_hle::original::car::{CAR_SIZE, CARS};
use hwtr_hle::script::Script;

const CARS_LOAD: u32 = 0x8003_beec;
const SCP: u32 = 0x800d_2658;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let mut frames = 2400u64;
    let mut script = Script::default();
    script.press("1300:down,1340:x,1600:start,1800:x");
    script.analog = true;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--press" => {
                script = Script::default();
                script.press(&args.next().unwrap_or_default());
            }
            n => frames = n.parse().expect("frames"),
        }
    }
    let cue = rrt::disc::Image::find(std::path::Path::new("work/disc")).expect("cue");
    let disc = Rc::new(rrt::disc::Image::open(&cue).expect("disc"));
    let iso = disc.iso().expect("iso");
    let big = iso.find("CCCPSX.BIG").and_then(|e| iso.read(&e)).expect("CCCPSX.BIG");
    let mut hle = Hle::new(disc.clone()).expect("hle");
    hle.m.step_limit = 30_000_000;
    let report = Rc::new(RefCell::new(Vec::<String>::new()));
    let r = report.clone();
    hle.m.check_through_interrupts(CARS_LOAD, move |cpu, bus| {
        let setup_at = cpu.r[4];
        let addrs: Vec<u32> = (0..8).map(|k| CARS + k * CAR_SIZE).collect();
        let mut ai = hwtr_hle::original::ai::ai(&Ram(&mut bus.ram.clone()), &addrs);
        let report = r.clone();
        let big = big.clone();
        Box::new(move |_, bus| {
            let big = hwtr_data::Big::parse(&big).expect("big");
            let mut copy = bus.ram.clone();
            let ram = Ram(&mut copy);
            let setup = hwtr_hle::original::race::setup(&ram, setup_at);
            let tuning = hwtr_hle::original::car::tuning(&ram);
            let tables = hwtr_hle::original::tables(ram.0);
            let scp = ram.i32(SCP) as u32;
            let track = format!("{}{}", setup.track.to_uppercase(), setup.track_number);
            let line_name = setup.best_line.as_ref().map_or(track.clone(), |n| n.to_uppercase());
            let bld = big.lookup(&format!("{track}BIG/{line_name}BLD")).expect("best line");
            let line = hwtr_game::line::BestLine::parse(bld).expect("BLD");
            let original_ai = hwtr_hle::original::ai::ai(&ram, &addrs);
            for (slot, entrant) in setup.cars.iter().enumerate() {
                let name = entrant.name.to_uppercase();
                let bmf = big.lookup(&format!("{track}BIG/{name}BMF")).expect("car BMF");
                let bmf = hwtr_data::car::CarBmf::parse(bmf).expect("BMF");
                let cwh = bmf.cwh;
                let faces = hwtr_data::car::Model::parse(bmf.models[0]).expect("model").root.faces.len();
                let parts = handling::parse_cwh(cwh).expect("CWH");
                let g = scp + 24 + 16 * entrant.grid as u32;
                let q = scp + 120 + 16 * entrant.grid as u32;
                let grid = (ram.vec3(g), [ram.i32(q), ram.i32(q + 4), ram.i32(q + 8), ram.i32(q + 12)]);
                // As the port's race builds it: loaded, then given its
                // collision object (whose zone sets its flags and lap
                // distance).
                let mut port = Car::load(slot as u8, entrant, &setup, (&parts.0, &parts.1), grid, &tuning);
                let computer = !entrant.driver.is_player();
                if computer {
                    hwtr_game::race::computer_start(&mut port, &line, entrant.grid as usize);
                    let grid = entrant.grid as usize;
                    ai.add(&mut port, &line, grid, setup.difficulty, setup.laps, &tuning, 0);
                }
                // The app counts the model's root faces, which a wreck
                // throws off.
                port.model_faces = faces.min(0xffff) as u16;
                let scp = hwtr_game::collision::Scp::parse(&hwtr_hle::original::world::scp_bytes(&ram).1).expect("SCP");
                let course = hwtr_game::laps::Course::new(&setup, &scp, line.lap_length);
                let mut collision = hwtr_game::collision::Collision::new(scp);
                collision.course = course;
                collision.add_car(&tables, &mut port);
                let original = Car::read(&ram, CARS + slot as u32 * CAR_SIZE);
                let line = if port == original {
                    format!("car {slot} ({}): the same", entrant.name)
                } else {
                    let (a, b) = (format!("{original:#?}"), format!("{port:#?}"));
                    let diff: Vec<String> = a
                        .lines()
                        .zip(b.lines())
                        .filter(|(x, y)| x != y)
                        .take(20)
                        .map(|(x, y)| format!("    original {}\n    port     {}", x.trim(), y.trim()))
                        .collect();
                    format!("car {slot} ({}): differs\n{}", entrant.name, diff.join("\n"))
                };
                report.borrow_mut().push(line);
                if computer {
                    let (a, b) = (&original_ai.drivers[slot], &ai.drivers[slot]);
                    let line = if a == b {
                        format!("driver {slot}: the same")
                    } else {
                        let (a, b) = (format!("{a:#?}"), format!("{b:#?}"));
                        if let Ok(dir) = std::env::var("LOADCHECK_DUMP") {
                            let _ = std::fs::write(format!("{dir}/driver{slot}-original.txt"), &a);
                            let _ = std::fs::write(format!("{dir}/driver{slot}-port.txt"), &b);
                        }
                        let diff: Vec<String> = a
                            .lines()
                            .zip(b.lines())
                            .filter(|(x, y)| x != y)
                            .take(20)
                            .map(|(x, y)| format!("    original {}\n    port     {}", x.trim(), y.trim()))
                            .collect();
                        format!("driver {slot}: differs\n{}", diff.join("\n"))
                    };
                    report.borrow_mut().push(line);
                }
            }
            Ok(())
        })
    });
    for f in 0..frames {
        script.apply(&mut hle, f);
        if let Err(e) = hle.frame() {
            tracing::error!("frame {f}: {e:x?}");
            break;
        }
        if !report.borrow().is_empty() {
            break;
        }
    }
    let report = report.borrow();
    if report.is_empty() {
        tracing::error!("cars_load never ran");
        std::process::exit(1);
    }
    for line in report.iter() {
        println!("{line}");
    }
}
