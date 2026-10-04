//! The Hot Wheels Turbo Racing CD, read from a CUE sheet and its BIN files.
//!
//! The disc is one Mode 2 data track followed by Red Book audio tracks. The
//! data track is stored as raw 2352-byte sectors, so every sector carries its
//! sync, header and CD-XA subheader; [`Sector`] exposes them. The file system on
//! the data track is ISO 9660 with the CD-XA extension ([`iso`]).

#![forbid(unsafe_code)]

pub mod cue;
pub mod iso;

use std::fmt;
use std::path::{Path, PathBuf};

pub use cue::{Cue, Track, TrackKind};
pub use iso::{DirEntry, Iso};

/// Bytes in a raw CD sector.
pub const RAW_SECTOR: usize = 2352;
/// User data bytes in a Mode 2 Form 1 sector (and in an ISO 9660 logical block).
pub const FORM1_DATA: usize = 2048;
/// User data bytes in a Mode 2 Form 2 sector.
pub const FORM2_DATA: usize = 2324;
/// Sectors per second at 1x; the 150-sector lead-in pregap is not in the BIN.
pub const SECTORS_PER_SECOND: u32 = 75;

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    Cue(String),
    Iso(String),
    OutOfRange(u32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Error::Cue(msg) => write!(f, "cue sheet: {msg}"),
            Error::Iso(msg) => write!(f, "iso 9660: {msg}"),
            Error::OutOfRange(lba) => write!(f, "sector {lba} is past the end of the data track"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// One raw sector of the data track.
#[derive(Clone, Copy)]
pub struct Sector<'a> {
    pub raw: &'a [u8],
}

/// The CD-XA subheader's submode bits.
pub mod submode {
    pub const EOR: u8 = 0x01;
    pub const VIDEO: u8 = 0x02;
    pub const AUDIO: u8 = 0x04;
    pub const DATA: u8 = 0x08;
    pub const TRIGGER: u8 = 0x10;
    pub const FORM2: u8 = 0x20;
    pub const REALTIME: u8 = 0x40;
    pub const EOF: u8 = 0x80;
}

impl<'a> Sector<'a> {
    /// The header's mode byte: 2 on every sector of this disc's data track.
    pub fn mode(&self) -> u8 {
        self.raw[15]
    }

    /// The header's address, as minutes, seconds and frames (decoded from BCD).
    pub fn msf(&self) -> (u8, u8, u8) {
        let bcd = |b: u8| (b >> 4) * 10 + (b & 15);
        (bcd(self.raw[12]), bcd(self.raw[13]), bcd(self.raw[14]))
    }

    /// The CD-XA subheader: file number, channel, submode, coding info.
    pub fn subheader(&self) -> [u8; 4] {
        [self.raw[16], self.raw[17], self.raw[18], self.raw[19]]
    }

    pub fn file(&self) -> u8 {
        self.raw[16]
    }

    pub fn channel(&self) -> u8 {
        self.raw[17]
    }

    pub fn submode(&self) -> u8 {
        self.raw[18]
    }

    pub fn coding(&self) -> u8 {
        self.raw[19]
    }

    pub fn is_form2(&self) -> bool {
        self.mode() == 2 && self.submode() & submode::FORM2 != 0
    }

    /// The user data: 2048 bytes for Form 1, 2324 for Form 2.
    pub fn data(&self) -> &'a [u8] {
        match self.mode() {
            1 => &self.raw[16..16 + FORM1_DATA],
            _ if self.is_form2() => &self.raw[24..24 + FORM2_DATA],
            _ => &self.raw[24..24 + FORM1_DATA],
        }
    }

    /// Form 1 user data regardless of the submode, as an ISO 9660 reader sees it.
    pub fn block(&self) -> &'a [u8] {
        match self.mode() {
            1 => &self.raw[16..16 + FORM1_DATA],
            _ => &self.raw[24..24 + FORM1_DATA],
        }
    }
}

/// The whole disc: the CUE sheet and the data track held in memory.
pub struct Disc {
    pub cue: Cue,
    data: Vec<u8>,
}

impl Disc {
    /// Opens a disc from its CUE sheet. The first track must be the data track.
    pub fn open(cue_path: &Path) -> Result<Disc> {
        let cue = Cue::load(cue_path)?;
        let first = cue.tracks.first().ok_or_else(|| Error::Cue("no tracks".into()))?;
        if first.kind != TrackKind::Mode2Raw && first.kind != TrackKind::Mode1Raw {
            return Err(Error::Cue(format!("track 1 is {:?}, not a raw data track", first.kind)));
        }
        let data = std::fs::read(&first.file).map_err(|e| Error::Io(first.file.clone(), e))?;
        if data.len() % RAW_SECTOR != 0 {
            return Err(Error::Cue(format!("{} is not a whole number of sectors", first.file.display())));
        }
        Ok(Disc { cue, data })
    }

    /// Finds the one CUE sheet in a directory.
    pub fn find_cue(dir: &Path) -> Result<PathBuf> {
        let entries = std::fs::read_dir(dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
        let mut found: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cue")))
            .collect();
        found.sort();
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(Error::Cue(format!("no .cue in {}", dir.display()))),
            _ => Err(Error::Cue(format!("several .cue files in {}", dir.display()))),
        }
    }

    /// Sectors on the data track.
    pub fn sectors(&self) -> u32 {
        (self.data.len() / RAW_SECTOR) as u32
    }

    pub fn sector(&self, lba: u32) -> Result<Sector<'_>> {
        let start = lba as usize * RAW_SECTOR;
        let raw = self.data.get(start..start + RAW_SECTOR).ok_or(Error::OutOfRange(lba))?;
        Ok(Sector { raw })
    }

    /// The ISO 9660 file system on the data track.
    pub fn iso(&self) -> Result<Iso<'_>> {
        Iso::open(self)
    }

    /// Reads `size` bytes of Form 1 user data starting at `lba`.
    pub fn read_blocks(&self, lba: u32, size: u32) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(size as usize);
        let count = (size as usize).div_ceil(FORM1_DATA) as u32;
        for i in 0..count {
            out.extend_from_slice(self.sector(lba + i)?.block());
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// The raw 2352-byte sectors of `count` sectors starting at `lba`.
    pub fn read_raw(&self, lba: u32, count: u32) -> Result<&[u8]> {
        let start = lba as usize * RAW_SECTOR;
        let end = start + count as usize * RAW_SECTOR;
        self.data.get(start..end).ok_or(Error::OutOfRange(lba + count - 1))
    }
}
