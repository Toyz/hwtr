//! The race's reverb against the original: the registers libspu leaves for
//! "studio large", and the depth 0x80015d04 eases and writes.

mod common;

use std::rc::Rc;

/// The SPU's registers from 0x1f801c00, a half word each.
fn reg(hle: &hwtr_hle::Hle, addr: u32) -> u16 {
    hle.hw.borrow().spu_regs[((addr - 0x1f80_1c00) / 2) as usize]
}

fn race(name: &str) -> Option<hwtr_hle::Hle> {
    let cue = rrt::disc::Image::find(std::path::Path::new("../../work/disc")).ok()?;
    let disc = Rc::new(rrt::disc::Image::open(&cue).ok()?);
    let mut hle = hwtr_hle::Hle::new(disc).ok()?;
    let bytes = std::fs::read(format!("../../work/states/{name}.bin")).ok()?;
    hle.load(&bytes).ok()?;
    hle.m.step_limit = 100_000_000;
    Some(hle)
}

#[test]
fn studio_large_is_what_the_race_sets() {
    let Some(hle) = race("desert1-race") else {
        eprintln!("skipped: no disc or state");
        return;
    };
    let regs: Vec<u16> = (0..32).map(|k| reg(&hle, 0x1f80_1dc0 + 2 * k)).collect();
    assert_eq!(regs, hwtr_game::snd::STUDIO_LARGE);
    assert_eq!(reg(&hle, 0x1f80_1da2), hwtr_game::snd::STUDIO_LARGE_BASE);
    assert_ne!(reg(&hle, 0x1f80_1daa) & 0x80, 0, "the reverb is on");
}

#[test]
fn the_reverb_depth_eases_as_in_the_original() {
    // The depth so far (0x800d0c40) and the output volume registers.
    const DEPTH: u32 = 0x800d_0c40;
    let Some(mut hle) = race("desert1-race") else {
        eprintln!("skipped: no disc or state");
        return;
    };
    let mut ours = hwtr_game::engines::Reverb::default();
    let mut rng = common::Rng(0x2e7e_0000_0000_0019);
    let mut changed = 0;
    for step in 0..400 {
        // In and out of a reverb zone.
        let level = if rng.below(40) < 22 { 4096 } else { 0 };
        hle.m.call(0x8001_5d04, &[0, level as u32]).unwrap();
        let change = ours.step(level);
        let original = hwtr_hle::original::Ram(&mut hle.m.bus.ram).i32(DEPTH);
        assert_eq!(original, ours.depth, "step {step}");
        let Some(hwtr_game::engines::Change::ReverbDepth(d)) = change else { panic!("step {step}: no depth") };
        let want = hwtr_game::snd::reverb_depth(d) as u16;
        assert_eq!((reg(&hle, 0x1f80_1d84), reg(&hle, 0x1f80_1d86)), (want, want), "step {step}: the output volume");
        changed += (d != 0) as u32;
    }
    eprintln!("depth nonzero on {changed} of 400 steps");
    assert!(changed > 0);
}

#[test]
fn the_pause_turns_the_reverb_off_and_on_as_in_the_original() {
    const SOUNDING: u32 = 0x800d_24d4;
    const PAUSED: u32 = 0x800d_24d3;
    let Some(mut hle) = race("desert1-race") else {
        eprintln!("skipped: no disc or state");
        return;
    };
    let mut ours = hwtr_game::engines::Reverb::default();
    // Into a reverb zone for a while, then the pause, the easing ignored
    // while paused, going on, and the easing again.
    for step in 0..60 {
        let pause = match step {
            30 | 45 => Some(()),
            _ => None,
        };
        if pause.is_some() {
            hle.m.call(0x8001_9f7c, &[]).unwrap();
            let paused = !ours.paused;
            let _ = ours.pause(paused);
        } else {
            hle.m.call(0x8001_5d04, &[0, 4096]).unwrap();
            let _ = ours.step(4096);
        }
        let ram = hwtr_hle::original::Ram(&mut hle.m.bus.ram);
        assert_eq!(ram.u8(PAUSED) != 0, ours.paused, "step {step}: paused");
        assert_eq!(ram.u8(SOUNDING) != 0, ours.sounding, "step {step}: sounding");
        assert_eq!(ram.i32(0x800d_0c40), ours.depth, "step {step}: depth");
    }
}
