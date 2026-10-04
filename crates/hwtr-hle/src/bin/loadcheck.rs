//! loadcheck: holds the port's car building to the original's. Boots the
//! game, plays it into a race with the scripted input, and when `cars_load`
//! (0x8003beec) returns, builds each player's car the port's way (the race
//! setup from RAM, the handling from the disc, the grid from the SCP) and
//! compares it, field by field, with the car the original built.
//!
//! ```text
//! loadcheck [FRAMES] [--press F:BUTTONS[:LEN],...]
//! ```

#![forbid(unsafe_code)]

use hwtr_hle::original::InMemory;
use std::cell::RefCell;
use std::rc::Rc;

use hwtr_hle::original::car::{CAR_SIZE, CARS};
use hwtr_game::car::{Car, handling};
use hwtr_hle::original::Ram;
use hwtr_hle::Hle;
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
    hle.m.check_through_interrupts(CARS_LOAD, move |cpu, _| {
        let setup_at = cpu.r[4];
        let report = r.clone();
        let big = big.clone();
        Box::new(move |_, bus| {
            let big = hwtr_data::Big::parse(&big).expect("big");
            let mut copy = bus.ram.clone();
            let ram = Ram(&mut copy);
            let setup = hwtr_hle::original::race::setup(&ram, setup_at);
            let tuning = hwtr_hle::original::car::tuning(&ram);
            let scp = ram.i32(SCP) as u32;
            let track = setup.track.to_uppercase();
            for (slot, entrant) in setup.cars.iter().enumerate() {
                if !entrant.driver.is_player() {
                    continue;
                }
                let name = entrant.name.to_uppercase();
                let bmf = big.lookup(&format!("{track}{}BIG/{name}BMF", setup.track_number)).expect("car BMF");
                let cwh = hwtr_data::car::CarBmf::parse(bmf).expect("BMF").cwh;
                let parts = handling::parse_cwh(cwh).expect("CWH");
                let g = scp + 24 + 16 * entrant.grid as u32;
                let q = scp + 120 + 16 * entrant.grid as u32;
                let grid = (ram.vec3(g), [ram.i32(q), ram.i32(q + 4), ram.i32(q + 8), ram.i32(q + 12)]);
                let mut port = Car::load(slot as u8, entrant, &setup, (&parts.0, &parts.1), grid, &tuning);
                let original = Car::read(&ram, CARS + slot as u32 * CAR_SIZE);
                // Not the port's to match yet: flag 0x20, which the zone code
                // sets as the collision object is made (not yet ported), and
                // the centre's padding word, which the original fills from
                // uninitialised stack.
                port.flags |= original.flags & 0x20;
                port.body.centre_pad = original.body.centre_pad;
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
