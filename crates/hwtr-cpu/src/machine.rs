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
    /// The BIOS heap behind malloc and free.
    pub heap: Heap,
    /// Set by a hook to stop `run` after the hook returns.
    pub halt: std::rc::Rc<std::cell::Cell<bool>>,
    pub vblank: VBlank,
    /// The return address that ends the innermost nested call.
    ret: u32,
    /// Words to watch: after each instruction, a change is recorded with the
    /// pc that made it (address, value last seen).
    pub watch: Vec<(u32, u32)>,
    /// Changes seen: (pc, address, old, new).
    pub watched: Vec<(u32, u32, u32, u32)>,
    /// The CPU as it stood at the last fault, even one inside a nested call
    /// (whose caller's state is put back before the fault is returned).
    pub fault_cpu: Option<Cpu>,
    /// Debugging: stop with a fault when sp drops below this (0: off).
    pub sp_guard: u32,
    /// Instructions executed since the machine was made; devices read it to
    /// time their work.
    pub clock: std::rc::Rc<std::cell::Cell<u64>>,
    /// Interrupts devices have scheduled: (due on `clock`, handler). Each
    /// runs like the vertical blank's handler, between instructions, when
    /// interrupts are on.
    pub pending: std::rc::Rc<std::cell::RefCell<Vec<(u64, u32)>>>,
}

/// The vertical blank interrupt, delivered every `period` instructions (or
/// sooner, when something asks to wait for it): the handler runs between two
/// instructions with the CPU's state saved around it, the counter advances,
/// and `run` returns `Fault::Halt` so the host sees a frame boundary.
#[derive(Default)]
pub struct VBlank {
    /// Instructions per frame; 0 turns the interrupt off.
    pub period: u64,
    pub since: u64,
    /// The handler to run, 0 for none.
    pub handler: std::rc::Rc<std::cell::Cell<u32>>,
    /// Blanks to deliver right away (a VSync(n) waiting for them).
    pub skip: std::rc::Rc<std::cell::Cell<u32>>,
    /// Blanks delivered.
    pub count: std::rc::Rc<std::cell::Cell<u64>>,
    /// A word in RAM the blank increments (libetc's counter), 0 for none.
    pub counter: u32,
    /// The return address the handler sees: where the kernel's dispatcher
    /// would have called it from. Some code reads it (as a stray stack
    /// argument), so it should be the real one. 0 for the harness's own.
    pub return_to: u32,
    /// Interrupts are off inside a critical section (Enter/ExitCriticalSection).
    pub masked: bool,
    /// The stack the handler runs on, as the kernel's exception stack would
    /// be, 0 to run below the interrupted code's stack.
    pub stack: u32,
    in_handler: bool,
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
            heap: Heap::new(0x8018_0000, 0x8_0000),
            halt: Default::default(),
            vblank: VBlank::default(),
            ret: RETURN,
            watch: Vec::new(),
            watched: Vec::new(),
            fault_cpu: None,
            sp_guard: 0,
            clock: Default::default(),
            pending: Default::default(),
        };
        m.install_bios();
        m
    }
}

/// A first-fit heap with coalescing, standing in for the BIOS's. Blocks are
/// 8-byte aligned. The game is linked for 8 MB (`_ramsize` 0x800000) and lives
/// in 2 MB only because freed blocks are reused, so a heap that never reuses
/// memory would run off the end of RAM and wrap onto the code.
#[derive(Clone, Debug)]
pub struct Heap {
    /// (address, size, in use), in address order, covering the whole heap.
    pub blocks: Vec<(u32, u32, bool)>,
}

impl Heap {
    pub fn new(base: u32, size: u32) -> Heap {
        let start = (base + 7) & !7;
        Heap { blocks: vec![(start, (size.saturating_sub(start - base)) & !7, false)] }
    }

    /// Returns 0 when nothing fits.
    pub fn alloc(&mut self, n: u32) -> u32 {
        let n = (n.max(1) + 7) & !7;
        let Some(i) = self.blocks.iter().position(|b| !b.2 && b.1 >= n) else { return 0 };
        let (at, size, _) = self.blocks[i];
        self.blocks[i] = (at, n, true);
        if size > n {
            self.blocks.insert(i + 1, (at + n, size - n, false));
        }
        at
    }

    pub fn free(&mut self, at: u32) {
        let Some(i) = self.blocks.iter().position(|b| b.0 == at && b.2) else { return };
        self.blocks[i].2 = false;
        if i + 1 < self.blocks.len() && !self.blocks[i + 1].2 {
            self.blocks[i].1 += self.blocks[i + 1].1;
            self.blocks.remove(i + 1);
        }
        if i > 0 && !self.blocks[i - 1].2 {
            self.blocks[i - 1].1 += self.blocks[i].1;
            self.blocks.remove(i);
        }
    }

