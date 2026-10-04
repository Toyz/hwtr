//! The pad reads against the original's.

mod common;

use hwtr_game::pad::layout::{MAPPING, PAD_BUFFER};
use hwtr_game::pad::{Mapping, PadKind, PadReader, PadState};
use hwtr_game::ram::Ram;

#[test]
fn reads_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    let mapping = {
        let ram = Ram(&mut m.bus.ram);
        Mapping(std::array::from_fn(|k| ram.i32(MAPPING + 4 * k as u32) as u32))
    };
    assert_eq!(mapping, Mapping::default(), "the state's mapping is the default");
    let mut reader = PadReader::read_ram(&Ram(&mut m.bus.ram), 0);
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
        let original = PadReader::read_ram(&Ram(&mut m.bus.ram), 0);
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
