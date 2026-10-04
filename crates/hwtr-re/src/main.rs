//! hwtr-re: the reverse engineering toolbox for Hot Wheels Turbo Racing.

#![forbid(unsafe_code)]

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
  tim FILE|DIR [OUT] [--clut N]
                            TIM to PNG; a directory converts every TIM under it (default work/png)
  vab DIR|VH [OUT]          check every VAB (VH with its VB) under DIR, or write one bank's samples as WAV
  exe info FILE             PS-X EXE header
  disasm FILE [--from ADDR] [--to ADDR] [--count N] [--gp ADDR]
                            disassemble a PS-X EXE (or a raw image with --base ADDR),
                            with function labels, BIOS calls, and the strings the code points at
  disasm FILE --func ADDR   one whole function
  funcs FILE                functions found by recursive descent: size, calls, callers, flags
                            (i indirect calls, u unresolved jr, ! hit invalid, t tail calls)
  calls FILE [ADDR|NAME] [--depth N]
                            the call tree under a function (default the entry)
  callers FILE ADDR|NAME [--depth N]
                            the tree of callers above a function, through interface slots too
  xrefs FILE ADDR           calls, jumps, data references and pointers to an address
  slots FILE                interface slots: fixed words holding function addresses, and their jalr uses
  pseudo FILE ADDR|NAME     C-like pseudo-code for a function (a reading aid)
  fsm FILE ADDR             one of the game's state machines: states, their function lists, transitions
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

fn tim_cmd(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let clut = args.take("--clut").map(|s| parse_num(&s)).transpose()?.unwrap_or(0) as usize;
    let input = args.items.first().ok_or("tim FILE|DIR [OUT]")?.clone();
    let convert = |bytes: &[u8], out: &Path| -> Result<String> {
        let tim = hwtr_data::Tim::parse(bytes).map_err(|e| e.to_string())?;
        let png = hwtr_data::png::encode(tim.width(), tim.height(), &tim.to_rgba(clut));
        std::fs::write(out, png).map_err(|e| e.to_string())?;
        Ok(format!(
            "{:?} {}x{} at vram ({}, {}){}",
            tim.mode,
            tim.width(),
            tim.height(),
            tim.rect.x,
            tim.rect.y,
            tim.clut.as_ref().map_or(String::new(), |(r, _)| format!(", {} palettes at ({}, {})", r.h, r.x, r.y))
        ))
    };
    let path = PathBuf::from(&input);
    if path.is_dir() {
        let out = args.items.get(1).map(PathBuf::from).unwrap_or_else(|| root().join("work/png"));
        let mut stack = vec![path.clone()];
        let (mut ok, mut bad) = (0, 0);
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy().to_uppercase();
                if !name.ends_with("TIM") {
                    continue;
                }
                let rel = p.strip_prefix(&path).unwrap();
                let dest = out.join(rel).with_extension("png");
                std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
                match convert(&std::fs::read(&p).map_err(|e| e.to_string())?, &dest) {
                    Ok(_) => ok += 1,
                    Err(e) => {
                        bad += 1;
                        eprintln!("{}: {e}", p.display());
                    }
                }
            }
        }
        println!("{ok} TIMs converted, {bad} failed -> {}", out.display());
    } else {
        let out = args.items.get(1).map(PathBuf::from).unwrap_or_else(|| path.with_extension("png"));
        println!("{}", convert(&read_input(&input, cue)?, &out)?);
    }
    Ok(())
}

