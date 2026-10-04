//! The front end's font spacing and screen layout against the original, on
//! the main menu (`work/states/menu-main`).

mod common;

use hwtr_game::front::font::ScreenFont;
use hwtr_game::front::screen::{Line, Text};
use hwtr_hle::original::Ram;

fn font(exe: &hwtr_psx::Exe) -> Option<ScreenFont> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/SCREENSBIG/SCRNFNTOVL");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipped: {} not found", path.display());
        return None;
    };
    let f = hwtr_game::hud::Font::parse(&bytes).expect("SCRNFNT.OVL parses");
    let view = exe.view();
    let mut font = ScreenFont::new(&f, |a| view.u8(a).unwrap_or(0));
    // The menu state has loaded the font once.
    font.prepare();
    Some(font)
}

#[test]
fn spacing_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(font) = font(&exe) else { return };
    let Some(mut m) = common::state(&exe, "menu-main") else { return };
    let mut rng = common::Rng(0x5eed_f0f0);
    let alphabet = hwtr_game::front::font::ALPHABET;
    // Every pair the kerning table lists, then random ones.
    let listed = b"LTLYLtLyKOKoYAYOYSYaYoYsOTOYyoytlyltA Y.,.";
    for round in 0..4000 {
        let pick = |rng: &mut common::Rng| {
            if rng.below(8) == 0 { rng.below(256) as u8 } else { alphabet[rng.below(75) as usize] }
        };
        let (a, b) = match listed.get(2 * round..2 * round + 2) {
            Some(&[a, b]) => (a, b),
            _ => (pick(&mut rng), pick(&mut rng)),
        };
        let f = rng.below(3);
        let original = m.call(0x8008_76ac, &[f, a as u32, b as u32]).unwrap() as i32;
        let port = font.advance(a, b);
        assert_eq!(original, port, "round {round}: advance {:?} {:?}", a as char, b as char);
        let original = m.call(0x8008_7764, &[f, a as u32]).unwrap() as u8 as i32;
        assert_eq!(original, font.last(a), "round {round}: last {:?}", a as char);
    }
}

#[test]
fn main_menu_lays_out_as_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(font) = font(&exe) else { return };
    let Some(mut m) = common::state(&exe, "menu-main") else { return };
    let ram = Ram(&mut m.bus.ram);
    let screen = ram.i32(0x800c_24f8) as u32;
    let texts = ram.i32(screen + 4) as u32;
    let mut letters = 0;
    for i in 0..ram.u8(screen) as u32 {
        let t = texts + 44 * i;
        let len = ram.u8(t + 0x18) as u32;
        let s: String = (0..len).map(|k| ram.u8(t + k) as char).collect();
        let mut port = Text::default();
        port.set(&Line {
            text: &s,
            x: ram.i16(t + 0x20),
            y: ram.i16(t + 0x22),
            extra: ram.i16(t + 0x24),
            entry: ram.u8(t + 0x26),
            centred: ram.u8(t + 0x27) != 0,
            font: ram.u8(t + 0x28),
            colour: [ram.u8(t + 0x29), ram.u8(t + 0x2a), ram.u8(t + 0x2b)],
        });
        port.copy_letters();
        port.lay_out(&font);
        let recs = ram.i32(t + 0x1c) as u32;
        for (k, l) in port.letters.iter().enumerate() {
            let r = recs + 10 * k as u32;
            assert_eq!(l.ch, ram.u8(r), "text {i} letter {k}");
            let to = (ram.i16(r + 6), ram.i16(r + 8));
            assert_eq!(l.to, to, "text {i} {s:?} letter {k}");
            // The menu has long since slid into place.
            assert_eq!((ram.i16(r + 2), ram.i16(r + 4)), to, "text {i} letter {k} in place");
            letters += 1;
        }
    }
    assert!(letters > 40, "{letters} letters");

    // The labels, from the screen table read out of the executable.
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let strings = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/SCREENSBIG/ENGLISHHWT"),
    )
    .ok()
    .and_then(|b| hwtr_game::front::strings::Strings::parse(&b))
    .expect("ENGLISH.HWT");
    let mut screens = hwtr_game::front::screen::read_screens(&byte, &strings);
    let menu = &mut screens[4];
    menu.boot(&font);
    menu.enter(&font);
    let labels = ram.i32(screen + 0x14) as u32;
    for (i, l) in menu.labels.iter().enumerate() {
        let q = labels + 24 * i as u32;
        let recs = ram.i32(q + 4) as u32;
        assert_eq!(l.letters.len(), ram.u8(q + 2) as usize, "label {i}");
        for (k, g) in l.letters.iter().enumerate() {
            let r = recs + 10 * k as u32;
            assert_eq!(g.to, (ram.i16(r + 6), ram.i16(r + 8)), "label {i} letter {k}");
        }
    }
}

/// A new game's settings and player records, as 0x80088544 and 0x80088474
/// leave them in memory, are what the port writes on the card.
#[test]
fn new_records_match_the_card_layout() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "menu-main") else { return };
    const SETTINGS: u32 = 0x8013_8f14;
    const PLAYER: u32 = 0x8013_97a4;
    m.call(0x8008_8544, &[SETTINGS]).unwrap();
    m.call(0x8008_8474, &[PLAYER, 0]).unwrap();
    let ram = Ram(&mut m.bus.ram);
    let strings = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/SCREENSBIG/ENGLISHHWT"),
    )
    .ok()
    .and_then(|b| hwtr_game::front::strings::Strings::parse(&b))
    .expect("ENGLISH.HWT");
    let settings = hwtr_game::front::Settings::new(ram.u8(0x8013_6a38));
    let mut save = hwtr_game::front::card::Save::new(ram.i32(SETTINGS) as u32, strings.get(84), settings);
    let mut player = hwtr_game::front::Profile::new(&strings, 0);
    player.tag = ram.i32(PLAYER) as u32;
    player.id = ram.i32(PLAYER + 4) as u32;
    save.players.push(player);
    let bytes = save.to_bytes();
    // The settings record. The table of cup winners is cleared as if it
    // had 60 lines, so the clearing runs on past the record; what it leaves
    // at +0x87b to +0x883 (stack bytes and a zero) the port does not copy.
    for k in 4..0x87b {
        let name_tail = (k - 4) % 16 >= 6 && (k - 4) % 16 < 12;
        if name_tail {
            continue; // past a name's end: the original's stack
        }
        assert_eq!(bytes[k as usize], ram.u8(SETTINGS + k), "settings +{k:#x}");
    }
    // The player's record, but for its name's tail (sprintf's buffer).
    for k in 0..0xb4u32 {
        if (0x11..0x14).contains(&k) {
            continue;
        }
        assert_eq!(bytes[0x888 + k as usize], ram.u8(PLAYER + k), "player +{k:#x}");
    }
}