    /// Bytes in use.
    pub fn used(&self) -> u32 {
        self.blocks.iter().filter(|b| b.2).map(|b| b.1).sum()
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
        // The kernel's event and interrupt plumbing the libraries set up.
        // Events are handed out as ids and always report as delivered.
        let next_event = std::rc::Rc::new(std::cell::Cell::new(0xf100_0000u32));
        self.bios.insert(
            (0xb0, 0x08),
            Box::new(move |_, _| {
                next_event.set(next_event.get() + 1);
                next_event.get()
            }),
        );
        for f in [0x07, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x20] {
            // DeliverEvent, CloseEvent, WaitEvent, TestEvent, EnableEvent,
            // DisableEvent, UnDeliverEvent
            self.bios.insert((0xb0, f), ret(1));
        }
        for f in [0x18, 0x19, 0x5b] {
            // ResetEntryInt, HookEntryInt, ChangeClearPad
            self.bios.insert((0xb0, f), ret(0));
        }
        for f in [0x02, 0x03, 0x0a] {
            // SysEnqIntRP, SysDeqIntRP, ChangeClearRCnt
            self.bios.insert((0xc0, f), ret(0));
        }
        // ReturnFromException never returns to the caller in a kernel; here
        // it just returns.
        self.bios.insert((0xb0, 0x17), ret(0));
        // The memory card: no card, every open fails.
        for f in [0x4a, 0x4b, 0x4c] {
            // InitCARD, StartCARD, StopCARD
            self.bios.insert((0xb0, f), ret(1));
        }
        for f in [0x32, 0x42] {
            // open, firstfile
            self.bios.insert((0xb0, f), ret(u32::MAX));
        }
        for f in [0x33, 0x34, 0x35, 0x36, 0x43, 0x4e, 0x4f, 0x50] {
            // lseek, read, write, close, nextfile, _card_write, _card_read, _new_card
            self.bios.insert((0xb0, f), ret(0));
        }
        // CdRemove
        self.bios.insert((0xa0, 0x72), ret(0));
        // GetC0Table, GetB0Table: where the real BIOS keeps them, in the
        // kernel's low RAM (zeros here; the libraries patch entries in).
        self.bios.insert((0xb0, 0x56), ret(0x674));
        self.bios.insert((0xb0, 0x57), ret(0x874));
        // GPU_cw, GPU_cwp(list, n), SendGP1Command, GetGPUStatus: through
        // the ports, to whatever device is attached.
        self.bios.insert(
            (0xa0, 0x49),
            Box::new(move |c, b| {
                b.write(0x1f80_1810, 4, a0(c));
                0
            }),
        );
        self.bios.insert(
            (0xa0, 0x4a),
            Box::new(move |c, b| {
                for k in 0..a1(c) {
                    let w = b.read_u32(a0(c) + 4 * k);
                    b.write(0x1f80_1810, 4, w);
                }
                0
            }),
        );
        self.bios.insert(
            (0xa0, 0x48),
            Box::new(move |c, b| {
                b.write(0x1f80_1814, 4, a0(c));
                0
            }),
        );
        self.bios.insert((0xa0, 0x4d), Box::new(move |_, b| b.read(0x1f80_1814, 4).unwrap_or(0)));
        // _bu_init: the memory card filing system.
        self.bios.insert((0xa0, 0x70), ret(0));
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

    /// Calls a function from between instructions of a run in progress (an
    /// interrupt handler, say): the CPU's state is saved and put back after,
    /// so the interrupted code carries on as if nothing ran.
    pub fn call_nested(&mut self, func: u32, args: &[u32]) -> Result<u32, Fault> {
        self.call_nested_from(func, args, RETURN)
    }

    /// `call_nested` with the callee returning to `ret` (which then ends the
    /// call) instead of the harness's own return address.
    pub fn call_nested_from(&mut self, func: u32, args: &[u32], ret: u32) -> Result<u32, Fault> {
        self.call_nested_on(func, args, ret, 0)
    }

    /// `call_nested_from` on a given stack (0: just below the interrupted
    /// code's).
    pub fn call_nested_on(&mut self, func: u32, args: &[u32], ret: u32, stack: u32) -> Result<u32, Fault> {
        self.cpu.settle();
        let saved = self.cpu.clone();
        let saved_ret = std::mem::replace(&mut self.ret, ret);
        let steps = self.steps;
        // Below the interrupted code's stack, as an exception handler would be.
        let sp = if stack != 0 { stack & !7 } else { (saved.r[29] - 0x400) & !7 };
        for (i, &a) in args.iter().enumerate().take(4) {
            self.cpu.r[4 + i] = a;
        }
        self.cpu.r[29] = sp;
        self.cpu.r[31] = ret;
        self.cpu.jump(func);
        let r = self.run();
        self.cpu = saved;
        self.steps = steps;
        self.ret = saved_ret;
        r
    }

    /// Runs from the current pc until `RETURN` is reached.
    pub fn run(&mut self) -> Result<u32, Fault> {
        self.steps = 0;
        loop {
            let pc = self.cpu.pc;
            if pc == RETURN || pc == self.ret {
                self.cpu.settle();
                return Ok(self.cpu.r[2]);
            }
            if self.steps >= self.step_limit {
                return Err(Fault::StepLimit { pc });
            }
            // Device interrupts that have come due.
            if !self.vblank.in_handler && !self.vblank.masked && self.cpu.next_pc == pc.wrapping_add(4) {
                let now = self.clock.get();
                let due = {
                    let mut p = self.pending.borrow_mut();
                    p.iter().position(|&(t, _)| t <= now).map(|i| p.remove(i))
                };
                if let Some((_, handler)) = due {
                    self.vblank.in_handler = true;
                    let ret = if self.vblank.return_to != 0 { self.vblank.return_to } else { RETURN };
                    let r = self.call_nested_on(handler, &[], ret, self.vblank.stack);
                    self.vblank.in_handler = false;
                    match r {
                        Ok(_) | Err(Fault::Halt { .. }) => {}
                        Err(f) => return Err(f),
                    }
                    continue;
                }
            }
            // The vertical blank, taken between instructions, never in a
            // delay slot, never inside its own handler.
            if self.vblank.period > 0
                && !self.vblank.in_handler
                && !self.vblank.masked
                && self.cpu.next_pc == pc.wrapping_add(4)
                && (self.vblank.skip.get() > 0 || self.vblank.since >= self.vblank.period)
            {
                self.vblank.skip.set(self.vblank.skip.get().saturating_sub(1));
                self.vblank.since = 0;
                self.vblank.count.set(self.vblank.count.get() + 1);
                if self.vblank.counter != 0 {
                    let v = self.bus.read_u32(self.vblank.counter);
                    self.bus.write_u32(self.vblank.counter, v.wrapping_add(1));
                }
                let handler = self.vblank.handler.get();
                if handler != 0 {
                    self.vblank.in_handler = true;
                    let ret = if self.vblank.return_to != 0 { self.vblank.return_to } else { RETURN };
                    let r = self.call_nested_on(handler, &[], ret, self.vblank.stack);
                    self.vblank.in_handler = false;
                    match r {
                        Ok(_) | Err(Fault::Halt { .. }) => {}
                        Err(f) => return Err(f),
                    }
                }
                return Err(Fault::Halt { pc });
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
                    if key == (0xa0, 0x39) {
                        self.heap = Heap::new(self.cpu.r[4], self.cpu.r[5]);
                        Some(0)
                    } else if key == (0xa0, 0x33) {
                        Some(self.heap.alloc(self.cpu.r[4]))
                    } else if key == (0xa0, 0x34) {
                        self.heap.free(self.cpu.r[4]);
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
                    if self.halt.replace(false) {
                        return Err(Fault::Halt { pc: ra });
                    }
                    continue;
                }
            }
            if let Some(t) = &mut self.trace {
                t.push(pc);
            }
            match self.cpu.step(&mut self.bus) {
                Ok(()) => {}
                Err(f) if !matches!(f, Fault::Syscall { .. }) => {
                    self.fault_cpu = Some(self.cpu.clone());
                    return Err(f);
                }
                // The kernel's critical sections, as PsyQ calls them: a0 = 1
                // Enter (returns whether interrupts were on), 2 Exit.
                Err(Fault::Syscall { pc, .. }) if matches!(self.cpu.r[4], 1 | 2) => {
                    // Enter returns whether interrupts were on before.
                    let was_on = !self.vblank.masked;
                    self.vblank.masked = self.cpu.r[4] == 1;
                    self.cpu.r[2] = was_on as u32;
                    self.cpu.jump(pc + 4);
                }
                Err(f) => return Err(f),
            }
            self.steps += 1;
            self.vblank.since += 1;
            self.clock.set(self.clock.get() + 1);
            if self.sp_guard != 0
                && !self.vblank.in_handler
                && self.cpu.r[29] < self.sp_guard
                && self.cpu.r[29] >= 0x8000_0000
            {
                self.fault_cpu = Some(self.cpu.clone());
                return Err(Fault::Address { pc, addr: self.cpu.r[29] });
            }
            for i in 0..self.watch.len() {
                let (addr, last) = self.watch[i];
                let now = self.bus.read_u32(addr);
                if now != last {
                    self.watched.push((pc, addr, last, now));
                    self.watch[i].1 = now;
                }
            }
        }
    }
}
