//! The players' passwords against the original: made from random records
//! (0x80069ae4), and read back, and read from random and cheat words
//! (0x80069d78).

mod common;

use hwtr_game::front::password::Passwords;
use hwtr_game::front::Profile;
use hwtr_game::pad::Mapping;
use hwtr_hle::original::Ram;

/// Free memory well below the stack.
const RECORD: u32 = 0x801f_8000;
const TEXT: u32 = RECORD + 0x100;

fn profile(rng: &mut common::Rng) -> Profile {
    let word = |rng: &mut common::Rng| rng.below(1 << 16) | rng.below(1 << 16) << 16;
    Profile {
        tag: 0,
        id: 0,
        name: String::new(),
        cars: [word(rng), word(rng) & 0x3ff],
        tracks: word(rng) & 0xfff,
        car: 0,
        track: 0,
        progress: [rng.below(4) as u8, if rng.below(3) == 0 { 0 } else { rng.below(16) as u8 }],
        records: std::array::from_fn(|_| rng.below(300)),
        cheats: 0,
        mapping: Mapping::default(),
    }
}

fn put(ram: &mut Ram, p: &Profile) {
    for k in 0..0xc0 {
        ram.set_u8(RECORD + k, 0);
    }
    ram.set_i32(RECORD + 0x14, p.cars[0] as i32);
    ram.set_i32(RECORD + 0x18, p.cars[1] as i32);
    ram.set_i32(RECORD + 0x1c, p.tracks as i32);
    ram.set_u8(RECORD + 0x22, p.progress[0]);
    ram.set_u8(RECORD + 0x23, p.progress[1]);
    for (k, &r) in p.records.iter().enumerate() {
        ram.set_i32(RECORD + 0x24 + 4 * k as u32, r as i32);
    }
}

fn get(ram: &Ram) -> ([u32; 2], u32, [u8; 2], [u32; 6]) {
    (
        [ram.i32(RECORD + 0x14) as u32, ram.i32(RECORD + 0x18) as u32],
        ram.i32(RECORD + 0x1c) as u32,
        [ram.u8(RECORD + 0x22), ram.u8(RECORD + 0x23)],
        std::array::from_fn(|k| ram.i32(RECORD + 0x24 + 4 * k as u32) as u32),
    )
}

fn text(ram: &Ram, at: u32) -> String {
    (0..20).map(|k| ram.u8(at + k)).take_while(|&c| c != 0).map(char::from).collect()
}

#[test]
fn passwords_are_made_as_the_original_makes_them() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "menu-main") else { return };
    let view = exe.view();
    let pw = Passwords::new(&|a| view.u8(a).unwrap_or(0));
    let mut rng = common::Rng(0x9a55_3011);
    for round in 0..500 {
        let p = profile(&mut rng);
        put(&mut Ram(&mut m.bus.ram), &p);
        m.call(0x8006_9ae4, &[RECORD, TEXT]).unwrap();
        let theirs = text(&Ram(&mut m.bus.ram), TEXT);
        assert_eq!(pw.make(&p), theirs, "round {round}: {p:?}");
    }
}

#[test]
fn passwords_are_read_as_the_original_reads_them() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "menu-main") else { return };
    let view = exe.view();
    let pw = Passwords::new(&|a| view.u8(a).unwrap_or(0));
    let alphabet = b"0123456789BCDFGHJKLMNPQRSTVWXYZ_";
    let cheats = ["V0LC4N0", "TWJM", "WH1T3C4R", "SH0RTY_L", "M0_D", "NOPE"];
    let mut rng = common::Rng(0x2ead_0001);
    let mut good = 0;
    for round in 0..600 {
        let start = profile(&mut rng);
        let word: String = match round % 3 {
            0 => pw.make(&profile(&mut rng)),
            1 => (0..20).map(|_| alphabet[rng.below(32) as usize] as char).collect(),
            _ => cheats[rng.below(cheats.len() as u32) as usize].to_string(),
        };
        let mut ours = start.clone();
        let ok = pw.read(&word, &mut ours);
        let mut ram = Ram(&mut m.bus.ram);
        put(&mut ram, &start);
        for k in 0..24 {
            ram.set_u8(TEXT + k, word.as_bytes().get(k as usize).copied().unwrap_or(0));
        }
        let r = m.call(0x8006_9d78, &[TEXT, RECORD, 0]).unwrap();
        let ram = Ram(&mut m.bus.ram);
        assert_eq!(ok, r == 0, "round {round}: {word}");
        assert_eq!((ours.cars, ours.tracks, ours.progress, ours.records), get(&ram), "round {round}: {word}");
        good += ok as u32;
    }
    assert!(good > 250, "{good} read");
}
