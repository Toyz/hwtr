//! lockstep: holds the ported functions to the original game.
//!
//! By default (shadow mode) runs the original from a save state and checks
//! each ported function on every call the game makes (see
//! `hwtr_hle::port::shadow`), reporting how many calls passed.
//!
//! With `--replace`, runs the game twice, once as it is and once with the
//! ported functions hooked in, and compares main RAM after every frame. The
//! first frame that differs is reported. A hooked function takes no
//! instructions, so the interrupts (timed in instructions) move and the two
//! runs drift apart wherever timing shows; this mode is for when the port
//! owns the frame.
//!
//! ```text
//! lockstep STATE FRAMES [--replace] [--press F:BUTTONS[:LEN],...] [--stick F:LX,LY[:LEN];...] [--analog]
//! ```

use std::rc::Rc;

use hwtr_game::car::{CAR_SIZE, CARS};
use hwtr_hle::Hle;
use hwtr_hle::script::Script;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let state = args.next().expect("lockstep STATE FRAMES [input]");
    let frames: u64 = args.next().and_then(|s| s.parse().ok()).expect("a frame count");
    let mut script = Script::default();
    let mut replace = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--replace" => replace = true,
            "--analog" => script.analog = true,
            "--press" => script.press(&args.next().unwrap_or_default()),
            "--stick" => script.stick(&args.next().unwrap_or_default()),
            other => panic!("unknown argument {other}"),
        }
    }
    let cue = hwtr_disc::Disc::find_cue(std::path::Path::new("work/disc")).expect("cue");
    let disc = Rc::new(hwtr_disc::Disc::open(&cue).expect("disc"));
    let bytes = std::fs::read(&state).expect("state file");
    let make = || {
        let mut hle = Hle::new(disc.clone()).expect("hle");
        hle.load(&bytes).expect("state");
        hle.m.step_limit = 30_000_000;
        hle
    };
    for (addr, name) in hwtr_hle::port::PORTED {
        tracing::info!("ported: {addr:08x} {name}");
    }
    if !replace {
        let mut hle = make();
        hwtr_hle::port::shadow(&mut hle.m);
        for f in 0..frames {
            script.apply(&mut hle, f);
            hle.frame().expect("frame");
            let s = &hle.m.check_stats;
            if let Some((func, e)) = s.failed.first() {
                tracing::error!("frame {f}: {func:08x} differs after {} calls passed: {e}", s.passed);
                std::process::exit(1);
            }
            if (f + 1) % 60 == 0 {
                tracing::info!("frame {}: {} calls checked, {} skipped", f + 1, s.passed, s.skipped);
            }
        }
        let s = &hle.m.check_stats;
        tracing::info!("{frames} frames: {} calls matched, {} skipped for interrupts", s.passed, s.skipped);
        return;
    }
    let (mut original, mut port) = (make(), make());
    hwtr_hle::port::install(&mut port.m);
    // The stack differs, since a hooked function has no frame.
    let mut skip = hwtr_hle::port::unmatched();
    skip.push((0x801f_0000, 0x1_0000));
    let skipped = |i: u32| skip.iter().any(|&(a, n)| (a & 0x1f_ffff..(a & 0x1f_ffff) + n).contains(&i));
    for f in 0..frames {
        script.apply(&mut original, f);
        script.apply(&mut port, f);
        original.frame().expect("original frame");
        port.frame().expect("ported frame");
        let (a, b) = (&original.m.bus.ram, &port.m.bus.ram);
        let diffs: Vec<u32> = (0..a.len() as u32).filter(|&i| a[i as usize] != b[i as usize] && !skipped(i)).collect();
        if !diffs.is_empty() {
            tracing::error!("frame {f}: {} bytes differ", diffs.len());
            for &i in diffs.iter().take(24) {
                let addr = 0x8000_0000 | i;
                let place = match addr.checked_sub(CARS) {
                    Some(off) if off < 8 * CAR_SIZE => format!(" car {} +0x{:x}", off / CAR_SIZE, off % CAR_SIZE),
                    _ => String::new(),
                };
                tracing::error!("  {addr:08x}{place}: original {:02x}, port {:02x}", a[i as usize], b[i as usize]);
            }
            std::process::exit(1);
        }
        if (f + 1) % 60 == 0 {
            tracing::info!("frame {}: same", f + 1);
        }
    }
    tracing::info!("{frames} frames, RAM the same throughout");
}