fn vab_cmd(args: &mut Args) -> Result<()> {
    use hwtr_data::vab;
    let input = PathBuf::from(args.items.first().ok_or("vab DIR|VH [OUT]")?);
    let body = |vh: &Path| {
        let name = vh.file_name().unwrap().to_string_lossy();
        vh.with_file_name(format!("{}VB", &name[..name.len() - 2]))
    };
    if input.is_dir() {
        let mut stack = vec![input];
        let (mut ok, mut bad, mut samples) = (0, 0, 0);
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if !p.file_name().unwrap().to_string_lossy().ends_with("VH") {
                    continue;
                }
                let vh = std::fs::read(&p).map_err(|e| e.to_string())?;
                let vb = std::fs::read(body(&p)).unwrap_or_default();
                match vab::Vab::parse(&vh) {
                    Ok(v) => {
                        let total: u32 = v.vag_sizes.iter().sum();
                        if total as usize != vb.len() {
                            bad += 1;
                            eprintln!("{}: samples total {total}, VB is {}", p.display(), vb.len());
                        } else {
                            ok += 1;
                            samples += v.vag_sizes.len() - 1;
                        }
                    }
                    Err(e) => {
                        bad += 1;
                        eprintln!("{}: {e}", p.display());
                    }
                }
            }
        }
        println!("{ok} banks parse and match their VB ({samples} samples), {bad} do not");
    } else {
        let vh = std::fs::read(&input).map_err(|e| e.to_string())?;
        let vb = std::fs::read(body(&input)).map_err(|e| e.to_string())?;
        let v = vab::Vab::parse(&vh)?;
        let out = args.items.get(1).map(PathBuf::from).unwrap_or_else(|| root().join("work/wav"));
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        println!("{} programs, {} tones, {} samples", v.programs.len(), v.tones.len(), v.vag_sizes.len() - 1);
        for t in &v.tones {
            println!(
                "  program {:>3} vag {:>3} notes {:>3}-{:>3} center {:>3}.{:<3} vol {:>3} pan {:>3} adsr {:04x} {:04x}",
                t.program, t.vag, t.min_note, t.max_note, t.center, t.shift, t.volume, t.pan, t.adsr1, t.adsr2
            );
        }
        let stem = input.file_name().unwrap().to_string_lossy().to_string();
        for n in 1..v.vag_sizes.len() {
            let pcm = vab::decode_adpcm(v.vag(&vb, n).ok_or("sample past the VB")?);
            // A VAB does not store the recording rate; 22050 Hz is assumed.
            std::fs::write(out.join(format!("{stem}_{n:02}.wav")), vab::wav(&pcm, 22050)).map_err(|e| e.to_string())?;
        }
        println!("-> {}", out.display());
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
    names: std::collections::BTreeMap<u32, String>,
    a: hwtr_psx::program::Program<'static>,
    refs: std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>>,
}

impl Program {
    fn load(args: &mut Args, cue: &Option<String>) -> Result<Program> {
        let base = args.take("--base").map(|s| parse_num(&s)).transpose()?;
        let gp_arg = args.take("--gp").map(|s| parse_num(&s)).transpose()?;
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
        // The analysis borrows the image for the life of the command.
        let image: &'static [u8] = Box::leak(image.into_boxed_slice());
        let mem = hwtr_psx::Memory { base, bytes: image };
        let entry = exe.as_ref().map_or(base, |e| e.pc0);
        let gp = gp_arg.or_else(|| hwtr_psx::analysis::find_gp(&mem, entry));
        let a = hwtr_psx::program::Program::analyze(mem, entry, gp);
        let refs = hwtr_psx::analysis::data_refs(&mem, gp);
        let mut names = std::collections::BTreeMap::new();
        for &f in a.funcs.keys() {
            let name = hwtr_psx::analysis::bios_stub(&mem, f)
                .and_then(hwtr_psx::analysis::bios_name)
                .map_or_else(|| format!("fn_{f:08x}"), |n| format!("bios_{n}"));
            names.insert(f, name);
        }
        for jt in a.jump_tables.values() {
            for &t in &jt.targets {
                names.entry(t).or_insert_with(|| format!("case_{t:08x}"));
            }
        }
        names.extend(load_symbols(&file));
        Ok(Program { exe, names, a, refs })
    }

