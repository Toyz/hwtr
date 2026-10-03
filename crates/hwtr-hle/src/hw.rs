//! The I/O ports the game's libraries touch: the GPU's two ports, DMA
//! channels 2 (GPU), 4 (SPU) and 6 (ordering-table clear), the SPU's
//! registers and RAM, the root counters and the interrupt registers. Every
//! transfer completes the moment it starts, so nothing ever has to be waited
//! for.

use std::cell::RefCell;
use std::rc::Rc;

use hwtr_cpu::Device;

use crate::gpu::Gpu;

pub const SPU_RAM: usize = 512 * 1024;

/// libgpu's queue processor, which its DMA-complete interrupt handler runs.
pub const GPU_QUEUE: u32 = 0x800a_20d0;
/// How long a GPU DMA keeps the channel busy, in instructions.
pub const GPU_DMA_TIME: u64 = 4000;

pub struct Hw {
    pub gpu: Gpu,
    /// The machine's instruction clock and interrupt list.
    pub clock: Rc<std::cell::Cell<u64>>,
    pub pending: Rc<RefCell<Vec<(u64, u32)>>>,
    gpu_busy_until: u64,
    /// MADR, BCR, CHCR per channel.
    dma: [[u32; 3]; 7],
    dpcr: u32,
    dicr: u32,
    pub spu_regs: Vec<u16>,
    pub spu_ram: Vec<u8>,
    spu_addr: usize,
    irq_mask: u32,
    counter: u32,
    pub unhandled: std::collections::BTreeMap<u32, u64>,
}

impl Default for Hw {
    fn default() -> Self {
        Hw {
            gpu: Gpu::default(),
            clock: Default::default(),
            pending: Default::default(),
            gpu_busy_until: 0,
            dma: [[0; 3]; 7],
            dpcr: 0x0765_4321,
            dicr: 0,
            spu_regs: vec![0; 512],
            spu_ram: vec![0; SPU_RAM],
            spu_addr: 0,
            irq_mask: 0,
            counter: 0,
            unhandled: Default::default(),
        }
    }
}

fn word(ram: &[u8], at: u32) -> u32 {
    let a = (at & 0x1f_fffc) as usize;
    u32::from_le_bytes(ram[a..a + 4].try_into().unwrap())
}

fn set_word(ram: &mut [u8], at: u32, v: u32) {
    let a = (at & 0x1f_fffc) as usize;
    ram[a..a + 4].copy_from_slice(&v.to_le_bytes());
}

impl Hw {
    fn start_dma(&mut self, ram: &mut [u8], ch: usize) {
        let [madr, bcr, chcr] = self.dma[ch];
        let words = match (chcr >> 9) & 3 {
            0 => {
                let n = bcr & 0xffff;
                if n == 0 { 0x10000 } else { n }
            }
            _ => (bcr & 0xffff) * (bcr >> 16).max(1),
        };
        let from_ram = chcr & 1 != 0;
        match ch {
            2 => match (chcr >> 9) & 3 {
                2 => {
                    // Linked list: each node is a header (count << 24 | next)
                    // and that many GP0 words; 0xffffff ends the list.
                    // A node visited twice means a loop; the hardware would
                    // run forever, so the walk stops and says so.
                    let mut at = madr & 0x1f_fffc;
                    let mut seen = std::collections::HashSet::new();
                    loop {
                        if !seen.insert(at) {
                            tracing::warn!("GPU DMA list loops at {at:06x} (started at {madr:08x})");
                            break;
                        }
                        let h = word(ram, at);
                        for k in 0..h >> 24 {
                            self.gpu.gp0(word(ram, at + 4 + 4 * k));
                        }
                        if h & 0x80_0000 != 0 {
                            break;
                        }
                        at = h & 0x1f_fffc;
                    }
                }
                _ if from_ram => {
                    for k in 0..words {
                        self.gpu.gp0(word(ram, madr + 4 * k));
                    }
                }
                _ => {
                    for k in 0..words {
                        let v = self.gpu.read();
                        set_word(ram, madr + 4 * k, v);
                    }
                }
            },
            4 => {
                if from_ram {
                    for k in 0..words {
                        let v = word(ram, madr + 4 * k);
                        for b in v.to_le_bytes() {
                            self.spu_ram[self.spu_addr % SPU_RAM] = b;
                            self.spu_addr += 1;
                        }
                    }
                }
            }
            6 => {
                // The ordering table, cleared backwards: each entry points at
                // the one before it, the first ends the list.
                let n = if bcr & 0xffff == 0 { 0x10000 } else { bcr & 0xffff };
                let mut at = madr & 0x1f_fffc;
                for k in 0..n {
                    let v = if k == n - 1 { 0x00ff_ffff } else { (at.wrapping_sub(4)) & 0x1f_ffff };
                    set_word(ram, at, v);
                    at = at.wrapping_sub(4);
                }
            }
            _ => {}
        }
        self.dma[ch][2] &= !(1 << 24 | 1 << 28);
        if self.dicr & (1 << (16 + ch)) != 0 {
            self.dicr |= 1 << (24 + ch);
        }
        if ch == 2 {
            // The transfer is done at once, but reads as busy for a while and
            // finishes with an interrupt, as libgpu expects: it calls the
            // draw-complete callback from that interrupt, never from inside
            // the call that queued the drawing.
            let now = self.clock.get();
            self.gpu_busy_until = now + GPU_DMA_TIME;
            self.pending.borrow_mut().push((self.gpu_busy_until, GPU_QUEUE));
        }
    }
}

