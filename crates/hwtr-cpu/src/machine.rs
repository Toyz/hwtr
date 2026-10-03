//! The harness: load the executable, call one of its functions with chosen
//! arguments, and stop when it returns.
//!
//! Anything the function reaches that should not run (library calls into
//! hardware, the BIOS, functions outside the part under test) is replaced by
//! a hook: a host closure that runs instead and returns a value.

use std::collections::HashMap;

use hwtr_psx::Exe;

use crate::bus::Bus;
use crate::cpu::{Cpu, Fault};

/// The return address `call` plants in `ra`; reaching it ends the call.
pub const RETURN: u32 = 0xbfc0_fff0;
/// Where `call` puts the stack.
pub const STACK_TOP: u32 = 0x801f_ff00;

/// A hook runs instead of the code at its address and returns `v0`.
pub type Hook = Box<dyn FnMut(&mut Cpu, &mut Bus) -> u32>;

pub struct Machine {
    pub cpu: Cpu,
    pub bus: Bus,
    hooks: HashMap<u32, Hook>,
    /// Calls into the BIOS tables: (table, function) -> v0.
    pub bios: HashMap<(u32, u32), Hook>,
    /// Instructions executed by the last `call`.
    pub steps: u64,
    pub step_limit: u64,
    /// Addresses executed, when set.
    pub trace: Option<Vec<u32>>,
    /// Bump allocator for the BIOS malloc.
    heap: u32,
}

impl Default for Machine {
    fn default() -> Self {
        let mut m = Machine {
            cpu: Cpu::default(),
            bus: Bus::default(),
            hooks: HashMap::new(),
            bios: HashMap::new(),
            steps: 0,
            step_limit: 50_000_000,
            trace: None,
            heap: 0x8018_0000,
        };
        m.install_bios();
        m
    }
}

fn ret(v: u32) -> Hook {
    Box::new(move |_, _| v)
}

impl Machine {
    /// A machine with the executable's image loaded at its address and `$gp`
    /// set from its crt0.
    pub fn with_exe(exe: &Exe) -> Machine {
        let mut m = Machine::default();
        m.bus.load(exe.t_addr, &exe.image);
        if let Some(gp) = hwtr_psx::analysis::find_gp(&exe.view(), exe.pc0) {
            m.cpu.r[28] = gp;
        }
        m
    }

    /// Runs `hook` instead of the function at `addr`.
    pub fn hook(&mut self, addr: u32, hook: impl FnMut(&mut Cpu, &mut Bus) -> u32 + 'static) {
        self.hooks.insert(addr, Box::new(hook));
    }

    /// Makes the function at `addr` return `v` without running.
    pub fn stub(&mut self, addr: u32, v: u32) {
        self.hooks.insert(addr, ret(v));
    }

    pub fn unhook(&mut self, addr: u32) {
        self.hooks.remove(&addr);
    }

    /// The BIOS functions the PsyQ library reaches that are pure enough to
    /// provide on the host. Others stop the call with [`Fault::Bios`].
    fn install_bios(&mut self) {
        let a0 = |c: &Cpu| c.r[4];
        let a1 = |c: &Cpu| c.r[5];
        let a2 = |c: &Cpu| c.r[6];
        // memcpy(dst, src, n)
        self.bios.insert(
            (0xa0, 0x2a),
            Box::new(move |c, b| {
                let bytes = b.bytes(a1(c), a2(c) as usize);
                b.load(a0(c), &bytes);
                a0(c)
            }),
        );
        // memset(dst, value, n)
        self.bios.insert(
            (0xa0, 0x2b),
            Box::new(move |c, b| {
                b.load(a0(c), &vec![a1(c) as u8; a2(c) as usize]);
                a0(c)
            }),
        );
        // strcmp, strcpy, strlen
        self.bios.insert(
            (0xa0, 0x17),
            Box::new(move |c, b| {
                let (x, y) = (b.cstr(a0(c)), b.cstr(a1(c)));
                match x.as_bytes().cmp(y.as_bytes()) {
                    std::cmp::Ordering::Less => -1i32 as u32,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                }
            }),
        );
        self.bios.insert(
            (0xa0, 0x19),
            Box::new(move |c, b| {
                let mut s = b.cstr(a1(c)).into_bytes();
                s.push(0);
                b.load(a0(c), &s);
                a0(c)
            }),
        );
        self.bios.insert((0xa0, 0x1b), Box::new(move |c, b| b.cstr(a0(c)).len() as u32));
        // printf and puts print nothing.
        self.bios.insert((0xa0, 0x3f), ret(0));
        self.bios.insert((0xb0, 0x3f), ret(0));
        // FlushCache
        self.bios.insert((0xa0, 0x44), ret(0));
    }

    /// Calls `func` with `args` (the first four in a0-a3, the rest on the
    /// stack) and runs until it returns. Returns v0.
    pub fn call(&mut self, func: u32, args: &[u32]) -> Result<u32, Fault> {
        let sp = STACK_TOP - 16 - 4 * args.len().saturating_sub(4) as u32;
        for (i, &a) in args.iter().enumerate() {
            if i < 4 {
                self.cpu.r[4 + i] = a;
            } else {
                self.bus.write_u32(sp + 16 + 4 * (i as u32 - 4), a);
            }
        }
        self.cpu.r[29] = sp;
        self.cpu.r[31] = RETURN;
        self.cpu.jump(func);
        self.run()
    }

    /// Runs from the current pc until `RETURN` is reached.
    pub fn run(&mut self) -> Result<u32, Fault> {
        self.steps = 0;
        loop {
            let pc = self.cpu.pc;
            if pc == RETURN {
                self.cpu.settle();
                return Ok(self.cpu.r[2]);
            }
            if self.steps >= self.step_limit {
                return Err(Fault::StepLimit { pc });
            }
            // A hook or a BIOS entry replaces the function: run it, then return
            // to ra as if the function had. Hooks only fire on a function's
            // first instruction, never in a delay slot.
            if self.cpu.next_pc == pc.wrapping_add(4) {
                let replaced = if let Some(h) = self.hooks.get_mut(&pc) {
                    self.cpu.settle();
                    Some(h(&mut self.cpu, &mut self.bus))
                } else if matches!(pc, 0xa0 | 0xb0 | 0xc0) {
                    self.cpu.settle();
                    let key = (pc, self.cpu.r[9] & 0xff);
                    if key == (0xa0, 0x33) {
                        // malloc: a bump allocator, 8-byte aligned.
                        let at = self.heap;
                        self.heap = (self.heap + self.cpu.r[4] + 7) & !7;
                        Some(at)
                    } else if key == (0xa0, 0x34) {
                        Some(0)
                    } else {
                        let h = self.bios.get_mut(&key).ok_or(Fault::Bios { pc, table: key.0, function: key.1 })?;
                        Some(h(&mut self.cpu, &mut self.bus))
                    }
                } else {
                    None
                };
                if let Some(v0) = replaced {
                    self.cpu.r[2] = v0;
                    let ra = self.cpu.r[31];
                    self.cpu.jump(ra);
                    continue;
                }
            }
            if let Some(t) = &mut self.trace {
                t.push(pc);
            }
            self.cpu.step(&mut self.bus)?;
            self.steps += 1;
        }
    }
}
