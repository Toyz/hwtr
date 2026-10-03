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
pub const PORTED: &[(u32, &str)] =
    &[(0x8004_0a90, "update_wheels"), (0x8004_1af0, "aero"), (0x8006_0138, "drivetrain")];

/// Hooks the ported functions into `m`.
pub fn install(m: &mut hwtr_cpu::Machine) {
    let t = Rc::new(Tables::from_ram(&m.bus.ram));
    let t1 = t.clone();
    m.hook(0x8004_0a90, move |cpu, bus| {
        hwtr_game::car::update_wheels(&t1, &mut Ram(&mut bus.ram), cpu.r[4]);
        0
    });
    m.hook(0x8004_1af0, aero);
    let t2 = t.clone();
    m.hook(0x8006_0138, move |cpu, bus| {
        hwtr_game::car::drivetrain(&t2, &mut Ram(&mut bus.ram), cpu.r[4]);
        0
    });
}

/// `aero` as the original is called: car, and where to put the two results.
fn aero(cpu: &mut hwtr_cpu::Cpu, bus: &mut hwtr_cpu::Bus) -> u32 {
    let mut ram = Ram(&mut bus.ram);
    let (a, b) = hwtr_game::car::aero(&mut ram, cpu.r[4]);
    ram.set_i32(cpu.r[5], a);
    ram.set_i32(cpu.r[6], b);
    0
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

/// Adds a shadow check for every ported function to `m`: the port runs on a
/// copy of RAM through the same entry point a hook would use, and RAM must
/// match when the original returns, apart from the stack below the entry
/// `sp` (the call's own frames), the 16 bytes above it (where the callee may
/// save its register arguments) and `unmatched` words.
pub fn shadow(m: &mut hwtr_cpu::Machine) {
    let t = Rc::new(Tables::from_ram(&m.bus.ram));
    let t1 = t.clone();
    let wheels = move |cpu: &mut hwtr_cpu::Cpu, bus: &mut hwtr_cpu::Bus| {
        hwtr_game::car::update_wheels(&t1, &mut Ram(&mut bus.ram), cpu.r[4]);
        0
    };
    shadow_one(m, 0x8004_0a90, wheels);
    shadow_one(m, 0x8004_1af0, aero);
    let t2 = t.clone();
    let drivetrain = move |cpu: &mut hwtr_cpu::Cpu, bus: &mut hwtr_cpu::Bus| {
        hwtr_game::car::drivetrain(&t2, &mut Ram(&mut bus.ram), cpu.r[4]);
        0
    };
    shadow_one(m, 0x8006_0138, drivetrain);
}

fn shadow_one(
    m: &mut hwtr_cpu::Machine,
    addr: u32,
    port: impl Fn(&mut hwtr_cpu::Cpu, &mut hwtr_cpu::Bus) -> u32 + 'static,
) {
    let skip = Rc::new(unmatched());
    m.check(addr, move |cpu, bus| {
        let (mut cpu, mut bus2) = (cpu.clone(), hwtr_cpu::Bus { ram: bus.ram.clone(), ..Default::default() });
        port(&mut cpu, &mut bus2);
        let ram = bus2.ram;
        let mut skip = (*skip).clone();
        let sp = cpu.r[29];
        // The call's frames, and the argument save area above them.
        skip.push((sp.wrapping_sub(0x4000), 0x4000 + 16));
        Box::new(move |_, bus| compare(&bus.ram, &ram, &skip))
    });
}

/// Words the port is not expected to reproduce: those the original fills
/// from uninitialised stack.
pub fn unmatched() -> Vec<(u32, u32)> {
    use hwtr_game::car::{CAR_SIZE, CARS, WHEEL_SIZE, WHEELS, wheel};
    let mut v = Vec::new();
    for k in 0..8 {
        for i in 0..4 {
            v.push((CARS + k * CAR_SIZE + WHEELS + i * WHEEL_SIZE + wheel::HEADING + 12, 4));
        }
    }
    v
}
