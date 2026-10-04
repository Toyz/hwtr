//! The pause menu's volume steps against the original: a tenth up
//! (0x8009f388) and down (0x8009f4a4) from every level, and whether each
//! says it moved (0x800d2806, 0x800d2805).

mod common;

use hwtr_game::pause::{volume_down, volume_up};
use hwtr_hle::original::Ram;

#[test]
fn volumes_step_as_the_original_steps_them() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    for v in 0..=255u8 {
        let up = m.call(0x8009_f388, &[v as u32]).unwrap() as u8;
        let moved_up = Ram(&mut m.bus.ram).u8(0x800d_2806) != 0;
        assert_eq!(volume_up(v), (up, moved_up), "{v} up");
        let down = m.call(0x8009_f4a4, &[v as u32]).unwrap() as u8;
        let moved_down = Ram(&mut m.bus.ram).u8(0x800d_2805) != 0;
        assert_eq!(volume_down(v), (down, moved_down), "{v} down");
    }
}