    fn mem(&self) -> hwtr_psx::Memory<'static> {
        self.a.mem
    }

    fn name(&self, addr: u32) -> String {
        self.names.get(&addr).cloned().unwrap_or_else(|| format!("0x{addr:08x}"))
    }

    /// A short note for an address the code refers to: a string or a name.
    fn note(&self, addr: u32) -> Option<String> {
        if let Some(n) = self.names.get(&addr) {
            return Some(n.clone());
        }
        if let Some(s) = self.mem().cstr(addr).filter(|s| s.len() >= 3) {
            return Some(format!("{:?}", if s.len() > 60 { &s[..60] } else { s }));
        }
        // Inside a named object: name+offset.
        let (&at, n) = self.names.range(..addr).next_back()?;
        (addr - at < 0x100 && !n.starts_with("fn_") && !n.starts_with("case_"))
            .then(|| format!("{n}+0x{:x}", addr - at))
    }
}

fn disasm(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let from = args.take("--from").map(|s| parse_num(&s)).transpose()?;
    let to = args.take("--to").map(|s| parse_num(&s)).transpose()?;
    let count = args.take("--count").map(|s| parse_num(&s)).transpose()?;
    let func = args.take("--func").map(|s| parse_num(&s)).transpose()?;
    let p = Program::load(args, cue)?;
    let mem = p.mem();
    let (start, end) = match func {
        Some(f) => {
            let f = p.a.funcs.get(&f).ok_or(format!("no function starts at 0x{f:08x}"))?;
            (f.start, f.end)
        }
        None => {
            let start = from.unwrap_or(mem.base) & !3;
            let end = match (to, count) {
                (Some(t), _) => t,
                (None, Some(n)) => start + n * 4,
                (None, None) => mem.end(),
            };
            (start, end)
        }
    };
    let end = end.min(mem.end());
    // Reference sites -> the address they form.
    let mut site_note = std::collections::BTreeMap::new();
    // Every address the code forms gets a note, so nobody has to add a
    // `lui` and an `addiu` by hand.
    for (&target, sites) in &p.refs {
        let n = p.note(target).unwrap_or_else(|| format!("0x{target:08x}"));
        for &s in sites {
            site_note.insert(s, n.clone());
        }
    }
    let label = |a: u32| p.names.get(&a).cloned();
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    let mut pc = start;
    while pc < end {
        if let Some(f) = p.a.funcs.get(&pc) {
            let callers = p.a.callers.get(&pc).map_or(0, |c| c.len());
            let _ = writeln!(
                out,
                "\n{}:  # {pc:08x}..{:08x}, {} insns, {callers} callers, found by {}",
                p.name(pc),
                f.end,
                f.insns,
                f.how
            );
        } else if let Some(n) = p.names.get(&pc) {
            let _ = writeln!(out, "{n}:");
        }
        let word = mem.u32(pc).unwrap();
        let insn = mips::decode(word);
        let text = insn.render(pc, &label);
        let mark = if p.a.owner.contains_key(&pc) { ' ' } else { '?' };
        match site_note.get(&pc) {
            Some(n) => {
                let _ = writeln!(out, "{pc:08x}:{mark}{word:08x}  {text:<40} # {n}");
            }
            None => {
                let _ = writeln!(out, "{pc:08x}:{mark}{word:08x}  {text}");
            }
        }
        if let Some(jt) = p.a.jump_tables.get(&pc.wrapping_sub(4)) {
            let _ = writeln!(out, "          # jump table 0x{:08x}, {} cases", jt.table, jt.targets.len());
        }
        pc += 4;
    }
    Ok(())
}

