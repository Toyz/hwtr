//! Memory as the R3000A sees it: 2 MB of RAM mirrored through the first 8 MB,
//! the 1 KB scratchpad, and the I/O ports, which are recorded rather than
//! emulated.

pub const RAM_SIZE: usize = 2 * 1024 * 1024;
pub const SCRATCH_BASE: u32 = 0x1f80_0000;
pub const IO_BASE: u32 = 0x1f80_1000;
pub const IO_END: u32 = 0x1f80_3000;

/// One access to an I/O port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoAccess {
    /// Physical address.
    pub addr: u32,
    pub width: u8,
    /// The value written, or `None` for a read.
    pub write: Option<u32>,
}

pub struct Bus {
    pub ram: Vec<u8>,
    pub scratch: Vec<u8>,
    /// Every I/O access, in order.
    pub io: Vec<IoAccess>,
    /// Values I/O reads return, by physical address; 0 otherwise.
    pub io_values: std::collections::HashMap<u32, u32>,
}

impl Default for Bus {
    fn default() -> Self {
        Bus { ram: vec![0; RAM_SIZE], scratch: vec![0; 1024], io: Vec::new(), io_values: Default::default() }
    }
}

enum Region {
    Ram(usize),
    Scratch(usize),
    Io(u32),
    /// Cache control and anything else that ignores the access.
    Ignore,
}

fn region(addr: u32) -> Option<Region> {
    if addr >= 0xfffe_0000 {
        return Some(Region::Ignore);
    }
    let phys = addr & 0x1fff_ffff;
    match phys {
        0..0x0080_0000 => Some(Region::Ram(phys as usize & (RAM_SIZE - 1))),
        SCRATCH_BASE..0x1f80_0400 => Some(Region::Scratch((phys - SCRATCH_BASE) as usize)),
        IO_BASE..IO_END => Some(Region::Io(phys)),
        _ => None,
    }
}

impl Bus {
    /// Reads `width` (1, 2 or 4) bytes, little-endian. `None` is a bus error.
    pub fn read(&mut self, addr: u32, width: u8) -> Option<u32> {
        let n = width as usize;
        let get = |mem: &[u8], at: usize| {
            let mut v = 0u32;
            for (i, b) in mem[at..at + n].iter().enumerate() {
                v |= (*b as u32) << (8 * i);
            }
            v
        };
        Some(match region(addr)? {
            Region::Ram(at) => get(&self.ram, at),
            Region::Scratch(at) => get(&self.scratch, at),
            Region::Io(phys) => {
                self.io.push(IoAccess { addr: phys, width, write: None });
                self.io_values.get(&phys).copied().unwrap_or(0)
            }
            Region::Ignore => 0,
        })
    }

    pub fn write(&mut self, addr: u32, width: u8, value: u32) -> Option<()> {
        let n = width as usize;
        let put = |mem: &mut [u8], at: usize| {
            for i in 0..n {
                mem[at + i] = (value >> (8 * i)) as u8;
            }
        };
        match region(addr)? {
            Region::Ram(at) => put(&mut self.ram, at),
            Region::Scratch(at) => put(&mut self.scratch, at),
            Region::Io(phys) => self.io.push(IoAccess { addr: phys, width, write: Some(value) }),
            Region::Ignore => {}
        }
        Some(())
    }

    pub fn read_u32(&mut self, addr: u32) -> u32 {
        self.read(addr, 4).unwrap_or(0)
    }

    pub fn write_u32(&mut self, addr: u32, v: u32) {
        self.write(addr, 4, v);
    }

    /// Copies bytes into memory.
    pub fn load(&mut self, addr: u32, bytes: &[u8]) {
        for (i, &b) in bytes.iter().enumerate() {
            self.write(addr + i as u32, 1, b as u32);
        }
    }

    /// Reads `len` bytes.
    pub fn bytes(&mut self, addr: u32, len: usize) -> Vec<u8> {
        (0..len).map(|i| self.read(addr + i as u32, 1).unwrap_or(0) as u8).collect()
    }

    /// A NUL-terminated string, up to 4096 bytes.
    pub fn cstr(&mut self, addr: u32) -> String {
        let mut out = Vec::new();
        for i in 0..4096 {
            match self.read(addr + i, 1) {
                Some(0) | None => break,
                Some(b) => out.push(b as u8),
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}
