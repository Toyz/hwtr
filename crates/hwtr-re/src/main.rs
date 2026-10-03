//! hwtr-re: the reverse engineering toolbox for Hot Wheels Turbo Racing.

mod docs;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use hwtr_disc::{Disc, TrackKind};
use hwtr_psx::Exe;
use hwtr_psx::mips;

const USAGE: &str = "usage: hwtr-re <command> ...

  disc info                 tracks, volume descriptor, sector kinds
  disc ls                   every file: LBA, size, XA attributes
  disc extract [DIR]        files to DIR (default work/fs); Form 2 files as raw 2352-byte sectors
  disc cat PATH             one file's Form 1 contents to stdout
  disc audio [DIR]          CD-DA tracks as WAV (default work/audio)
  big ls FILE               every member of a BIG archive, nested ones too, checksums verified
  big extract FILE [DIR]    members to DIR (default work/big)
  exe info FILE             PS-X EXE header
  disasm FILE [--from ADDR] [--to ADDR] [--count N]
                            disassemble a PS-X EXE (or a raw image with --base ADDR),
                            with function labels, BIOS calls, and the strings the code points at
  funcs FILE                function starts found, with size and caller count
  strings FILE              strings the code references, with the functions that use them

Names in symbols/<stem>.txt (`0xADDRESS name` per line) override the found ones.
  docs index|check          the reference index under docs/

FILE may be `disc:PATH` to read straight off the CD. The disc is the one .cue
under work/disc unless --cue is given.";

type Result<T> = std::result::Result<T, String>;

/// The repository root: the nearest directory holding cairns.toml.
fn root() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    loop {
        if dir.join("cairns.toml").exists() {
            return dir;
        }
        if !dir.pop() {
            return std::env::current_dir().unwrap();
        }
    }
}

struct Args {
    items: Vec<String>,
}

impl Args {
    /// Removes `--name VALUE` and returns VALUE.
    fn take(&mut self, name: &str) -> Option<String> {
        let at = self.items.iter().position(|a| a == name)?;
        self.items.remove(at);
        (at < self.items.len()).then(|| self.items.remove(at))
    }
}

fn parse_num(text: &str) -> Result<u32> {
    let t = text.trim_start_matches("0x");
    if t.len() != text.len() || t.chars().any(|c| c.is_ascii_alphabetic()) {
        u32::from_str_radix(t, 16).map_err(|e| format!("{text}: {e}"))
    } else {
        t.parse().map_err(|e| format!("{text}: {e}"))
    }
}

fn open_disc(cue: &Option<String>) -> Result<Disc> {
    let path = match cue {
        Some(p) => PathBuf::from(p),
        None => Disc::find_cue(&root().join("work/disc")).map_err(|e| e.to_string())?,
    };
    Disc::open(&path).map_err(|e| e.to_string())
}

fn read_input(file: &str, cue: &Option<String>) -> Result<Vec<u8>> {
    if let Some(path) = file.strip_prefix("disc:") {
        let disc = open_disc(cue)?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let entry = iso.find(path).map_err(|e| e.to_string())?;
        iso.read(&entry).map_err(|e| e.to_string())
    } else {
        std::fs::read(file).map_err(|e| format!("{file}: {e}"))
    }
}

fn xa_text(attr: Option<u16>) -> String {
    use hwtr_disc::iso::xa;
    let Some(a) = attr else { return "-".into() };
    let mut parts = Vec::new();
    for (bit, name) in
        [(xa::FORM1, "f1"), (xa::FORM2, "f2"), (xa::INTERLEAVED, "il"), (xa::CDDA, "da"), (xa::DIRECTORY, "dir")]
    {
        if a & bit != 0 {
            parts.push(name);
        }
    }
    format!("{a:04x}:{}", parts.join("+"))
}

