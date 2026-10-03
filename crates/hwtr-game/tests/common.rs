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
