//! A stunt's announcement against the original: the lines set up
//! (0x80064dec) and their halves sliding in, standing and sliding off
//! frame by frame (0x80064724), the record and the letters drawn compared.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use hwtr_game::front::font::ScreenFont;
use hwtr_game::front::screen::Style;
use hwtr_game::front::strings::Strings;
use hwtr_game::hud::{Announcement, Font, Hud, Slide, StuntWords};
use hwtr_game::math::Tables;
use hwtr_game::pause::PauseKit;
use hwtr_game::rand::Rand;
use hwtr_hle::original::Ram;
use hwtr_hle::original::rand::SEED;

/// Player one's announcement record.
const RECORD: u32 = 0x800b_ead0;
const PLAYERS: u32 = 0x800d_24d0;
/// The general interface's glyph draw (+0x44).
const DRAW_GLYPH: u32 = 0x8012_fd74;
/// The race clock as the HUD reads it.
const NOW: u32 = 0x8006_122c;

fn work(path: &str) -> Option<Vec<u8>> {
    std::fs::read(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big").join(path)).ok()
}

fn c_string(ram: &Ram, at: u32) -> String {
    (0..40).map(|k| ram.u8(at + k)).take_while(|&b| b != 0).map(char::from).collect()
}

fn slide(ram: &Ram, at: u32) -> Slide {
    Slide {
        lo: ram.i16(at),
        hi: ram.i16(at + 2),
        x: ram.i16(at + 4),
        y: ram.i16(at + 6),
        speed: ram.i16(at + 8),
        width: ram.i16(at + 10) as u16,
        settled: ram.u8(at + 12) != 0,
        done: ram.u8(at + 13) != 0,
        squeeze: ram.u8(at + 14),
    }
}

fn record(ram: &Ram) -> Announcement {
    Announcement {
        active: ram.u8(RECORD + 237) != 0,
        from: ram.i32(RECORD + 4) as u32,
        points: ram.i32(RECORD + 16),
        lines: std::array::from_fn(|k| c_string(ram, RECORD + 116 + 40 * k as u32)),
        slides: std::array::from_fn(|k| {
            let at = RECORD + 20 + 32 * k as u32;
            [slide(ram, at), slide(ram, at + 16)]
        }),
    }
}

#[test]
fn a_stunt_announcement_slides_as_in_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let (Some(hwt), Some(scrn), Some(actn)) =
        (work("SCREENSBIG/ENGLISHHWT"), work("SCREENSBIG/SCRNFNTOVL"), work("DESERT1BIG/ACTNFNTOVL"))
    else {
        return;
    };
    let strings = Strings::parse(&hwt).unwrap();
    let mut kerning = ScreenFont::new(&Font::parse(&scrn).unwrap(), byte);
    kerning.prepare();
    let kit = PauseKit::new(&byte, &strings, &Font::parse(&actn).unwrap(), kerning).unwrap();
    let style = kit.style().clone();
    let words =
        StuntWords { points: strings.get(217).into(), turbo: strings.get(292).into(), turbos: strings.get(293).into() };
    assert_eq!(Ram(&mut m.bus.ram).u8(PLAYERS), 1, "a one-player race");

    let now = Rc::new(RefCell::new(0u32));
    let clock = now.clone();
    m.hook(NOW, move |_, _| *clock.borrow());
    let drawn = Rc::new(RefCell::new(Vec::new()));
    let log = drawn.clone();
    let widths = style.clone();
    let glyph = m.bus.read_u32(DRAW_GLYPH);
    m.hook(glyph, move |cpu, _| {
        let c = cpu.r[5] as u8;
        log.borrow_mut().push((c.to_ascii_uppercase(), cpu.r[6] as i16, cpu.r[7] as i16));
        widths.last(c) as u32
    });

    let mut hud = Hud::default();
    hud.players = 1;
    let mut rng = common::Rng(0x57_0a7);
    let start = m.bus.ram.clone();
    let mut letters = 0;
    for round in 0..60 {
        m.bus.ram.copy_from_slice(&start);
        // The stunt's name is drawn in there (0x80080d6c).
        let stunt = rng.below(49) as u16;
        let points = [0, 0, 150, 1000, 2500, 40000][rng.below(6) as usize];
        let turbos = rng.below(4) as u8;
        let seed = rng.word();
        Ram(&mut m.bus.ram).set_i32(SEED, seed as i32);
        *now.borrow_mut() = 1 + rng.below(100_000);
        m.call(0x8006_4dec, &[0, points as u32, stunt as u32, turbos as u32]).unwrap();
        let mut rand = Rand { seed };
        let name = t.stunts.name(stunt, &mut rand).map_or("", |n| strings.get(n as usize));
        assert_eq!(m.bus.read_u32(SEED), rand.seed, "round {round}: seed");
        hud.announce(0, points, name, turbos, &words, &style);
        let what = format!("round {round}: {name:?} for {points} and {turbos}");
        assert_eq!(record(&Ram(&mut m.bus.ram)), hud.announcements[0], "{what}: set up");
        for frame in 0..150 {
            *now.borrow_mut() += 17;
            drawn.borrow_mut().clear();
            m.call(0x8006_4724, &[0]).unwrap();
            let ours = hud.announcement(0, *now.borrow(), &style);
            let ours: Vec<_> = ours.iter().map(|s| (s.glyph, s.x, s.y)).collect();
            assert_eq!(*drawn.borrow(), ours, "{what}: frame {frame}'s letters");
            assert_eq!(record(&Ram(&mut m.bus.ram)), hud.announcements[0], "{what}: frame {frame}");
            letters += ours.len();
        }
    }
    assert!(letters > 10_000, "{letters} letters");
}
