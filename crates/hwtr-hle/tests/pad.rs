//! The pad reads against the original's.

mod common;

use hwtr_hle::original::InMemory;
use hwtr_hle::original::pad::{MAPPING, PAD_BUFFER};
use hwtr_game::pad::{Mapping, PadKind, PadReader, PadState};
use hwtr_hle::original::Ram;

#[test]
fn reads_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    let mapping = {
        let ram = Ram(&mut m.bus.ram);
        Mapping(std::array::from_fn(|k| ram.i32(MAPPING + 4 * k as u32) as u32))
    };
    assert_eq!(mapping, Mapping::default(), "the state's mapping is the default");
    let mut reader = <PadReader as InMemory>::read(&Ram(&mut m.bus.ram), hwtr_hle::original::pad::port(0));
    let mut rng = common::Rng(0x9ad0_0000_0000_0001);
    let mut seen = [0u32; 8];
    for frame in 0..3000 {
        // Runs of the same pad, so ramps and easing get somewhere.
        let kind = if (frame / 300) % 2 == 0 { PadKind::Digital } else { PadKind::Analog };
        let buttons = if frame % 7 == 0 { rng.word() as u16 } else { reader_buttons(frame, &mut rng) };
        let pad = PadState {
            kind,
            buttons,
            left: [rng.below(256) as u8, rng.below(256) as u8],
            right: [rng.below(256) as u8, rng.below(256) as u8],
        };
        let elapsed = [17, 34, 0, 51, 200, rng.below(400)][rng.below(6) as usize];
        let b = !pad.buttons;
        let bytes = match kind {
            PadKind::Digital => vec![0x00, 0x41, b as u8, (b >> 8) as u8],
            PadKind::Analog => {
                vec![0x00, 0x73, b as u8, (b >> 8) as u8, pad.right[0], pad.right[1], pad.left[0], pad.left[1]]
            }
        };
        m.bus.load(PAD_BUFFER, &bytes);
        m.call(0x8001_bec0, &[0, elapsed]).unwrap();
        reader.read(&pad, &mapping, elapsed);
        let original = <PadReader as InMemory>::read(&Ram(&mut m.bus.ram), hwtr_hle::original::pad::port(0));
        assert_eq!(original, reader, "frame {frame}: {pad:?}, {elapsed} ms");
        for (s, l) in seen.iter_mut().zip(original.levels) {
            *s += (l != 0 && l != 255) as u32;
        }
    }
    assert!(seen.iter().all(|&s| s > 0), "partial levels per action: {seen:?}");
}

/// Buttons held for stretches: one or two of the d-pad, Cross, Square.
fn reader_buttons(frame: u32, rng: &mut common::Rng) -> u16 {
    const SOME: [u16; 6] = [1 << 7, 1 << 5, 1 << 4, 1 << 6, 1 << 14, 1 << 15];
    let k = (frame / 23) as usize;
    let mut b = SOME[k % 6];
    if (frame / 41).is_multiple_of(3) {
        b |= SOME[(k + 1 + (frame as usize / 97)) % 6];
    }
    if rng.below(50) == 0 { 0 } else { b }
}

#[test]
fn front_end_actions_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    let mapping = Mapping::default();
    let mut rng = common::Rng(0x9ad0_0000_0000_0002);
    let mut held = 0;
    for round in 0..2000 {
        let buttons = if rng.below(2) == 0 { 1 << rng.below(16) } else { rng.word() as u16 };
        let kind = if rng.below(2) == 0 { PadKind::Digital } else { PadKind::Analog };
        let pad = PadState { kind, buttons, ..PadState::default() };
        let b = !buttons;
        m.bus.load(PAD_BUFFER, &[0x00, if kind == PadKind::Digital { 0x41 } else { 0x73 }, b as u8, (b >> 8) as u8]);
        let action = 14 + rng.below(14) as u8;
        let original = m.call(0x8001_aebc, &[0, action as u32]).unwrap() != 0;
        assert_eq!(original, pad.holds(&mapping, action), "round {round}: action {action}, buttons {buttons:#06x}");
        held += original as u32;
    }
    assert!(held > 100, "{held} held");
}

/// The motors (0x8001b7b4 the road's feel, 0x8001b934 a jolt, 0x8001ccc4 the
/// wind-down at each read) against the original, on random runs; libpad is
/// told a DualShock is there.
#[test]
fn motors_match_the_original() {
    use hwtr_game::pad::Motors;
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    m.stub(0x800a_7e44, 6); // PadGetState: stable, with actuators
    m.stub(0x800a_7f10, 1); // PadInfoAct
    m.stub(0x800a_8114, 1); // PadSetAct
    m.stub(0x800a_80dc, 1); // PadSetActAlign
    const ACT: u32 = 0x8011_b2b8;
    const CLOCK: u32 = 0x800d_240c;
    const STATE: u32 = 0x800d_246c;
    let mut rng = common::Rng(0x0b00_b1e5);
    let mut port_motors = [Motors::default(); 2];
    let mut clock = 1000u32;
    let mut ran = 0;
    for round in 0..6000 {
        let port = rng.below(2) as usize;
        let on = rng.below(8) != 0;
        m.bus.write_u32(MAPPING + 0x74 * port as u32 + 0x70, on as u32);
        clock = clock.wrapping_add(rng.below(60));
        m.bus.write_u32(CLOCK, clock);
        let state = [0u8, 1, 1, 1, 2, 3][rng.below(6) as usize];
        m.bus.write(STATE, 1, state as u32).unwrap();
        let motors = &mut port_motors[port];
        match rng.below(3) {
            0 => {
                let (r, s) = (if rng.below(4) == 0 { 0 } else { rng.below(256) }, rng.below(256));
                m.call(0x8001_b7b4, &[port as u32, r, s]).unwrap();
                motors.rumble(on, r as u8, s as u8);
            }
            1 => {
                let level = rng.below(256);
                m.call(0x8001_b934, &[port as u32, level]).unwrap();
                motors.jolt(on, level as u8, clock);
            }
            _ => {
                let elapsed = rng.below(200);
                m.call(0x8001_ccc4, &[port as u32, elapsed]).unwrap();
                motors.fade(on, elapsed, !matches!(state, 0 | 2 | 3), clock);
            }
        }
        let rec = ACT + 98 * port as u32;
        let (small, large) = (m.bus.read(rec + 0x18, 1).unwrap() as u8, m.bus.read(rec + 0x19, 1).unwrap() as u8);
        assert_eq!((small != 0, large), (motors.small, motors.large), "round {round}, port {port}");
        ran += (large != 0) as u32;
    }
    assert!(ran > 500, "{ran} rounds with the motor running");
}
