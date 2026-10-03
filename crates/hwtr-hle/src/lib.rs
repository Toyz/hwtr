//! The original game, run in `hwtr-cpu`.
//!
//! The executable runs as it is. What a console would do in hardware or the
//! kernel is supplied: the BIOS shim from `hwtr-cpu`, the hardware model of
//! [`hw`] (GPU, DMA, SPU), and these PsyQ library calls replaced at their
//! entry points, because the hardware behind them is interrupt-driven:
//!
//! ```text
//! 0x800a4d18  CdInit
//! 0x800a4e18  CdSearchFile(CdlFILE *fp, char *name)
//! 0x800a589c  CdControl(u8 com, u8 *param, u8 *result)
//! 0x800a5b0c  CdControlB(u8 com, u8 *param, u8 *result)
//! 0x800a4b18  CdRead(int sectors, u32 *buf, int mode)
//! 0x800a4c18  CdReadSync(int mode, u8 *result)
//! 0x800a38cc  VSync(int mode)          one call = one frame; the run halts there
//! ```

pub mod gpu;
pub mod hw;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hwtr_cpu::{Fault, Machine};
use hwtr_disc::Disc;

pub const CD_INIT: u32 = 0x800a_4d18;
pub const CD_SEARCH_FILE: u32 = 0x800a_4e18;
pub const CD_CONTROL: u32 = 0x800a_589c;
pub const CD_CONTROL_B: u32 = 0x800a_5b0c;
pub const CD_READ: u32 = 0x800a_4b18;
pub const CD_READ_SYNC: u32 = 0x800a_4c18;
pub const VSYNC: u32 = 0x800a_38cc;
/// libetc's VSyncCallback(func): the game's vertical blank handler.
pub const VSYNC_CALLBACK: u32 = 0x8009_fdb8;
/// libetc's vertical blank counter.
pub const VCOUNT: u32 = 0x800c_86b4;
/// libmcrd: MemCardAccept(chan) starts checking a card; MemCardSync(mode,
/// int *cmd, int *result) reports -1 nothing pending, 0 running, 1 done.
/// libpad's receive buffers, 34 bytes per port, as controls_init hands them
/// to PadInitDirect.
pub const PAD_BUFFER: u32 = 0x8011_b388;
/// The game's printf: its debug output, logged instead of printed.
pub const PRINTF: u32 = 0x800a_2d28;
pub const MEMCARD_ACCEPT: u32 = 0x800a_8778;
pub const MEMCARD_SYNC: u32 = 0x800a_9184;
/// libmcrd command starters, by the guard messages they print.
pub const MEMCARD_EXIST: u32 = 0x800a_8524;
pub const MEMCARD_NO_CARD: [u32; 5] = [0x800a_8f1c, 0x800a_92a0, 0x800a_89dc, 0x800a_8c7c, 0x800a_94a8];

pub struct Hle {
    pub m: Machine,
    /// Port 1's buttons, PlayStation bit order, 1 = pressed; written into
    /// libpad's receive buffer every frame (port 2 reads as empty).
    pub pad: u16,
    pub hw: Rc<RefCell<hw::Hw>>,
    pub frames: Rc<Cell<u64>>,
    /// Files the game read, in order: (lba, sectors).
    pub reads: Rc<RefCell<Vec<(u32, u32)>>>,
}

fn bcd(v: u32) -> u8 {
    ((v / 10) << 4 | (v % 10)) as u8
}

fn from_bcd(b: u8) -> u32 {
    (b >> 4) as u32 * 10 + (b & 15) as u32
}

impl Hle {
    /// Loads `CCCPSX.EXE` from the disc and installs the hardware and hooks.
    pub fn new(disc: Rc<Disc>) -> Result<Hle, String> {
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let exe = iso.find("CCCPSX.EXE").and_then(|e| iso.read(&e)).map_err(|e| e.to_string())?;
        let exe = hwtr_psx::Exe::parse(&exe).map_err(|e| e.to_string())?;
        let mut m = Machine::with_exe(&exe);
        m.step_limit = u64::MAX;
        let hw = Rc::new(RefCell::new(hw::Hw::default()));
        hw.borrow_mut().clock = m.clock.clone();
        hw.borrow_mut().pending = m.pending.clone();
        m.bus.device = Some(Box::new(hw::HwDevice(hw.clone())));
        // The files on the disc, by name, for CdSearchFile.
        let files: Vec<(String, u32, u32)> = iso
            .walk()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|e| !e.is_dir)
            .map(|e| (e.path.trim_start_matches('/').to_uppercase(), e.lba, e.size))
            .collect();
        let pos = Rc::new(Cell::new(0u32));
        let reads: Rc<RefCell<Vec<(u32, u32)>>> = Default::default();

