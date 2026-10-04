//! The ported functions, as checks on the original and as replacements.
//!
//! Each entry in [`PORTED`] pairs an original function with an adapter: it
//! reads the port's types out of RAM through their codecs, runs the port,
//! and writes them back, taking arguments and leaving results as the
//! original's calling convention does.
//!
//! `shadow` checks each ported function on every call the running game
//! makes: the adapter runs on a copy of RAM taken as the original function
//! is entered, and when the original returns the two must agree. The game
//! itself runs unchanged, so its timing does too. `install` replaces the
//! originals outright.

use crate::original::InMemory;
use std::rc::Rc;

use hwtr_cpu::{Bus, Cpu, Machine};
use hwtr_game::body::Body;
use crate::original::car::{BODY, CAR_SIZE, CARS, WHEEL_SIZE, WHEELS, wheel};
use hwtr_game::car::{Car, Tuning};
use hwtr_game::math::Tables;
use crate::original::Ram;

/// Runs a port in place of the original: arguments in the CPU's registers,
/// memory on the bus; returns v0.
pub type Adapter = fn(&Tables, &mut Cpu, &mut Bus) -> u32;

/// Every ported function: (address, name, adapter).
pub const PORTED: &[(u32, &str, Adapter)] = &[
    (0x8004_0a90, "place_wheels", place_wheels),
    (0x8004_1af0, "aero", aero),
    (0x8006_0138, "drivetrain", drivetrain),
    (0x8004_27a8, "physics", physics),
    (0x8004_4fc4, "spin_wheels", spin_wheels),
    (0x8006_c504, "integrate", integrate),
    (0x8002_5be4, "orthonormalize", orthonormalize),
    (0x8003_d71c, "air_control", air_control),
    (0x8007_1bc0, "align", align),
    (0x8003_d5cc, "damp_spin", damp_spin),
    (0x8006_dc08, "impulse", impulse),
];

/// Runs `f` on the car at `at`, read out of RAM and written back.
fn on_car<R>(bus: &mut Bus, at: u32, f: impl FnOnce(&mut Car, &Tuning) -> R) -> R {
    let mut ram = Ram(&mut bus.ram);
    let (mut car, tuning) = (Car::read(&ram, at), crate::original::car::tuning(&ram));
    let r = f(&mut car, &tuning);
    car.write(&mut ram, at);
    r
}

fn place_wheels(t: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_car(bus, cpu.r[4], |car, _| car.place_wheels(t));
    0
}

/// The original takes the car and two pointers for the downforce.
fn aero(_: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    let down = on_car(bus, cpu.r[4], |car, tuning| car.aero(tuning));
    let mut ram = Ram(&mut bus.ram);
    ram.set_i32(cpu.r[5], down.front);
    ram.set_i32(cpu.r[6], down.rear);
    0
}

fn drivetrain(t: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_car(bus, cpu.r[4], |car, _| car.drivetrain(t));
    0
}

fn physics(t: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_car(bus, cpu.r[4], |car, tuning| car.physics(t, tuning));
    0
}

fn spin_wheels(_: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_car(bus, cpu.r[4], |car, _| car.spin_wheels());
    0
}

/// The original takes the body and the step's length.
fn integrate(t: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_body(bus, cpu.r[4], |body| body.integrate(t, cpu.r[5] as i32));
    0
}

fn air_control(_: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_car(bus, cpu.r[4], |car, _| car.air_control());
    0
}

/// The contact at a0 (its object's body, point and normal) takes an
/// impulse with friction a1, the bounce as for a car.
fn impulse(t: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    let contact = cpu.r[4];
    let (body, point, normal, kind) = {
        let ram = Ram(&mut bus.ram);
        let object = ram.i32(contact) as u32;
        (ram.i32(object + 0x64) as u32, ram.vec3(contact + 4), ram.vec3(contact + 0x14), ram.u8(object + 0x60))
    };
    let bounce = if kind == 6 { 0x14cc } else { 0x800 };
    let mut size = 0;
    on_body(bus, body, |b| size = b.impulse(t, point, normal, bounce, cpu.r[5] as i32));
    size as u32
}

/// Runs `f` on the body at `at`, read out of RAM and written back.
fn on_body(bus: &mut Bus, at: u32, f: impl FnOnce(&mut Body)) {
    let mut ram = Ram(&mut bus.ram);
    let mut body = Body::read(&ram, at);
    f(&mut body);
    body.write(&mut ram, at);
}

/// The original takes the body, the axis, and a VECTOR by value: two words
/// in registers, the third on the stack.
fn align(_: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    let dir = [cpu.r[6] as i32, cpu.r[7] as i32, bus.read_u32(cpu.r[29] + 16) as i32];
    on_body(bus, cpu.r[4], |body| body.align(cpu.r[5] as u8, dir));
    0
}

/// The original takes the car.
fn damp_spin(_: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    on_body(bus, cpu.r[4] + BODY, Body::damp_spin);
    0
}

/// The original takes a MATRIX.
fn orthonormalize(t: &Tables, cpu: &mut Cpu, bus: &mut Bus) -> u32 {
    let mut ram = Ram(&mut bus.ram);
    let m = t.orthonormalize(&ram.matrix(cpu.r[4]));
    ram.set_matrix(cpu.r[4], &m);
    0
}

/// Hooks the ported functions into `m` in place of the originals.
pub fn install(m: &mut Machine) {
    let t = Rc::new(crate::original::tables(&m.bus.ram));
    for &(addr, _, adapter) in PORTED {
        let t = t.clone();
        m.hook(addr, move |cpu, bus| adapter(&t, cpu, bus));
    }
}

/// Adds a shadow check for every ported function to `m`. RAM must match
/// when the original returns, apart from the stack below the entry `sp`
/// (the call's own frames), the 16 bytes above it (where the callee may save
/// its register arguments) and [`unmatched`] words.
pub fn shadow(m: &mut Machine) {
    let t = Rc::new(crate::original::tables(&m.bus.ram));
    let skip = Rc::new(unmatched());
    for &(addr, _, adapter) in PORTED {
        let (t, skip) = (t.clone(), skip.clone());
        m.check(addr, move |cpu, bus| {
            let (mut cpu, mut copy) = (cpu.clone(), Bus { ram: bus.ram.clone(), ..Default::default() });
            adapter(&t, &mut cpu, &mut copy);
            let sp = cpu.r[29];
            let mut skip = (*skip).clone();
            skip.push((sp.wrapping_sub(0x4000), 0x4000 + 16));
            let ram = copy.ram;
            Box::new(move |_, bus| compare(&bus.ram, &ram, &skip))
        });
    }
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

/// Words the port is not expected to reproduce: those the original fills
/// from uninitialised stack.
pub fn unmatched() -> Vec<(u32, u32)> {
    (0..8)
        .flat_map(|k| (0..6).map(move |i| (CARS + k * CAR_SIZE + WHEELS + i * WHEEL_SIZE + wheel::HEADING_PAD, 4)))
        .collect()
}
