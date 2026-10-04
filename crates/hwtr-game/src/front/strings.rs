//! The front end's words: `ENGLISH.HWT` (0x8007fbcc), 300 lines of text,
//! and `ENGCARS.CDT` (0x80088204), five lines about each of the 41 cars.

/// The string table: line `k` of the file, blank lines skipped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Strings(Vec<String>);

pub const STRING_COUNT: usize = 300;

impl Strings {
    /// Line ends become string ends; each string starts at the next
    /// non-empty text. A file of more than 6 KiB is refused, as the
    /// original's buffer is that big.
    pub fn parse(bytes: &[u8]) -> Option<Strings> {
        if bytes.is_empty() || bytes.len() > 0x1800 {
            return None;
        }
        let b: Vec<u8> = bytes.iter().map(|&c| if c == b'\n' || c == b'\r' { 0 } else { c }).collect();
        let at = |p: usize| b.get(p).copied().unwrap_or(0);
        let mut p = 0;
        let mut out = Vec::with_capacity(STRING_COUNT);
        for k in 0..STRING_COUNT {
            let start = p;
            while at(p) != 0 {
                p += 1;
            }
            out.push(b.get(start..p.min(b.len())).map(|s| s.iter().map(|&c| c as char).collect()).unwrap_or_default());
            if k < STRING_COUNT - 1 {
                while p < b.len() && at(p) == 0 {
                    p += 1;
                }
            }
        }
        Some(Strings(out))
    }

    /// String `k`, or nothing past the table.
    pub fn get(&self, k: usize) -> &str {
        self.0.get(k).map_or("", |s| s.as_str())
    }
}

/// The car facts: 41 cars of five fields, each at most 20 bytes, fields
/// ending at `|`, line ends skipped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CarFacts(pub Vec<[String; 5]>);

pub const CARS: usize = 41;

impl CarFacts {
    pub fn parse(bytes: &[u8]) -> CarFacts {
        let mut it = bytes.iter().copied();
        let cars = (0..CARS)
            .map(|_| {
                std::array::from_fn(|_| {
                    // A field stops at `|` or when 20 bytes are written; a
                    // field that long runs on into the next.
                    let mut field = Vec::new();
                    let mut written = 0;
                    while written < 20 {
                        match it.next() {
                            Some(b'|') => break,
                            Some(b'\r' | b'\n') => continue,
                            Some(c) => {
                                field.push(c);
                                written += 1;
                            }
                            None => break,
                        }
                    }
                    field.iter().map(|&c| c as char).collect()
                })
            })
            .collect();
        CarFacts(cars)
    }
}
