//! The race's effects table (0x8001a73c) against the original, both ways:
//! the normal one, and the DUDE cheat's (option 16).

mod common;

use hwtr_game::snd::effects_table;

#[test]
fn effects_table_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let view = exe.view();
    let normal = effects_table(&|a| view.u8(a).unwrap_or(0), false);
    let changed = effects_table(&|a| view.u8(a).unwrap_or(0), true).iter().zip(&normal).filter(|(d, n)| d != n).count();
    assert!(changed > 40, "the cheat changes only {changed} effects");
    for dude in [false, true] {
        let port = effects_table(&|a| view.u8(a).unwrap_or(0), dude);
        m.call(0x8001_a73c, &[dude as u32]).unwrap();
        for (id, entry) in port.iter().enumerate() {
            let r = 0x8011_aec0 + 16 * id as u32;
            let word = |m: &mut hwtr_cpu::Machine, a: u32| m.bus.read(a, 4).unwrap();
            let original = [word(&mut m, r), word(&mut m, r + 4), word(&mut m, r + 8)];
            assert_eq!(original, *entry, "dude {dude}: id {id}");
            assert_eq!(word(&mut m, r + 12), id as u32, "dude {dude}: id {id}");
        }
    }
}