fn funcs(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let p = Program::load(args, cue)?;
    let mut by_kind = std::collections::BTreeMap::new();
    for f in p.a.funcs.values() {
        *by_kind.entry(f.how).or_insert(0) += 1;
    }
    if let Some(exe) = &p.exe {
        println!("entry 0x{:08x}", exe.pc0);
    }
    println!("gp {}", p.a.gp.map_or("unknown".into(), |g| format!("0x{g:08x}")));
    println!(
        "{} functions {by_kind:?}, code ends 0x{:08x}, {} jump tables, {} unowned code words",
        p.a.funcs.len(),
        p.a.code_end,
        p.a.jump_tables.len(),
        p.a.unowned_words()
    );
    let resolved = p.a.indirect.values().filter(|s| p.a.slots.get(s).is_some_and(|t| t.len() == 1)).count();
    println!(
        "{} interface slots hold functions; {} jalr sites load from a fixed slot, {resolved} resolved to one callee",
        p.a.slots.len(),
        p.a.indirect.len()
    );
    println!("{:>8} {:>6} {:>5} {:>5} {:>4}  name", "start", "bytes", "calls", "calrs", "flags");
    for f in p.a.funcs.values() {
        let callers = p.a.callers.get(&f.start).map_or(0, |c| c.len());
        let mut flags = String::new();
        if f.indirect_calls > 0 {
            flags.push('i');
        }
        if f.unresolved_jumps > 0 {
            flags.push('u');
        }
        if f.hit_invalid {
            flags.push('!');
        }
        if !f.tail_calls.is_empty() {
            flags.push('t');
        }
        println!(
            "{:08x} {:>6} {:>5} {:>5} {:>4}  {}",
            f.start,
            f.end - f.start,
            f.calls.len(),
            callers,
            flags,
            p.name(f.start)
        );
    }
    Ok(())
}

fn strings(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let p = Program::load(args, cue)?;
    for (&target, sites) in &p.refs {
        if let Some(s) = p.mem().cstr(target).filter(|s| s.len() >= 3) {
            let users: std::collections::BTreeSet<String> = sites
                .iter()
                .map(|&site| p.a.func_of(site).map_or(format!("{site:08x}"), |f| p.name(f.start)))
                .collect();
            println!("{target:08x} {:<50} {}", format!("{s:?}"), users.into_iter().collect::<Vec<_>>().join(" "));
        }
    }
    Ok(())
}

/// The call tree under a function, each callee once.
fn calls(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let depth = args.take("--depth").map(|s| parse_num(&s)).transpose()?.unwrap_or(4);
    let root_arg = args.items.get(1).cloned();
    let p = Program::load(args, cue)?;
    let root = match root_arg {
        Some(r) => parse_num(&r)
            .or_else(|_| p.names.iter().find(|(_, n)| **n == r).map(|(&a, _)| a).ok_or(format!("no symbol {r}")))?,
        None => p.exe.as_ref().map_or(p.mem().base, |e| e.pc0),
    };
    let mut seen = std::collections::BTreeSet::new();
    fn go(p: &Program, f: u32, d: u32, max: u32, seen: &mut std::collections::BTreeSet<u32>) {
        let Some(func) = p.a.funcs.get(&f) else { return };
        let again = !seen.insert(f);
        println!(
            "{}{} 0x{f:08x}{}{}",
            "  ".repeat(d as usize),
            p.name(f),
            if func.indirect_calls > 0 { format!(" [{} indirect]", func.indirect_calls) } else { String::new() },
            if again { " ..." } else { "" }
        );
        if again || d >= max {
            return;
        }
        for &c in func.calls.iter().chain(func.tail_calls.iter()) {
            go(p, c, d + 1, max, seen);
        }
    }
    go(&p, root, 0, depth, &mut seen);
    Ok(())
}

