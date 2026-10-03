//! The ported functions, as checks on the original and as replacements.
//!
//! `shadow` checks each ported function on every call the running game makes:
//! the port runs on a copy of RAM taken as the original function is entered,
//! and when the original returns the two must agree. The game itself runs
//! unchanged, so its timing does too. `install` replaces the originals
//! outright.

use std::rc::Rc;

use hwtr_game::math::Tables;
use hwtr_game::ram::Ram;

/// (address, name) of every function `install` replaces.
pub const PORTED: &[(u32, &str)] = &[(0x8004_0a90, "update_wheels")];

/// Hooks the ported functions into `m`.
pub fn install(m: &mut hwtr_cpu::Machine) {
    let t = Rc::new(Tables::from_ram(&m.bus.ram));
    let t1 = t.clone();
    m.hook(0x8004_0a90, move |cpu, bus| {
        hwtr_game::car::update_wheels(&t1, &mut Ram(&mut bus.ram), cpu.r[4]);
        0
    });
}

/// Compares RAM after the original against the port's copy.
fn compare(original: &[u8], port: &[u8], skip: &[(u32, u32)]) -> Result<(), String> {
    let skipped = |i: u32| skip.iter().any(|&(a, n)| (a & 0x1f_ffff..(a & 0x1f_ffff) + n).contains(&i));
    let diffs: Vec<String> = (0..original.len() as u32)
        .filter(|&i| original[i as usize] != port[i as usize] && !skipped(i))
        .take(8)
        .map(|i| {
            format!("{:08x}: original {:02x}, port {:02x}", 0x8000_0000 | i, original[i as usize], port[i as usize])
        })
        .collect();
    if diffs.is_empty() { Ok(()) } else { Err(diffs.join("; ")) }
}

/// Adds a shadow check for every ported function to `m`.
pub fn shadow(m: &mut hwtr_cpu::Machine) {
    let t = Rc::new(Tables::from_ram(&m.bus.ram));
    let skip = Rc::new(unmatched());
    let (t1, skip1) = (t.clone(), skip.clone());
    m.check(0x8004_0a90, move |cpu, bus| {
        let mut ram = bus.ram.clone();
        hwtr_game::car::update_wheels(&t1, &mut Ram(&mut ram), cpu.r[4]);
        let skip = skip1.clone();
        Box::new(move |_, bus| compare(&bus.ram, &ram, &skip))
    });
}

/// Main RAM the port is not expected to reproduce: the stack, and words the
/// original fills from uninitialised stack.
pub fn unmatched() -> Vec<(u32, u32)> {
    use hwtr_game::car::{CAR_SIZE, CARS, WHEEL_SIZE, WHEELS, wheel};
    let mut v = vec![(0x801f_0000, 0x1_0000)];
    for k in 0..8 {
        for i in 0..4 {
            v.push((CARS + k * CAR_SIZE + WHEELS + i * WHEEL_SIZE + wheel::HEADING + 12, 4));
        }
    }
    v
}