        m.stub(CD_INIT, 1);
        m.hook(CD_SEARCH_FILE, move |c, b| {
            let (fp, name) = (c.r[4], b.cstr(c.r[5]));
            let want = name.trim_start_matches('\\').split(';').next().unwrap_or("").to_uppercase();
            let Some((n, lba, size)) = files.iter().find(|f| f.0 == want) else {
                tracing::warn!("CdSearchFile: no {name}");
                return 0;
            };
            let l = lba + 150;
            b.load(fp, &[bcd(l / 75 / 60), bcd(l / 75 % 60), bcd(l % 75), 0]);
            b.write_u32(fp + 4, *size);
            let mut nm = n.clone().into_bytes();
            nm.resize(16, 0);
            b.load(fp + 8, &nm);
            fp
        });
        for addr in [CD_CONTROL, CD_CONTROL_B] {
            let pos = pos.clone();
            m.hook(addr, move |c, b| {
                let (com, param, result) = (c.r[4] & 0xff, c.r[5], c.r[6]);
                // Setloc, and the reads that take a position.
                if matches!(com, 0x02 | 0x06 | 0x1b) && param != 0 {
                    let p = b.bytes(param, 3);
                    pos.set((from_bcd(p[0]) * 60 + from_bcd(p[1])) * 75 + from_bcd(p[2]) - 150);
                }
                if result != 0 {
                    // Status: motor on, reading if a read was asked for.
                    b.load(result, &[if matches!(com, 0x06 | 0x1b) { 0x22 } else { 0x02 }]);
                }
                1
            });
        }
        {
            let pos = pos.clone();
            let reads = reads.clone();
            let disc = disc.clone();
            m.hook(CD_READ, move |c, b| {
                let (sectors, buf) = (c.r[4], c.r[5]);
                tracing::debug!("CdRead {sectors} sectors at {} -> {buf:08x}, mode {:x}", pos.get(), c.r[6]);
                match disc.read_blocks(pos.get(), sectors * 2048) {
                    Ok(data) => b.load(buf, &data),
                    Err(e) => tracing::error!("CdRead at {}: {e}", pos.get()),
                }
                reads.borrow_mut().push((pos.get(), sectors));
                pos.set(pos.get() + sectors);
                1
            });
        }
        m.stub(CD_READ_SYNC, 0);
        if std::env::var_os("HWTR_IRQLOG").is_some() {
            for f in [0x800a_0234u32, 0x800a_a120, 0x800a_02d4, 0x800a_00ec, 0x8009_fe44, 0x800a_a348] {
                m.hook(f, move |c, _| {
                    tracing::info!("irq setter {f:08x}({:x}, {:08x}) from {:08x}", c.r[4], c.r[5], c.r[31]);
                    0
                });
            }
        }
        m.hook(PRINTF, |c, b| {
            let f = b.cstr(c.r[4]);
            // Fill %d, %x and %s from a1-a3; the rest stay as written.
            let mut args = [c.r[5], c.r[6], c.r[7]].into_iter();
            let mut out = String::new();
            let mut chars = f.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch != '%' {
                    out.push(ch);
                    continue;
                }
                let mut spec = String::new();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n.is_ascii_alphabetic() || n == '%' {
                        spec.push(n);
                        break;
                    }
                }
                match (spec.chars().last(), spec.as_str()) {
                    (Some('%'), _) => out.push('%'),
                    (Some('s'), _) => out.push_str(&args.next().map(|a| b.cstr(a)).unwrap_or_default()),
                    (Some('x' | 'X'), _) => out.push_str(&format!("{:x}", args.next().unwrap_or(0))),
                    (Some(_), _) => out.push_str(&format!("{}", args.next().unwrap_or(0) as i32)),
                    (None, _) => {}
                }
            }
            tracing::debug!(target: "hwtr_hle::printf", "{}", out.trim_end());
            out.len() as u32
        });
        // No memory card in either slot: every check finishes at once with
        // McErrCardNotExist (1).
        m.stub(MEMCARD_ACCEPT, 1);
        // The other libmcrd commands: the check starter is accepted; the
        // directory, file and format commands report McErrCardNotExist.
        m.stub(MEMCARD_EXIST, 1);
        for f in MEMCARD_NO_CARD {
            m.stub(f, 1);
        }
        m.hook(MEMCARD_SYNC, |c, b| {
            if c.r[6] != 0 {
                b.write_u32(c.r[6], 1);
            }
            1
        });
        // Vertical blanks come from the machine every ~560,000 instructions
        // (33.87 MHz at 60 Hz, a little under one instruction per cycle).
        m.vblank.period = 560_000;
        m.vblank.counter = VCOUNT;
        // libetc's dispatcher calls the callbacks at 0x8009fff0.
        m.vblank.return_to = 0x8009_fff8;
        // Handlers run on their own stack in the kernel's low 64 KB, which the
        // game never touches, as the BIOS's exception stack would.
        m.vblank.stack = 0x8000_fff0;
        {
            let h = m.vblank.handler.clone();
            m.hook(VSYNC_CALLBACK, move |c, _| h.replace(c.r[4]));
        }
        {
            let skip = m.vblank.skip.clone();
            m.hook(VSYNC, move |c, b| {
                let mode = c.r[4] as i32;
                let count = b.read_u32(VCOUNT);
                if mode < 0 {
                    return count;
                }
                if mode == 1 {
                    return 0;
                }
                // Waiting: the next n blanks arrive right away.
                let n = mode.max(1) as u32;
                tracing::trace!("VSync({mode}) from {:08x}", c.r[31]);
                skip.set(skip.get() + n);
                263 * n
            });
        }
        let frames = m.vblank.count.clone();
        let mut hle = Hle { m, pad: 0, hw, frames, reads };
        hle.m.cpu.jump(exe.pc0);
        hle.m.cpu.r[29] = 0x801f_ff00;
        hle.m.cpu.r[31] = hwtr_cpu::machine::RETURN;
        Ok(hle)
    }

    /// Runs to the next vertical blank (one frame), or a fault.
    pub fn frame(&mut self) -> Result<(), Fault> {
        // libpad's buffers as its interrupt routine leaves them: status 0,
        // a digital pad (0x41), the buttons inverted; port 2 not connected.
        let b = !self.pad;
        self.m.bus.load(PAD_BUFFER, &[0x00, 0x41, b as u8, (b >> 8) as u8]);
        self.m.bus.load(PAD_BUFFER + 34, &[0xff, 0xff, 0xff, 0xff]);
        match self.m.run() {
            Err(Fault::Halt { .. }) => Ok(()),
            Ok(_) => Err(Fault::Halt { pc: hwtr_cpu::machine::RETURN }),
            Err(f) => Err(f),
        }
    }
}