/// Dumps one of the game's table-driven state machines (ticked by
/// 0x8006bb18): its states, each state's enter, update and exit function
/// lists, and its event transitions.
fn fsm(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let addr = parse_num(args.items.get(1).ok_or("fsm FILE ADDR")?)?;
    let p = Program::load(args, cue)?;
    let m = p.mem();
    let word = |a: u32| m.u32(a).ok_or(format!("0x{a:08x} is outside the image"));
    let half = |a: u32| m.u16(a).map(|v| v as i16).ok_or(format!("0x{a:08x} is outside the image"));
    let byte = |a: u32| m.u8(a).map(|v| v as i8).ok_or(format!("0x{a:08x} is outside the image"));
    let states = word(addr)?;
    let (initial, last) = (half(addr + 4)?, half(addr + 6)?);
    println!("machine 0x{addr:08x}: states at 0x{states:08x}, initial {initial}, final {last}");
    // The state table has no count; read until a word is not a pointer to a
    // plausible state record, and at least past both named states.
    // Who posts which event to this machine: calls to fsm_post with this
    // machine in a0 and a constant event in a1.
    let post = p.names.iter().find(|(_, n)| *n == "fsm_post").map(|(&a, _)| a);
    let mut posters: std::collections::BTreeMap<u32, std::collections::BTreeSet<String>> = Default::default();
    if let Some(post) = post {
        for &site in p.a.callers.get(&post).into_iter().flatten() {
            if p.a.arg_at_call(site, 4) == Some(addr)
                && let Some(event) = p.a.arg_at_call(site, 5)
            {
                let by = p.a.func_of(site).map_or(format!("{site:08x}"), |f| p.name(f.start));
                posters.entry(event).or_default().insert(by);
            }
        }
    }
    let mut index = 0u32;
    while let Ok(st) = word(states + index * 4) {
        if !m.contains(st) || (index as i16 > initial.max(last) && st < m.base) {
            break;
        }
        let counts = [byte(st + 16)?, byte(st + 17)?, byte(st + 18)?, byte(st + 19)?];
        if counts.iter().any(|&c| !(0..=64).contains(&c)) {
            break;
        }
        println!("\nstate {index} @ 0x{st:08x}");
        for (k, label) in ["enter", "update", "exit"].iter().enumerate() {
            let list = word(st + k as u32 * 4)?;
            let fns: Vec<String> =
                (0..counts[k] as u32).map(|i| word(list + i * 4).map(|f| p.name(f))).collect::<Result<_>>()?;
            println!("  {label:<7} {}", fns.join(", "));
        }
        let trans = word(st + 12)?;
        for i in 0..counts[3] as u32 {
            let (event, target) = (half(trans + i * 4)?, half(trans + i * 4 + 2)?);
            let by = posters.get(&(event as u32)).map_or(String::new(), |b| {
                format!("   posted by {}", b.iter().cloned().collect::<Vec<_>>().join(", "))
            });
            println!("  on event {event:>3} -> state {target}{by}");
        }
        index += 1;
    }
    Ok(())
}

/// The fixed words the code stores function addresses into, and how many
/// `jalr` sites call through each.
fn slots(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let p = Program::load(args, cue)?;
    let mut uses: std::collections::BTreeMap<u32, u32> = Default::default();
    for &slot in p.a.indirect.values() {
        *uses.entry(slot).or_default() += 1;
    }
    let all: std::collections::BTreeSet<u32> = p.a.slots.keys().chain(uses.keys()).copied().collect();
    for slot in all {
        let fns =
            p.a.slots
                .get(&slot)
                .map_or(String::from("-"), |t| t.iter().map(|&f| p.name(f)).collect::<Vec<_>>().join(" "));
        println!(
            "{slot:08x} {:>4} calls  {} {fns}",
            uses.get(&slot).copied().unwrap_or(0),
            p.note(slot).unwrap_or_default()
        );
    }
    Ok(())
}

