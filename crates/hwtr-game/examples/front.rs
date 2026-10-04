//! Runs the front end from boot with a scripted pad and prints each state
//! it enters and the texts it shows.
//!
//! ```text
//! cargo run -p hwtr-game --example front -- FRAMES [FRAME:BUTTONS,...]
//! ```
//!
//! Reads `work/fs/CCCPSX.EXE` and the screens' files from `work/big`.
//! Buttons are libpad bits by name: up down left right cross triangle start.
fn main() {
    let mut args = std::env::args().skip(1);
    let frames: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(600);
    let script: Vec<(u32, u16)> = args
        .next()
        .unwrap_or_default()
        .split(',')
        .filter_map(|e| {
            let (f, b) = e.split_once(':')?;
            let bit = match b {
                "up" => 0x10,
                "right" => 0x20,
                "down" => 0x40,
                "left" => 0x80,
                "start" => 0x08,
                "triangle" => 0x1000,
                "circle" => 0x2000,
                "cross" | "x" => 0x4000,
                "square" => 0x8000,
                _ => return None,
            };
            Some((f.parse().ok()?, bit))
        })
        .collect();
    let exe = std::fs::read("work/fs/CCCPSX.EXE").expect("exe");
    let byte = |a: u32| a.checked_sub(0x8001_0000).and_then(|o| exe.get(o as usize + 0x800)).copied().unwrap_or(0);
    let strings = std::fs::read("work/big/SCREENSBIG/ENGLISHHWT").expect("strings");
    let cars = std::fs::read("work/big/SCREENSBIG/ENGCARSCDT").expect("cars");
    let font = hwtr_game::hud::Font::parse(&std::fs::read("work/big/SCREENSBIG/SCRNFNTOVL").expect("font")).expect("font");
    let tuning = std::fs::read("work/big/SCREENSBIG/TUNINGPRM").expect("tuning");
    let cwhs = std::fs::read("work/big/SCREENSBIG/CWHSBMF").ok();
    let name_keys = std::fs::read("work/big/SCREENSBIG/ENGNAMECHM").ok();
    let password_keys = std::fs::read("work/big/SCREENSBIG/ENGPWDCHM").ok();
    let files = hwtr_game::front::Files {
        strings: &strings,
        tuning: &tuning,
        cars: &cars,
        font: &font,
        card: None,
        card2: None,
        card_slots: [true, false],
        cwhs: cwhs.as_deref(),
        name_keys: name_keys.as_deref(),
        password_keys: password_keys.as_deref(),
    };
    let mut front = hwtr_game::front::Front::new(&byte, files, 1).expect("front end");
    println!("front end: {} actions not ported yet", front.unported_actions().len());
    if frames == 0 {
        for a in front.unported_actions() {
            println!("unported 0x{a:08x}");
        }
    }
    let mut last = usize::MAX;
    let mut last_text = String::new();
    for f in 0..frames {
        // A press lasts 4 frames.
        let buttons = script.iter().filter(|&&(at, _)| f >= at && f < at + 4).fold(0, |b, &(_, bit)| b | bit);
        let pad = hwtr_game::pad::PadState { buttons, ..Default::default() };
        front.tick([pad, Default::default()]);
        front.clock = front.clock.wrapping_add(hwtr_game::front::FRAME_MS);
        if front.state() != last {
            last = front.state();
            println!("frame {f}: state {last}");
        }
        for a in std::mem::take(&mut front.unported) {
            println!("frame {f}: unported 0x{a:08x}");
        }
        let text: String = front.shown.glyphs.iter().map(|g| g.ch as char).collect();
        if text != last_text {
            println!("frame {f}: {:?} {text:?}", front.shown.background);
            last_text = text;
        }
        if let Some(r) = front.race.take() {
            println!("frame {f}: race {r:?}");
            // The player first, the rest in order, every lap run.
            let cars = (0..r.cars.len())
                .map(|k| hwtr_game::front::CarResult {
                    time: 60_000 + 1000 * k as u32,
                    best: 15_000 + 100 * k as u32,
                    laps: r.laps,
                    points: [10, 8, 7, 6, 5, 4][k.min(5)],
                    score: 0,
                })
                .collect();
            front.race_over(hwtr_game::front::RaceResult { end: hwtr_game::front::RaceEnd::Finished, cars, unlocks: [0; 6] });
        }
    }
}