fn disc_cmd(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let sub = if args.items.is_empty() { String::new() } else { args.items.remove(0) };
    let disc = open_disc(cue)?;
    let iso = disc.iso().map_err(|e| e.to_string())?;
    match sub.as_str() {
        "info" => {
            println!("cue        {}", disc.cue.path.display());
            for t in &disc.cue.tracks {
                let sectors = t.file_sectors().map_err(|e| e.to_string())?;
                let secs = sectors as f64 / 75.0;
                println!(
                    "track {:02}   {:?}  {} sectors ({:.0}:{:05.2})  index01 at {}",
                    t.number,
                    t.kind,
                    sectors,
                    (secs / 60.0).floor(),
                    secs % 60.0,
                    t.index1
                );
            }
            let p = &iso.pvd;
            println!("system     {:?}", p.system_id);
            println!("volume     {:?}  {} blocks", p.volume_id, p.volume_blocks);
            println!("publisher  {:?}", p.publisher);
            println!("preparer   {:?}", p.preparer);
            println!("app        {:?}", p.application);
            println!("created    {:?}", p.created);
            // Sector kinds across the data track, by submode.
            let mut kinds = std::collections::BTreeMap::new();
            for lba in 0..disc.sectors() {
                let s = disc.sector(lba).map_err(|e| e.to_string())?;
                *kinds.entry((s.mode(), s.submode() & 0x7e)).or_insert(0u32) += 1;
            }
            println!("data track {} sectors", disc.sectors());
            for ((mode, sm), n) in kinds {
                println!("  mode {mode} submode {sm:02x}: {n}");
            }
            // Runs of sectors whose submode differs from plain Form 1 data,
            // with the file that owns them.
            let files = iso.walk().map_err(|e| e.to_string())?;
            let owner = |lba: u32| {
                files
                    .iter()
                    .find(|f| !f.is_dir && lba >= f.lba && lba < f.lba + f.sectors())
                    .map_or("(no file)", |f| f.path.as_str())
            };
            let mut lba = 0;
            while lba < disc.sectors() {
                let sub = disc.sector(lba).map_err(|e| e.to_string())?.subheader();
                if sub[2] & 0x7e == 0x08 {
                    lba += 1;
                    continue;
                }
                let start = lba;
                while lba < disc.sectors() && disc.sector(lba).map_err(|e| e.to_string())?.subheader() == sub {
                    lba += 1;
                }
                println!("  {start}..{lba}: subheader {:02x?}  {}", sub, owner(start));
            }
        }
        "ls" => {
            println!("{:>7} {:>10} {:>6} {:>14}  path", "lba", "size", "secs", "xa");
            for e in iso.walk().map_err(|e| e.to_string())? {
                let d = e.date;
                println!(
                    "{:>7} {:>10} {:>6} {:>14}  {}{}   {}-{:02}-{:02} {:02}:{:02}",
                    e.lba,
                    e.size,
                    e.sectors(),
                    xa_text(e.xa_attr),
                    e.path,
                    if e.is_dir { "/" } else { "" },
                    1900 + d[0] as u32,
                    d[1],
                    d[2],
                    d[3],
                    d[4]
                );
            }
        }
        "extract" => {
            let out = args.items.first().map(PathBuf::from).unwrap_or_else(|| root().join("work/fs"));
            let mut files = 0;
            let mut raw = 0;
            for e in iso.walk().map_err(|e| e.to_string())? {
                let dest = out.join(e.path.trim_start_matches('/'));
                if e.is_dir {
                    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
                    continue;
                }
                // CD-DA files are directory entries pointing into the audio
                // tracks; `disc audio` writes those.
                if e.xa_attr.is_some_and(|a| a & hwtr_disc::iso::xa::CDDA != 0) {
                    continue;
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                // A file is written raw when any of its sectors is Form 2,
                // whatever its directory record claims.
                let form2 = e.is_form2()
                    || (0..e.sectors()).any(|i| disc.sector(e.lba + i).map(|s| s.is_form2()).unwrap_or(false));
                let bytes = if form2 {
                    raw += 1;
                    disc.read_raw(e.lba, e.sectors()).map_err(|e| e.to_string())?.to_vec()
                } else {
                    iso.read(&e).map_err(|e| e.to_string())?
                };
                std::fs::write(&dest, bytes).map_err(|err| format!("{}: {err}", dest.display()))?;
                files += 1;
            }
            println!("{files} files ({raw} raw 2352-byte) -> {}", out.display());
        }
        "cat" => {
            let path = args.items.first().ok_or("disc cat PATH")?;
            let e = iso.find(path).map_err(|e| e.to_string())?;
            let bytes = iso.read(&e).map_err(|e| e.to_string())?;
            std::io::stdout().write_all(&bytes).map_err(|e| e.to_string())?;
        }
        "audio" => {
            let out = args.items.first().map(PathBuf::from).unwrap_or_else(|| root().join("work/audio"));
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            for t in disc.cue.tracks.iter().filter(|t| t.kind == TrackKind::Audio) {
                let pcm = std::fs::read(&t.file).map_err(|e| format!("{}: {e}", t.file.display()))?;
                let pcm = &pcm[t.index1 as usize * hwtr_disc::RAW_SECTOR..];
                let dest = out.join(format!("track{:02}.wav", t.number));
                write_wav(&dest, pcm).map_err(|e| e.to_string())?;
                println!("{} ({:.1} s)", dest.display(), pcm.len() as f64 / 176400.0);
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// 44.1 kHz 16-bit stereo little-endian, as CD-DA is stored.
fn write_wav(path: &Path, pcm: &[u8]) -> std::io::Result<()> {
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    let len = pcm.len() as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&44100u32.to_le_bytes())?;
    w.write_all(&(44100u32 * 4).to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(pcm)?;
    w.flush()
}

fn big_cmd(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let sub = args.items.first().cloned().unwrap_or_default();
    let file = args.items.get(1).ok_or("big ls|extract FILE")?;
    let bytes = read_input(file, cue)?;
    let big = hwtr_data::Big::parse(&bytes).map_err(|e| e.to_string())?;
    let all = big.walk().map_err(|e| e.to_string())?;
    match sub.as_str() {
        "ls" => {
            let mut bad = 0;
            println!("{:>10} {:>9} {:>8}  path", "offset", "size", "sum");
            for (path, m, data) in &all {
                let ok = hwtr_data::big::checksum(data) == m.sum;
                bad += usize::from(!ok);
                let magic: String =
                    data.iter().take(4).map(|&c| if c.is_ascii_graphic() { c as char } else { '.' }).collect();
                println!(
                    "{:>#10x} {:>9} {:08x}{} {path}  [{magic}]",
                    m.offset,
                    m.size,
                    m.sum,
                    if ok { " " } else { "!" }
                );
            }
            println!("{} members, {bad} bad checksums", all.len());
        }
        "extract" => {
            let out = args.items.get(2).map(PathBuf::from).unwrap_or_else(|| root().join("work/big"));
            let mut n = 0;
            for (path, m, data) in &all {
                if hwtr_data::Big::is_nested(m) {
                    continue;
                }
                let dest = out.join(path);
                std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
                std::fs::write(&dest, data).map_err(|e| e.to_string())?;
                n += 1;
            }
            println!("{n} members -> {}", out.display());
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn exe_info(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let file = args.items.get(1).ok_or("exe info FILE")?;
    let exe = Exe::parse(&read_input(file, cue)?).map_err(|e| e.to_string())?;
    println!("pc0     0x{:08x}", exe.pc0);
    println!("gp0     0x{:08x}", exe.gp0);
    println!("text    0x{:08x} .. 0x{:08x} ({} bytes)", exe.t_addr, exe.t_addr + exe.t_size, exe.t_size);
    println!("data    0x{:08x} ({} bytes)", exe.d_addr, exe.d_size);
    println!("bss     0x{:08x} .. 0x{:08x} ({} bytes)", exe.b_addr, exe.b_addr + exe.b_size, exe.b_size);
    println!("stack   0x{:08x} + 0x{:x}", exe.s_addr, exe.s_size);
    println!("marker  {:?}", exe.marker);
    Ok(())
}

/// Names from `symbols/<stem>.txt`: one `0xADDRESS name` per line, `#` comments.
fn load_symbols(file: &str) -> std::collections::BTreeMap<u32, String> {
    let stem = file.trim_start_matches("disc:").rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = stem.split('.').next().unwrap_or(stem).to_lowercase();
    let path = root().join("symbols").join(format!("{stem}.txt"));
    let mut out = std::collections::BTreeMap::new();
    for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut words = line.split_whitespace();
        if let (Some(a), Some(n)) = (words.next(), words.next())
            && let Ok(a) = parse_num(a)
        {
            out.insert(a, n.to_string());
        }
    }
    out
}

struct Program {
    exe: Option<Exe>,
    base: u32,
    image: Vec<u8>,
    names: std::collections::BTreeMap<u32, String>,
    funcs: hwtr_psx::analysis::Functions,
    refs: std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>>,
}

impl Program {
    fn load(args: &mut Args, cue: &Option<String>) -> Result<Program> {
        let base = args.take("--base").map(|s| parse_num(&s)).transpose()?;
        let file = args.items.first().ok_or("FILE")?.clone();
        let bytes = read_input(&file, cue)?;
        let (exe, base, image) = match base {
            Some(b) => (None, b, bytes),
            None => {
                let exe = Exe::parse(&bytes).map_err(|e| e.to_string())?;
                let (b, i) = (exe.t_addr, exe.image.clone());
                (Some(exe), b, i)
            }
        };
        let mem = hwtr_psx::Memory { base, bytes: &image };
        let entry = exe.as_ref().map_or(base, |e| e.pc0);
        let funcs = hwtr_psx::analysis::find_functions(&mem, entry);
        let refs = hwtr_psx::analysis::data_refs(&mem);
        let mut names = std::collections::BTreeMap::new();
        for &a in funcs.starts.keys() {
            let name = hwtr_psx::analysis::bios_stub(&mem, a)
                .and_then(hwtr_psx::analysis::bios_name)
                .map_or_else(|| format!("fn_{a:08x}"), |n| format!("bios_{n}"));
            names.insert(a, name);
        }
        names.extend(load_symbols(&file));
        Ok(Program { exe, base, image, names, funcs, refs })
    }

    fn mem(&self) -> hwtr_psx::Memory<'_> {
        hwtr_psx::Memory { base: self.base, bytes: &self.image }
    }

    /// A short note for an address the code refers to: a string or a name.
    fn note(&self, addr: u32) -> Option<String> {
        if let Some(n) = self.names.get(&addr) {
            return Some(n.clone());
        }
        let s = self.mem().cstr(addr).filter(|s| s.len() >= 3)?;
        Some(format!("{:?}", if s.len() > 60 { &s[..60] } else { s }))
    }
}

fn disasm(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let from = args.take("--from").map(|s| parse_num(&s)).transpose()?;
    let to = args.take("--to").map(|s| parse_num(&s)).transpose()?;
    let count = args.take("--count").map(|s| parse_num(&s)).transpose()?;
    let p = Program::load(args, cue)?;
    let mem = p.mem();
    let start = from.unwrap_or(p.base) & !3;
    let end = match (to, count) {
        (Some(t), _) => t,
        (None, Some(n)) => start + n * 4,
        (None, None) => mem.end(),
    }
    .min(mem.end());
    // Reference sites -> the address they form.
    let mut site_note = std::collections::BTreeMap::new();
    for (&target, sites) in &p.refs {
        if let Some(n) = p.note(target) {
            for &s in sites {
                site_note.insert(s, n.clone());
            }
        }
    }
    let label = |a: u32| p.names.get(&a).cloned();
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    let mut pc = start;
    while pc < end {
        if let Some(name) = p.names.get(&pc) {
            let callers = p.funcs.callers.get(&pc).map_or(0, |c| c.len());
            let _ = writeln!(out, "\n{name}:  # {pc:08x}, {callers} callers");
        }
        let word = mem.u32(pc).unwrap();
        let insn = mips::decode(word);
        let text = insn.render(pc, &label);
        match site_note.get(&pc) {
            Some(n) => {
                let _ = writeln!(out, "{pc:08x}: {word:08x}  {text:<40} # {n}");
            }
            None => {
                let _ = writeln!(out, "{pc:08x}: {word:08x}  {text}");
            }
        }
        pc += 4;
    }
    Ok(())
}

fn funcs(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let p = Program::load(args, cue)?;
    let mut by_kind = std::collections::BTreeMap::new();
    for kind in p.funcs.starts.values() {
        *by_kind.entry(*kind).or_insert(0) += 1;
    }
    if let Some(exe) = &p.exe {
        println!("entry 0x{:08x}", exe.pc0);
    }
    println!("{} functions: {by_kind:?}", p.funcs.starts.len());
    let starts: Vec<u32> = p.funcs.starts.keys().copied().collect();
    for (i, &a) in starts.iter().enumerate() {
        let size = starts.get(i + 1).map_or(p.mem().end(), |&n| n) - a;
        let callers = p.funcs.callers.get(&a).map_or(0, |c| c.len());
        println!("{a:08x} {size:>6} {callers:>4}  {}", p.names[&a]);
    }
    Ok(())
}

fn strings(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let p = Program::load(args, cue)?;
    for (&target, sites) in &p.refs {
        if let Some(s) = p.mem().cstr(target).filter(|s| s.len() >= 3) {
            let users: std::collections::BTreeSet<String> = sites
                .iter()
                .map(|&site| {
                    let f = p.funcs.starts.range(..=site).next_back().map_or(0, |(&a, _)| a);
                    p.names.get(&f).cloned().unwrap_or_else(|| format!("{f:08x}"))
                })
                .collect();
            println!("{target:08x} {:<50} {}", format!("{s:?}"), users.into_iter().collect::<Vec<_>>().join(" "));
        }
    }
    Ok(())
}

fn main() {
    let mut args = Args { items: std::env::args().skip(1).collect() };
    let cue = args.take("--cue");
    let command = if args.items.is_empty() { String::new() } else { args.items.remove(0) };
    let result = match command.as_str() {
        "disc" => disc_cmd(&mut args, &cue),
        "exe" => exe_info(&mut args, &cue),
        "big" => big_cmd(&mut args, &cue),
        "disasm" => disasm(&mut args, &cue),
        "funcs" => funcs(&mut args, &cue),
        "strings" => strings(&mut args, &cue),
        "docs" => docs::run(&root(), &args.items),
        _ => Err(USAGE.into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