/// The device the bus calls; shares its state with the host.
pub struct HwDevice(pub Rc<RefCell<Hw>>);

impl Device for HwDevice {
    fn read(&mut self, _ram: &mut [u8], phys: u32, width: u8) -> u32 {
        let mut hw = self.0.borrow_mut();
        match phys {
            0x1f80_1070 => 0,
            0x1f80_1074 => hw.irq_mask,
            0x1f80_1080..0x1f80_10f0 => {
                let ch = ((phys - 0x1f80_1080) / 16) as usize;
                let reg = ((phys & 0xf) / 4) as usize % 3;
                let v = hw.dma[ch][reg];
                if ch == 2 && reg == 2 && hw.clock.get() < hw.gpu_busy_until { v | 1 << 24 } else { v }
            }
            0x1f80_10f0 => hw.dpcr,
            0x1f80_10f4 => hw.dicr,
            0x1f80_1100..0x1f80_1130 => {
                // Root counters: a free-running value, so polling ends.
                hw.counter = hw.counter.wrapping_add(37);
                hw.counter & 0xffff
            }
            0x1f80_1810 => hw.gpu.read(),
            0x1f80_1814 => hw.gpu.status(),
            0x1f80_1c00..0x1f80_2000 => {
                let r = ((phys - 0x1f80_1c00) / 2) as usize;
                let v = if phys == 0x1f80_1dae {
                    // SPUSTAT mirrors the mode bits of SPUCNT once applied.
                    hw.spu_regs[(0x1daa - 0x1c00) / 2] & 0x3f
                } else {
                    hw.spu_regs[r]
                };
                if width == 4 { v as u32 | (hw.spu_regs[(r + 1) % 512] as u32) << 16 } else { v as u32 }
            }
            _ => {
                *hw.unhandled.entry(phys).or_default() += 1;
                0
            }
        }
    }

    fn write(&mut self, ram: &mut [u8], phys: u32, width: u8, value: u32) {
        let mut hw = self.0.borrow_mut();
        match phys {
            0x1f80_1070 => {}
            0x1f80_1074 => hw.irq_mask = value,
            0x1f80_1080..0x1f80_10f0 => {
                let ch = ((phys - 0x1f80_1080) / 16) as usize;
                let reg = ((phys & 0xf) / 4) as usize % 3;
                hw.dma[ch][reg] = value;
                if reg == 2 && value & (1 << 24) != 0 {
                    hw.start_dma(ram, ch);
                }
            }
            0x1f80_10f0 => hw.dpcr = value,
            0x1f80_10f4 => {
                // Writing a 1 to a flag bit acknowledges it.
                let flags = (hw.dicr & !(value & 0x7f00_0000)) & 0x7f00_0000;
                hw.dicr = (value & 0x00ff_ffff) | flags;
            }
            0x1f80_1100..0x1f80_1130 => {}
            // Memory control (bus delays) and RAM size: nothing to model.
            0x1f80_1000..0x1f80_1024 | 0x1f80_1060 => {}
            0x1f80_1810 => hw.gpu.gp0(value),
            0x1f80_1814 => hw.gpu.gp1(value),
            0x1f80_1c00..0x1f80_2000 => {
                let r = ((phys - 0x1f80_1c00) / 2) as usize;
                hw.spu_regs[r] = value as u16;
                if width == 4 {
                    hw.spu_regs[(r + 1) % 512] = (value >> 16) as u16;
                }
                match phys {
                    // The transfer address, in units of 8 bytes.
                    0x1f80_1da6 => hw.spu_addr = (value as u16 as usize) * 8,
                    // A halfword through the data port.
                    0x1f80_1da8 => {
                        let a = hw.spu_addr % SPU_RAM;
                        hw.spu_ram[a] = value as u8;
                        hw.spu_ram[(a + 1) % SPU_RAM] = (value >> 8) as u8;
                        hw.spu_addr += 2;
                    }
                    _ => {}
                }
            }
            _ => {
                *hw.unhandled.entry(phys).or_default() += 1;
            }
        }
    }
}
