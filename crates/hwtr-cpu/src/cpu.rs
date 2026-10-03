//! The R3000A: MIPS I with its load delay slot and branch delay slot, COP0
//! as plain registers, and the GTE as COP2.
//!
//! Exceptions do not vector anywhere; they stop the step and are reported as
//! a [`Fault`] for the harness to deal with.

use hwtr_psx::mips::{Gte as GteOp, Op, decode};

use crate::bus::Bus;
use crate::gte::Gte;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Syscall {
        pc: u32,
        code: u32,
    },
    Break {
        pc: u32,
        code: u32,
    },
    Overflow {
        pc: u32,
    },
    /// Misaligned load, store or fetch.
    Address {
        pc: u32,
        addr: u32,
    },
    /// Nothing mapped there.
    Bus {
        pc: u32,
        addr: u32,
    },
    Reserved {
        pc: u32,
        word: u32,
    },
    /// The harness's step budget ran out.
    StepLimit {
        pc: u32,
    },
    /// A BIOS function the harness does not provide.
    Bios {
        pc: u32,
        table: u32,
        function: u32,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Cpu {
    pub r: [u32; 32],
    pub hi: u32,
    pub lo: u32,
    /// The instruction about to execute.
    pub pc: u32,
    /// The one after it: pc + 4, or a branch target when pc is a delay slot.
    pub next_pc: u32,
    pub cop0: [u32; 32],
    pub gte: Gte,
    /// A load issued by the previous instruction, landing after this one.
    load: Option<(usize, u32)>,
}

impl Cpu {
    /// Sets the program counter, clearing any branch in flight.
    pub fn jump(&mut self, pc: u32) {
        self.pc = pc;
        self.next_pc = pc.wrapping_add(4);
    }

    /// Lets a pending delayed load land now (for a harness about to inspect
    /// or replace registers between instructions).
    pub fn settle(&mut self) {
        if let Some((r, v)) = self.load.take() {
            self.r[r] = v;
        }
    }

    /// Executes one instruction.
    pub fn step(&mut self, bus: &mut Bus) -> Result<(), Fault> {
        let pc = self.pc;
        if pc & 3 != 0 {
            return Err(Fault::Address { pc, addr: pc });
        }
        let word = bus.read(pc, 4).ok_or(Fault::Bus { pc, addr: pc })?;
        let insn = decode(word);
        self.pc = self.next_pc;
        self.next_pc = self.pc.wrapping_add(4);

        // Reads see the registers before the pending load lands; writes go to
        // `out`, where the pending load has landed, so a write by this
        // instruction to the same register wins over it.
        let mut out = self.r;
        if let Some((reg, v)) = self.load.take() {
            out[reg] = v;
        }
        let mut new_load = None;
        let r = self.r;
        let (rs, rt, rd) = (insn.rs(), insn.rt(), insn.rd());
        let s = r[rs];
        let t = r[rt];
        let imm_s = insn.simm() as u32;
        let imm_u = insn.imm() as u32;
        let addr = s.wrapping_add(imm_s);

        macro_rules! branch {
            ($cond:expr) => {
                if $cond {
                    self.next_pc = insn.target(pc).unwrap();
                }
            };
        }
        macro_rules! load {
            ($width:expr, $align:expr, $conv:expr) => {{
                if addr & $align != 0 {
                    return Err(Fault::Address { pc, addr });
                }
                let v = bus.read(addr, $width).ok_or(Fault::Bus { pc, addr })?;
                if rt != 0 {
                    new_load = Some((rt, $conv(v)));
                }
            }};
        }
        macro_rules! store {
            ($width:expr, $align:expr) => {{
                if addr & $align != 0 {
                    return Err(Fault::Address { pc, addr });
                }
                bus.write(addr, $width, t).ok_or(Fault::Bus { pc, addr })?;
            }};
        }

        match insn.op {
            Op::Sll => out[rd] = t << insn.sa(),
            Op::Srl => out[rd] = t >> insn.sa(),
            Op::Sra => out[rd] = ((t as i32) >> insn.sa()) as u32,
            Op::Sllv => out[rd] = t << (s & 31),
            Op::Srlv => out[rd] = t >> (s & 31),
            Op::Srav => out[rd] = ((t as i32) >> (s & 31)) as u32,
            Op::Jr => self.next_pc = s,
            Op::Jalr => {
                out[rd] = pc.wrapping_add(8);
                self.next_pc = s;
            }
            Op::Syscall => return Err(Fault::Syscall { pc, code: (word >> 6) & 0xfffff }),
            Op::Break => return Err(Fault::Break { pc, code: (word >> 6) & 0xfffff }),
            Op::Mfhi => out[rd] = self.hi,
            Op::Mthi => self.hi = s,
            Op::Mflo => out[rd] = self.lo,
            Op::Mtlo => self.lo = s,
            Op::Mult => {
                let p = (s as i32 as i64) * (t as i32 as i64);
                self.lo = p as u32;
                self.hi = (p >> 32) as u32;
            }
            Op::Multu => {
                let p = (s as u64) * (t as u64);
                self.lo = p as u32;
                self.hi = (p >> 32) as u32;
            }
            Op::Div => {
                let (n, d) = (s as i32, t as i32);
                if d == 0 {
                    self.hi = n as u32;
                    self.lo = if n >= 0 { 0xffff_ffff } else { 1 };
                } else if n == i32::MIN && d == -1 {
                    self.hi = 0;
                    self.lo = n as u32;
                } else {
                    self.hi = (n % d) as u32;
                    self.lo = (n / d) as u32;
                }
            }
            Op::Divu => {
                if t == 0 {
                    self.hi = s;
                    self.lo = 0xffff_ffff;
                } else {
                    self.hi = s % t;
                    self.lo = s / t;
                }
            }
            Op::Add => out[rd] = (s as i32).checked_add(t as i32).ok_or(Fault::Overflow { pc })? as u32,
            Op::Addu => out[rd] = s.wrapping_add(t),
            Op::Sub => out[rd] = (s as i32).checked_sub(t as i32).ok_or(Fault::Overflow { pc })? as u32,
            Op::Subu => out[rd] = s.wrapping_sub(t),
            Op::And => out[rd] = s & t,
            Op::Or => out[rd] = s | t,
            Op::Xor => out[rd] = s ^ t,
            Op::Nor => out[rd] = !(s | t),
            Op::Slt => out[rd] = ((s as i32) < (t as i32)) as u32,
            Op::Sltu => out[rd] = (s < t) as u32,
            Op::Bltz => branch!((s as i32) < 0),
            Op::Bgez => branch!((s as i32) >= 0),
            Op::Bltzal => {
                out[31] = pc.wrapping_add(8);
                branch!((s as i32) < 0)
            }
            Op::Bgezal => {
                out[31] = pc.wrapping_add(8);
                branch!((s as i32) >= 0)
            }
            Op::J => self.next_pc = insn.target(pc).unwrap(),
            Op::Jal => {
                out[31] = pc.wrapping_add(8);
                self.next_pc = insn.target(pc).unwrap();
            }
            Op::Beq => branch!(s == t),
            Op::Bne => branch!(s != t),
            Op::Blez => branch!((s as i32) <= 0),
            Op::Bgtz => branch!((s as i32) > 0),
            Op::Addi => out[rt] = (s as i32).checked_add(imm_s as i32).ok_or(Fault::Overflow { pc })? as u32,
            Op::Addiu => out[rt] = s.wrapping_add(imm_s),
            Op::Slti => out[rt] = ((s as i32) < (imm_s as i32)) as u32,
            Op::Sltiu => out[rt] = (s < imm_s) as u32,
            Op::Andi => out[rt] = s & imm_u,
            Op::Ori => out[rt] = s | imm_u,
            Op::Xori => out[rt] = s ^ imm_u,
            Op::Lui => out[rt] = imm_u << 16,
            Op::Lb => load!(1, 0, |v: u32| v as u8 as i8 as u32),
            Op::Lh => load!(2, 1, |v: u32| v as u16 as i16 as u32),
            Op::Lw => load!(4, 3, |v: u32| v),
            Op::Lbu => load!(1, 0, |v: u32| v),
            Op::Lhu => load!(2, 1, |v: u32| v),
            Op::Lwl | Op::Lwr => {
                let aligned = addr & !3;
                let m = bus.read(aligned, 4).ok_or(Fault::Bus { pc, addr })?;
                // The unaligned loads merge with the register's value as it
                // will be after a pending load to it lands.
                let cur = out[rt];
                let sh = (addr & 3) * 8;
                let v = if insn.op == Op::Lwl {
                    let keep = if sh == 24 { 0 } else { 0x00ff_ffff >> sh };
                    (cur & keep) | (m << (24 - sh))
                } else {
                    let keep = if sh == 0 { 0 } else { 0xffff_ff00 << (24 - sh) };
                    (cur & keep) | (m >> sh)
                };
                if rt != 0 {
                    new_load = Some((rt, v));
                }
            }
            Op::Sb => store!(1, 0),
            Op::Sh => store!(2, 1),
            Op::Sw => store!(4, 3),
            Op::Swl | Op::Swr => {
                let aligned = addr & !3;
                let m = bus.read(aligned, 4).ok_or(Fault::Bus { pc, addr })?;
                let sh = (addr & 3) * 8;
                let v = if insn.op == Op::Swl {
                    let keep = if sh == 24 { 0 } else { 0xffff_ff00 << sh };
                    (m & keep) | (t >> (24 - sh))
                } else {
                    let keep = if sh == 0 { 0 } else { 0x00ff_ffff >> (24 - sh) };
                    (m & keep) | (t << sh)
                };
                bus.write(aligned, 4, v).ok_or(Fault::Bus { pc, addr })?;
            }
            Op::Mfc0 => new_load = (rt != 0).then_some((rt, self.cop0[rd])),
            Op::Mtc0 => self.cop0[rd] = t,
            Op::Rfe => {
                let sr = self.cop0[12];
                self.cop0[12] = (sr & !0xf) | ((sr >> 2) & 0xf);
            }
            Op::Mfc2 => new_load = (rt != 0).then_some((rt, self.gte.read_data(rd))),
            Op::Cfc2 => new_load = (rt != 0).then_some((rt, self.gte.read_ctrl(rd))),
            Op::Mtc2 => self.gte.write_data(rd, t),
            Op::Ctc2 => self.gte.write_ctrl(rd, t),
            Op::Lwc2 => {
                if addr & 3 != 0 {
                    return Err(Fault::Address { pc, addr });
                }
                let v = bus.read(addr, 4).ok_or(Fault::Bus { pc, addr })?;
                self.gte.write_data(rt, v);
            }
            Op::Swc2 => {
                if addr & 3 != 0 {
                    return Err(Fault::Address { pc, addr });
                }
                let v = self.gte.read_data(rt);
                bus.write(addr, 4, v).ok_or(Fault::Bus { pc, addr })?;
            }
            Op::Gte(g) => {
                let _: GteOp = g;
                self.gte.command(word & 0x01ff_ffff);
            }
            Op::Invalid => return Err(Fault::Reserved { pc, word }),
        }
        out[0] = 0;
        self.r = out;
        self.load = new_load;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(words: &[u32], setup: impl FnOnce(&mut Cpu)) -> Cpu {
        let mut bus = Bus::default();
        let base = 0x8001_0000;
        for (i, w) in words.iter().enumerate() {
            bus.write_u32(base + i as u32 * 4, *w);
        }
        let mut cpu = Cpu::default();
        cpu.jump(base);
        setup(&mut cpu);
        for _ in 0..words.len() {
            cpu.step(&mut bus).unwrap();
        }
        cpu.settle();
        cpu
    }

    #[test]
    fn load_delay_slot_sees_the_old_value() {
        // sw a0, 0(sp); lw v0, 0(sp); move v1, v0; nop
        let cpu = run(&[0xafa4_0000, 0x8fa2_0000, 0x0040_1821, 0], |c| {
            c.r[29] = 0x8010_0000;
            c.r[4] = 7;
            c.r[2] = 3;
        });
        assert_eq!(cpu.r[3], 3, "the delay slot reads v0 before the load lands");
        assert_eq!(cpu.r[2], 7);
    }

    #[test]
    fn write_in_the_delay_slot_beats_the_load() {
        // lw v0, 0(sp); li v0, 5; nop
        let cpu = run(&[0x8fa2_0000, 0x2402_0005, 0], |c| c.r[29] = 0x8010_0000);
        assert_eq!(cpu.r[2], 5);
    }

    #[test]
    fn branch_runs_its_delay_slot() {
        // b +2; li v0, 1; li v0, 2; li v1, 3
        let cpu = run(&[0x1000_0002, 0x2402_0001, 0x2402_0002, 0x2403_0003], |_| {});
        assert_eq!(cpu.r[2], 1);
        assert_eq!(cpu.r[3], 3);
    }

    #[test]
    fn unaligned_word_through_lwl_lwr() {
        let mut bus = Bus::default();
        bus.load(0x8010_0000, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        // lwr v0, 1(a0); lwl v0, 4(a0); nop
        let words = [0x9882_0001u32, 0x8882_0004, 0];
        for (i, w) in words.iter().enumerate() {
            bus.write_u32(0x8001_0000 + i as u32 * 4, *w);
        }
        let mut cpu = Cpu::default();
        cpu.jump(0x8001_0000);
        cpu.r[4] = 0x8010_0000;
        for _ in 0..3 {
            cpu.step(&mut bus).unwrap();
        }
        cpu.settle();
        assert_eq!(cpu.r[2], 0x5544_3322);
    }
}
