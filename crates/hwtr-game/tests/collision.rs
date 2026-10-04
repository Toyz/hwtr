//! The collision code against the original, on the track loaded in a saved
//! race.

mod common;

use hwtr_game::collision::Scp;
use hwtr_game::ram::Ram;

/// Where the game keeps its pointer to the loaded SCP.
const SCP: u32 = 0x800d_2658;

/// The SCP as the saved race has it in RAM.
pub fn scp_from(ram: &mut [u8]) -> Scp {
    let ram = Ram(ram);
    let at = ram.i32(SCP) as u32;
    let sizes = [20, 24, 12, 20, 12, 32];
    let len = 240 + (0..6).map(|k| ram.i32(at + 4 * k) as usize * sizes[k as usize]).sum::<usize>();
    let bytes: Vec<u8> = (0..len as u32).map(|k| ram.u8(at + k)).collect();
    Scp::parse(&bytes).expect("the SCP parses")
}

#[test]
fn zone_at_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    let scp = scp_from(&mut m.bus.ram);
    assert!(scp.zones.len() > 100, "{} zones", scp.zones.len());
    let mut rng = common::Rng(0x2011_e000_0000_0009);
    let car = Ram(&mut m.bus.ram).vec3(0x8012_8fcc + 0x13c - 0x30);
    for round in 0..3000 {
        // Around the car, and anywhere on the track's scale.
        let p = if round % 2 == 0 {
            car.map(|c| c.wrapping_add((rng.word() as i32) >> (8 + rng.below(12))))
        } else {
            [0; 3].map(|_| (rng.word() as i32) >> rng.below(8))
        };
        let want = m.call(0x8004_dc50, &[p[0] as u32, p[1] as u32, p[2] as u32, 0]).unwrap() as u16;
        assert_eq!(scp.zone_at(p), want, "zone_at({p:?})");
    }
}
