//! The sound library's key-on (0x800a67c4) against the original: the
//! SPU settings it leaves in its shadow registers for a voice, on the
//! front end's bank (`HWMENU`, bank 0 in `menu-main`).

mod common;

use hwtr_game::snd::{Bank, Tables, key_on};

const SHADOW: u32 = 0x8014_31e4;

#[test]
fn key_on_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "menu-main") else { return };
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/SCREENSBIG/HWMENUVH");
    let Ok(vh) = std::fs::read(&path) else {
        eprintln!("skipped: {} not found", path.display());
        return;
    };
    let bank = Bank::from_vh(&vh, 0).expect("HWMENU.VH");
    let view = exe.view();
    let tables = Tables::read(&|a| view.u8(a).unwrap_or(0));
    let present: Vec<usize> = (0..128).filter(|&p| bank.programs[p].is_some()).collect();
    let mut rng = common::Rng(0x50_0d);
    let mut delta = None;
    let mut keyed = 0;
    for round in 0..3000 {
        let voice = rng.below(24);
        let program = present[rng.below(present.len() as u32) as usize];
        let tone = rng.below(16) as usize;
        let (note, fine) = (rng.below(128) as i32, rng.below(128) as i32);
        let (left, right) = (rng.below(128) as i32, rng.below(128) as i32);
        let Some(port) = key_on(&bank, &tables, program, tone, note, fine, left, right, false, 0) else { continue };
        let got = m.call(0x800a_67c4, &[voice, 0, program as u32, tone as u32, note as u32, fine as u32, left as u32, right as u32]).unwrap();
        if got as i16 != voice as i16 {
            continue; // a tone libsnd turns down (none in the bank's range)
        }
        let r = SHADOW + 16 * voice;
        let half = |m: &mut hwtr_cpu::Machine, a: u32| m.bus.read(a, 2).unwrap() as u16;
        let original = (
            [half(&mut m, r), half(&mut m, r + 2)],
            half(&mut m, r + 4),
            half(&mut m, r + 6),
            half(&mut m, r + 8),
            half(&mut m, r + 10),
        );
        let d = *delta.get_or_insert(original.2.wrapping_sub(port.start));
        assert_eq!(
            original,
            (port.volume, port.pitch, port.start.wrapping_add(d), port.adsr1, port.adsr2),
            "round {round}: voice {voice} program {program} tone {tone} note {note} fine {fine} at {left}/{right}"
        );
        keyed += 1;
    }
    assert!(keyed > 500, "{keyed} keyed");
}
