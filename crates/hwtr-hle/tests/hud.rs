//! The HUD button (action 12, 0x8006545c) against the original: the HUD
//! turned off and on by presses, and announcements cancelled by either
//! edge.

mod common;

use hwtr_hle::original::Ram;

/// Each player's show mask, the mask when on, the button's last level.
const SHOW: u32 = 0x800d_0e58;
const SHOW_ON: u32 = 0x800d_0e5c;
const LAST: u32 = 0x800d_0e60;
/// The announcement halves' done bytes: 240 a player, 32 a line, +0x21 and
/// +0x31.
const LINES: u32 = 0x800b_ead0;

#[test]
fn the_hud_button_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let mut rng = common::Rng(0x40d0_0000_0000_0012);
    let mut hud = hwtr_game::hud::Hud::default();
    let (mut toggled, mut cancelled) = (0, 0);
    for round in 0..20 {
        // A fresh mask, the button up, every half still going.
        let on = [rng.word() as u16 & 0x7ff, rng.word() as u16 & 0x7ff];
        {
            let mut ram = Ram(&mut m.bus.ram);
            for p in 0..2u32 {
                ram.set_i16(SHOW + 2 * p, on[p as usize] as i16);
                ram.set_i16(SHOW_ON + 2 * p, on[p as usize] as i16);
                ram.set_u8(LAST + p, 0);
                for k in 0..3 {
                    ram.set_u8(LINES + 240 * p + 32 * k + 0x21, 0);
                    ram.set_u8(LINES + 240 * p + 32 * k + 0x31, 0);
                }
            }
        }
        hud.show = on;
        hud.show_on = on;
        hud.reset_hud_button();
        for p in 0..2 {
            for line in &mut hud.announcements[p].slides {
                for half in line {
                    half.done = false;
                }
            }
        }
        for step in 0..60 {
            let player = rng.below(2) as usize;
            let level = [0u8, 255, 255, 0, 128][rng.below(5) as usize];
            let before = hud.show[player];
            m.call(0x8006_545c, &[player as u32, level as u32]).unwrap();
            hud.hud_button(player, level);
            let ram = Ram(&mut m.bus.ram);
            for p in 0..2usize {
                assert_eq!(
                    ram.i16(SHOW + 2 * p as u32) as u16,
                    hud.show[p],
                    "round {round} step {step}: player {p}'s mask"
                );
                for (k, line) in hud.announcements[p].slides.iter().enumerate() {
                    let at = LINES + 240 * p as u32 + 32 * k as u32;
                    let done = [ram.u8(at + 0x21) != 0, ram.u8(at + 0x31) != 0];
                    assert_eq!(done, [line[0].done, line[1].done], "round {round} step {step}: player {p} line {k}");
                }
            }
            toggled += (hud.show[player] != before) as u32;
            cancelled += hud.announcements[player].slides.iter().flatten().filter(|h| h.done).count() as u32;
            // Some halves going again, the same on both sides.
            if rng.below(3) == 0 {
                let mut ram = Ram(&mut m.bus.ram);
                for k in 0..3 {
                    ram.set_u8(LINES + 240 * player as u32 + 32 * k + 0x21, 0);
                    hud.announcements[player].slides[k as usize][0].done = false;
                }
            }
        }
    }
    eprintln!("toggled {toggled}, halves done {cancelled}");
    assert!(toggled > 0 && cancelled > 0);
}
