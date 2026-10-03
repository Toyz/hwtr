//! ISO 9660 with the CD-XA system use field, as the PlayStation mastering
//! tools wrote it.

use crate::{Disc, Error, FORM1_DATA, Result};

/// The CD-XA attribute bits, from the big-endian word in the system use field.
pub mod xa {
    pub const FORM1: u16 = 0x0800;
    pub const FORM2: u16 = 0x1000;
    pub const INTERLEAVED: u16 = 0x2000;
    pub const CDDA: u16 = 0x4000;
    pub const DIRECTORY: u16 = 0x8000;
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    /// The full path from the root, `/`-separated, without the `;1` version.
    pub path: String,
    pub lba: u32,
    /// The recorded size in bytes. For Form 2 files this counts 2048 per sector.
    pub size: u32,
    pub is_dir: bool,
    /// Recording date: year, month, day, hour, minute, second, GMT offset.
    pub date: [u8; 7],
    /// The CD-XA attributes, if the record carries the XA system use field.
    pub xa_attr: Option<u16>,
    /// The CD-XA file number.
    pub xa_file: Option<u8>,
}

impl DirEntry {
    pub fn sectors(&self) -> u32 {
        self.size.div_ceil(FORM1_DATA as u32)
    }

    /// True when the directory record says the file holds Form 2 sectors
    /// (XA audio or STR video).
    pub fn is_form2(&self) -> bool {
        self.xa_attr.is_some_and(|a| a & (xa::FORM2 | xa::INTERLEAVED) != 0)
    }
}

#[derive(Clone, Debug)]
pub struct Pvd {
    pub system_id: String,
    pub volume_id: String,
    pub volume_blocks: u32,
    pub publisher: String,
    pub preparer: String,
    pub application: String,
    pub created: String,
    pub root_lba: u32,
    pub root_size: u32,
}

pub struct Iso<'a> {
    pub disc: &'a Disc,
    pub pvd: Pvd,
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_end().to_string()
}

impl<'a> Iso<'a> {
    pub fn open(disc: &'a Disc) -> Result<Iso<'a>> {
        let block = disc.sector(16)?.block();
        if block[0] != 1 || &block[1..6] != b"CD001" {
            return Err(Error::Iso("sector 16 is not a primary volume descriptor".into()));
        }
        let root = &block[156..156 + 34];
        let pvd = Pvd {
            system_id: text(&block[8..40]),
            volume_id: text(&block[40..72]),
            volume_blocks: le32(block, 80),
            publisher: text(&block[318..446]),
            preparer: text(&block[446..574]),
            application: text(&block[574..702]),
            created: text(&block[813..830]),
            root_lba: le32(root, 2),
            root_size: le32(root, 10),
        };
        Ok(Iso { disc, pvd })
    }

    /// Every file and directory, depth first, directories before their contents.
    pub fn walk(&self) -> Result<Vec<DirEntry>> {
        let mut out = Vec::new();
        self.walk_dir(self.pvd.root_lba, self.pvd.root_size, "", &mut out, 0)?;
        Ok(out)
    }

    fn walk_dir(&self, lba: u32, size: u32, prefix: &str, out: &mut Vec<DirEntry>, depth: u32) -> Result<()> {
        if depth > 16 {
            return Err(Error::Iso(format!("directory nesting too deep at {prefix}")));
        }
        let data = self.disc.read_blocks(lba, size)?;
        let mut children = Vec::new();
        for chunk in data.chunks(FORM1_DATA) {
            let mut at = 0;
            while at < chunk.len() {
                let len = chunk[at] as usize;
                if len == 0 {
                    break;
                }
                let rec = chunk
                    .get(at..at + len)
                    .ok_or_else(|| Error::Iso(format!("record overruns its sector in {prefix}/")))?;
                at += len;
                let name_len = rec[32] as usize;
                let name = &rec[33..33 + name_len];
                if name == [0] || name == [1] {
                    continue;
                }
                let mut name = String::from_utf8_lossy(name).to_string();
                if let Some(semi) = name.find(';') {
                    name.truncate(semi);
                }
                let mut su = 33 + name_len;
                if su % 2 == 1 {
                    su += 1;
                }
                let (xa_attr, xa_file) = match rec.get(su..su + 14) {
                    Some(f) if &f[6..8] == b"XA" => (Some(u16::from_be_bytes([f[4], f[5]])), Some(f[8])),
                    _ => (None, None),
                };
                let entry = DirEntry {
                    path: format!("{prefix}/{name}"),
                    lba: le32(rec, 2),
                    size: le32(rec, 10),
                    is_dir: rec[25] & 2 != 0,
                    date: rec[18..25].try_into().unwrap(),
                    xa_attr,
                    xa_file,
                };
                children.push(entry);
            }
        }
        for child in children {
            let (is_dir, lba, size, path) = (child.is_dir, child.lba, child.size, child.path.clone());
            out.push(child);
            if is_dir {
                self.walk_dir(lba, size, &path, out, depth + 1)?;
            }
        }
        Ok(())
    }

    /// Finds a file by path, case-insensitively, with or without a leading `/`.
    pub fn find(&self, path: &str) -> Result<DirEntry> {
        let want = format!("/{}", path.trim_start_matches('/').trim_end_matches(";1"));
        self.walk()?
            .into_iter()
            .find(|e| e.path.eq_ignore_ascii_case(&want))
            .ok_or_else(|| Error::Iso(format!("no {want} on the disc")))
    }

    /// Reads a file's Form 1 contents.
    pub fn read(&self, entry: &DirEntry) -> Result<Vec<u8>> {
        self.disc.read_blocks(entry.lba, entry.size)
    }
}