/// The tree of callers above a function: who calls it, who calls them.
/// Calls through resolved interface slots count.
fn callers(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let depth = args.take("--depth").map(|s| parse_num(&s)).transpose()?.unwrap_or(4);
    let target = args.items.get(1).cloned().ok_or("callers FILE ADDR|NAME")?;
    let p = Program::load(args, cue)?;
    let root = parse_num(&target).or_else(|_| {
        p.names.iter().find(|(_, n)| **n == target).map(|(&a, _)| a).ok_or(format!("no symbol {target}"))
    })?;
    // Slots a function is stored in, and the jalr sites that call through them.
    let mut via_slot: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
    for (&site, slot) in &p.a.indirect {
        if let Some(fs) = p.a.slots.get(slot) {
            for &f in fs {
                via_slot.entry(f).or_default().push(site);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    fn go(
        p: &Program,
        via: &std::collections::BTreeMap<u32, Vec<u32>>,
        f: u32,
        d: u32,
        max: u32,
        seen: &mut std::collections::BTreeSet<u32>,
    ) {
        let again = !seen.insert(f);
        println!("{}{} 0x{f:08x}{}", "  ".repeat(d as usize), p.name(f), if again { " ..." } else { "" });
        if again || d >= max {
            return;
        }
        let mut up: std::collections::BTreeSet<u32> = Default::default();
        for &site in p.a.callers.get(&f).into_iter().flatten().chain(via.get(&f).into_iter().flatten()) {
            if let Some(c) = p.a.func_of(site) {
                up.insert(c.start);
            }
        }
        for c in up {
            go(p, via, c, d + 1, max, seen);
        }
    }
    go(&p, &via_slot, root, 0, depth, &mut seen);
    Ok(())
}

/// C-like pseudo-code for a function, or with `--all` a check that every
/// function decompiles.
fn pseudo(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let target = args.items.get(1).cloned().ok_or("pseudo FILE ADDR|NAME|--all")?;
    let p = Program::load(args, cue)?;
    let names = |a: u32| -> Option<String> {
        if let Some(n) = p.names.get(&a) {
            return Some(n.clone());
        }
        p.mem().cstr(a).filter(|s| s.len() >= 3).map(|s| format!("{:?}", if s.len() > 40 { &s[..40] } else { s }))
    };
    if target == "--all" {
        let (mut ok, mut bad) = (0, 0);
        for &f in p.a.funcs.keys() {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hwtr_psx::decomp::decompile(&p.a, f, &names)
            })) {
                Ok(Some(t)) if !t.contains('\0') => ok += 1,
                Ok(_) => {
                    bad += 1;
                    println!("{f:08x}: unresolved mark in output");
                }
                Err(_) => {
                    bad += 1;
                    println!("{f:08x}: panicked");
                }
            }
        }
        println!("{ok} functions decompiled, {bad} failed");
        return Ok(());
    }
    let addr = parse_num(&target).or_else(|_| {
        p.names.iter().find(|(_, n)| **n == target).map(|(&a, _)| a).ok_or(format!("no symbol {target}"))
    })?;
    let text = hwtr_psx::decomp::decompile(&p.a, addr, &names).ok_or(format!("no function at 0x{addr:08x}"))?;
    print!("{text}");
    Ok(())
}

/// Everything that refers to an address: calls, jumps, data references.
fn xrefs(args: &mut Args, cue: &Option<String>) -> Result<()> {
    let target = parse_num(args.items.get(1).ok_or("xrefs FILE ADDR")?)?;
    let p = Program::load(args, cue)?;
    let at = |site: u32| p.a.func_of(site).map_or(String::from("?"), |f| p.name(f.start));
    for &site in p.a.callers.get(&target).into_iter().flatten() {
        println!("{site:08x} call/jump   in {}", at(site));
    }
    for &site in p.refs.get(&target).into_iter().flatten() {
        let insn = mips::decode(p.mem().u32(site).unwrap());
        println!("{site:08x} {:<10}  in {}", insn.mnemonic(), at(site));
    }
    // Words in the image holding the address.
    let mem = p.mem();
    let mut a = mem.base;
    while a + 4 <= mem.end() {
        if mem.u32(a) == Some(target) {
            println!("{a:08x} pointer");
        }
        a += 4;
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
        "tim" => tim_cmd(&mut args, &cue),
        "vab" => vab_cmd(&mut args),
        "disasm" => disasm(&mut args, &cue),
        "funcs" => funcs(&mut args, &cue),
        "strings" => strings(&mut args, &cue),
        "calls" => calls(&mut args, &cue),
        "xrefs" => xrefs(&mut args, &cue),
        "fsm" => fsm(&mut args, &cue),
        "pseudo" => pseudo(&mut args, &cue),
        "callers" => callers(&mut args, &cue),
        "slots" => slots(&mut args, &cue),
        "docs" => docs::run(&root(), &args.items),
        _ => Err(USAGE.into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
