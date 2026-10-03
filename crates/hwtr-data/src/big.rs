//! The BIG archive: CCCPSX.BIG, and the BIG archives nested inside it.
//!
//! ```text
//! u32           count
//! entry[count]  24 bytes each
//!   char[12]    name     8.3 with the dot dropped, NUL padded: "SCREENSBIG"
//!   u32         offset   from the start of this archive
//!   u32         size     bytes
//!   u32         sum      wrapping sum of the member's little-endian u32 words,
//!                        then of its trailing 1-3 bytes one at a time
//! ```

use std::fmt;

#[derive(Clone, Debug)]
pub struct Member {
    /// The name as stored, without the padding.
    pub name: String,
    /// Offset from the start of the archive that holds it.
    pub offset: u32,
    pub size: u32,
    pub sum: u32,
}

#[derive(Debug)]
pub struct BigError(pub String);

impl fmt::Display for BigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BIG: {}", self.0)
    }
}

impl std::error::Error for BigError {}

pub struct Big<'a> {
    pub bytes: &'a [u8],
    pub members: Vec<Member>,
}

/// The checksum the archive stores: whole little-endian words summed, then
/// any trailing bytes added one byte at a time.
pub fn checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    let mut words = data.chunks_exact(4);
    for w in &mut words {
        sum = sum.wrapping_add(u32::from_le_bytes(w.try_into().unwrap()));
    }
    for &b in words.remainder() {
        sum = sum.wrapping_add(b as u32);
    }
    sum
}

impl<'a> Big<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Big<'a>, BigError> {
        let count =
            bytes.get(..4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or(BigError("empty".into()))?;
        let table_end = 4 + count as usize * 24;
        if count == 0 || table_end > bytes.len() {
            return Err(BigError(format!("count {count} does not fit {} bytes", bytes.len())));
        }
        let mut members = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let e = &bytes[4 + i * 24..4 + i * 24 + 24];
            let name_end = e[..12].iter().position(|&c| c == 0).unwrap_or(12);
            let word = |at: usize| u32::from_le_bytes(e[at..at + 4].try_into().unwrap());
            let m = Member {
                name: String::from_utf8_lossy(&e[..name_end]).to_string(),
                offset: word(12),
                size: word(16),
                sum: word(20),
            };
            if (m.offset as usize) < table_end || m.offset as usize + m.size as usize > bytes.len() {
                return Err(BigError(format!("{} at {:#x}+{:#x} is outside the archive", m.name, m.offset, m.size)));
            }
            members.push(m);
        }
        Ok(Big { bytes, members })
    }

    pub fn data(&self, m: &Member) -> &'a [u8] {
        &self.bytes[m.offset as usize..m.offset as usize + m.size as usize]
    }

    pub fn find(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// Looks a member up by a `/`-separated path through nested archives:
    /// `DESERT1BIG/ACTNFNTOVL`.
    pub fn lookup(&self, path: &str) -> Option<&'a [u8]> {
        let (head, rest) = match path.split_once('/') {
            Some((h, r)) => (h, Some(r)),
            None => (path, None),
        };
        let data = self.data(self.find(head)?);
        match rest {
            None => Some(data),
            Some(rest) => Big::parse(data).ok()?.lookup(rest),
        }
    }

    pub fn is_nested(m: &Member) -> bool {
        m.name.ends_with("BIG")
    }

    /// Every member, depth first, with its path through the nested archives.
    pub fn walk(&self) -> Result<Vec<(String, Member, &'a [u8])>, BigError> {
        let mut out = Vec::new();
        self.walk_into("", &mut out)?;
        Ok(out)
    }

    fn walk_into(&self, prefix: &str, out: &mut Vec<(String, Member, &'a [u8])>) -> Result<(), BigError> {
        for m in &self.members {
            let path = format!("{prefix}{}", m.name);
            let data = self.data(m);
            out.push((path.clone(), m.clone(), data));
            if Big::is_nested(m) {
                Big::parse(data)
                    .map_err(|e| BigError(format!("{path}: {}", e.0)))?
                    .walk_into(&format!("{path}/"), out)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_sum() {
        let payload = b"abcdefg";
        let mut bytes = vec![1, 0, 0, 0];
        let mut name = [0u8; 12];
        name[..6].copy_from_slice(b"FOOBIN");
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(&28u32.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&checksum(payload).to_le_bytes());
        bytes.extend_from_slice(payload);
        let big = Big::parse(&bytes).unwrap();
        assert_eq!(big.members[0].name, "FOOBIN");
        assert_eq!(big.lookup("FOOBIN"), Some(&payload[..]));
        assert_eq!(checksum(payload), u32::from_le_bytes(*b"abcd") + (b'e' + b'f') as u32 + b'g' as u32);
    }
}
