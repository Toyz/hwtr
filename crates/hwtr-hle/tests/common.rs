//! Shared by the tests that check the port against the original executable.

use std::path::PathBuf;

/// `CCCPSX.EXE` as extracted by `hwtr-re disc extract`, or `None` (and the
/// test is skipped) when the disc has not been extracted.
pub fn exe() -> Option<hwtr_psx::Exe> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/fs/CCCPSX.EXE");
    match std::fs::read(&path) {
        Ok(bytes) => Some(hwtr_psx::Exe::parse(&bytes).expect("CCCPSX.EXE parses")),
        Err(_) => {
            eprintln!("skipped: {} not found (run `hwtr-re disc extract`)", path.display());
            None
        }
    }
}

/// A small deterministic generator for test inputs.
pub struct Rng(pub u64);

#[allow(dead_code)]
impl Rng {
    pub fn word(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }

    pub fn below(&mut self, n: u32) -> u32 {
        self.word() % n
    }
}

/// The machine from a save state written by `hwtr-hle --save` under
/// `work/states/`, with its interrupts off so a call runs alone, or `None`
/// (and the test is skipped) when there is no such state.
#[allow(dead_code)]
pub fn state(exe: &hwtr_psx::Exe, name: &str) -> Option<hwtr_cpu::Machine> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../work/states/{name}.bin"));
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipped: {} not found (make it with `hwtr-hle --save`)", path.display());
        return None;
    };
    let mut m = hwtr_cpu::Machine::with_exe(exe);
    let mut r = hwtr_cpu::state::Reader(&bytes);
    r.expect(b"hwtr-hle state 1").expect("an hwtr-hle state");
    m.load(&mut r).expect("the machine loads");
    m.vblank.period = 0;
    m.pending.borrow_mut().clear();
    m.step_limit = 100_000_000;
    Some(m)
}

/// Where `Machine::call` leaves room for results the caller passes pointers
/// to: above the stack it calls on.
#[allow(dead_code)]
pub const OUT: u32 = hwtr_cpu::machine::STACK_TOP + 0x20;

/// Compares RAM after the original and the port ran, skipping the stack below
/// the one `Machine::call` starts from (the call's own frames) and the 16
/// bytes above it (where a MIPS callee may save its register arguments), and
/// any `skip` ranges (address, length); panics on the first differences, with
/// addresses relative to `base`.
#[allow(dead_code)]
pub fn same_ram(original: &[u8], port: &[u8], base: u32, skip: &[(u32, u32)], what: &str) {
    let top = hwtr_cpu::machine::STACK_TOP - 16;
    let stack = (top - 0x8000) & 0x1f_ffff..(top & 0x1f_ffff) + 16;
    let skipped =
        |i: u32| stack.contains(&i) || skip.iter().any(|&(a, n)| (a & 0x1f_ffff..(a & 0x1f_ffff) + n).contains(&i));
    let diffs: Vec<String> = (0..original.len() as u32)
        .filter(|&i| original[i as usize] != port[i as usize] && !skipped(i))
        .take(16)
        .map(|i| {
            let a = 0x8000_0000 | i;
            format!(
                "  {a:08x} (+0x{:x}): original {:02x}, port {:02x}",
                a.wrapping_sub(base),
                original[i as usize],
                port[i as usize]
            )
        })
        .collect();
    assert!(diffs.is_empty(), "{what}: RAM differs\n{}", diffs.join("\n"));
}
