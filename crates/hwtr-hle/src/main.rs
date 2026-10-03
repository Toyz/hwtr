//! hwtr-hle: run the original game headless for N frames.
//!
//! ```text
//! hwtr-hle [FRAMES] [--shots DIR] [--every N] [--press F:BUTTONS[:LEN],...]
//!          [--analog] [--stick F:LX,LY[:LEN];...] [--save FILE] [--load FILE]
//!          [--peek ADDR:LEN,...]     memory after the run, as words
//!          [--writes]                which functions stored where
//! ```
//!
//! Writes the displayed picture every N frames (and VRAM at the end) as PNG.

use std::path::PathBuf;
use std::rc::Rc;

fn png(path: &std::path::Path, w: usize, h: usize, rgba: &[u8]) {
    // A tiny PNG writer lives in hwtr-data; this crate stays free of it by
    // writing PPM, which every viewer opens.
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in rgba.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    std::fs::write(path.with_extension("ppm"), out).expect("write");
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let (mut frames, mut shots, mut every) = (120u64, PathBuf::from("work/hle"), 30u64);
    let mut script = hwtr_hle::script::Script::default();
    let (mut save, mut load) = (None::<PathBuf>, None::<PathBuf>);
    let mut peeks: Vec<(u32, u32)> = Vec::new();
    let mut writes = false;
    let mut writes_from = 0u64;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--analog" => script.analog = true,
            "--save" => save = args.next().map(PathBuf::from),
            "--writes" => writes = true,
            "--writes-from" => {
                writes = true;
                writes_from = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            }
            "--peek" => {
                for p in args.next().unwrap_or_default().split(',') {
                    let (a, n) = p.split_once(':').unwrap_or((p, "64"));
                    let a = u32::from_str_radix(a.trim_start_matches("0x"), 16).unwrap_or(0);
                    let n = n.parse().or_else(|_| u32::from_str_radix(n.trim_start_matches("0x"), 16)).unwrap_or(64);
                    peeks.push((a, n));
                }
            }
            "--load" => load = args.next().map(PathBuf::from),
            "--stick" => script.stick(&args.next().unwrap_or_default()),
            "--press" => script.press(&args.next().unwrap_or_default()),
            "--shots" => shots = args.next().map(PathBuf::from).unwrap_or(shots),
            "--every" => every = args.next().and_then(|s| s.parse().ok()).unwrap_or(every),
            n => frames = n.parse().unwrap_or(frames),
        }
    }
    std::fs::create_dir_all(&shots).expect("shots dir");
    let cue = hwtr_disc::Disc::find_cue(std::path::Path::new("work/disc")).expect("cue");
    let disc = Rc::new(hwtr_disc::Disc::open(&cue).expect("disc"));
    let mut hle = hwtr_hle::Hle::new(disc).expect("hle");
    if let Some(p) = &load {
        let bytes = std::fs::read(p).expect("state file");
        hle.load(&bytes).expect("state");
        tracing::info!("loaded {} ({} frames in)", p.display(), hle.frames.get());
    }
    if writes && writes_from == 0 {
        hle.m.stores = Some(Vec::new());
    }
    // Watches start from the values in memory now, not from zero.
    for i in 0..hle.m.watch.len() {
        let a = hle.m.watch[i].0;
        hle.m.watch[i].1 = hle.m.bus.read_u32(a);
    }
    let start = std::time::Instant::now();
    let mut steps = 0u64;
    hle.m.step_limit = 30_000_000;
    let sp_guard = std::env::var("HWTR_SPGUARD").ok().and_then(|v| u32::from_str_radix(&v, 16).ok());
    // HWTR_WATCH=ADDR[,ADDR]: report every change to those words.
    if let Ok(w) = std::env::var("HWTR_WATCH") {
        for a in w.split(',') {
            if let Ok(a) = u32::from_str_radix(a.trim_start_matches("0x"), 16) {
                hle.m.watch.push((a, 0));
            }
        }
    }
    for f in 0..frames {
        hle.m.trace = Some(Vec::new());
        if writes && writes_from > 0 && f == writes_from {
            hle.m.stores = Some(Vec::new());
        }
        if f == 300 {
            hle.m.sp_guard = sp_guard.unwrap_or(0);
        }
        script.apply(&mut hle, f);
        if let Err(e) = hle.frame() {
            tracing::error!("frame {f}: {e:x?} after {} steps", hle.m.steps);
            let fc = hle.m.fault_cpu.clone().unwrap_or_else(|| hle.m.cpu.clone());
            let r = &fc.r;
            let names = hwtr_psx::mips::REG;
            let regs: Vec<String> = (1..32).map(|i| format!("{}={:08x}", names[i], r[i])).collect();
            tracing::error!("registers: {}", regs.join(" "));
            let sp = r[29];
            let stack: Vec<String> = (0..48).map(|k| format!("{:08x}", hle.m.bus.read_u32(sp + 4 * k))).collect();
            tracing::error!("stack from {sp:08x}: {}", stack.join(" "));
            // Code addresses on the handler stack: the call chain.
            let mut codes = std::collections::BTreeMap::<u32, u32>::new();
            for a in (0x8000_0000u32..0x8001_0000).step_by(4) {
                let v = hle.m.bus.read_u32(a);
                if (0x8001_0000..0x800b_6000).contains(&v) {
                    *codes.entry(v).or_default() += 1;
                }
            }
            let mut codes: Vec<_> = codes.into_iter().collect();
            codes.sort_by_key(|&(a, n)| (std::cmp::Reverse(n), a));
            tracing::error!("code addresses on the handler stack: {:x?}", &codes[..codes.len().min(10)]);
            let rd = |b: &mut hwtr_cpu::Bus, a: u32| b.read_u32(a);
            let (head, tail, chcr_ptr) =
                (rd(&mut hle.m.bus, 0x800c_78c4), rd(&mut hle.m.bus, 0x800c_78c8), rd(&mut hle.m.bus, 0x800c_78b0));
            let chcr = rd(&mut hle.m.bus, chcr_ptr);
            tracing::error!("gpu queue head {head} tail {tail}; *0x800c78b0 = {chcr_ptr:08x} -> {chcr:08x}");
            if let Some(t) = &hle.m.trace {
                let tail: Vec<String> = t.iter().rev().take(40).rev().map(|pc| format!("{pc:08x}")).collect();
                tracing::error!("last pcs: {}", tail.join(" "));
                let mut hist = std::collections::HashMap::<u32, u64>::new();
                for &pc in t.iter().rev().take(2_000_000) {
                    *hist.entry(pc & !0xff).or_default() += 1;
                }
                let mut hot: Vec<_> = hist.into_iter().collect();
                hot.sort_by_key(|&(a, n)| (std::cmp::Reverse(n), a));
                tracing::error!("hot 256-byte blocks: {:x?}", &hot[..hot.len().min(6)]);
            }
            break;
        }
        steps += hle.m.steps;
        if f == 150 && std::env::var_os("HWTR_IRQTAB").is_some() {
            let t = hle.m.bus.read_u32(0x800c_775c);
            let words: Vec<String> = (0..16).map(|k| format!("{:08x}", hle.m.bus.read_u32(t + 4 * k))).collect();
            tracing::info!("interrupt table at {t:08x}: {}", words.join(" "));
        }
        if std::env::var_os("HWTR_SP").is_some() && f % 25 == 0 {
            tracing::info!("frame {f}: sp {:08x} pc {:08x}", hle.m.cpu.r[29], hle.m.cpu.pc);
        }
        for (pc, addr, old, new) in hle.m.watched.drain(..) {
            tracing::info!("frame {f}: {pc:08x} wrote {addr:08x}: {old:08x} -> {new:08x}");
        }
        if f % every == every - 1 {
            let (w, h, rgba) = hle.hw.borrow().gpu.screen();
            png(&shots.join(format!("frame{f:05}")), w, h, &rgba);
            let hw = hle.hw.borrow();
            tracing::info!(
                "frame {f}: {} steps so far, {} primitives drawn, display {:?}",
                steps,
                hw.gpu.drawn,
                hw.gpu.display
            );
        }
    }
    // What the last frame spent its time in, by function.
    if let Some(t) = &hle.m.trace {
        let exe_bytes = {
            let cue = hwtr_disc::Disc::find_cue(std::path::Path::new("work/disc")).expect("cue");
            let disc = hwtr_disc::Disc::open(&cue).expect("disc");
            let iso = disc.iso().expect("iso");
            iso.find("CCCPSX.EXE").and_then(|e| iso.read(&e)).expect("exe")
        };
        let exe = hwtr_psx::Exe::parse(&exe_bytes).expect("exe");
        let gp = hwtr_psx::analysis::find_gp(&exe.view(), exe.pc0);
        let a = hwtr_psx::program::Program::analyze(exe.view(), exe.pc0, gp);
        let mut by: std::collections::HashMap<u32, u64> = Default::default();
        for &pc in t {
            if let Some(f) = a.func_of(pc) {
                *by.entry(f.start).or_default() += 1;
            }
        }
        let mut by: Vec<_> = by.into_iter().collect();
        by.sort_by_key(|&(f, n)| (std::cmp::Reverse(n), f));
        tracing::info!("last frame, {} steps, by function: {:x?}", t.len(), &by[..by.len().min(10)]);
    }
    // --writes: which functions stored where, over the whole run.
    if let Some(log) = hle.m.stores.take() {
        let exe_bytes = {
            let cue = hwtr_disc::Disc::find_cue(std::path::Path::new("work/disc")).expect("cue");
            let disc = hwtr_disc::Disc::open(&cue).expect("disc");
            let iso = disc.iso().expect("iso");
            iso.find("CCCPSX.EXE").and_then(|e| iso.read(&e)).expect("exe")
        };
        let exe = hwtr_psx::Exe::parse(&exe_bytes).expect("exe");
        let gp = hwtr_psx::analysis::find_gp(&exe.view(), exe.pc0);
        let a = hwtr_psx::program::Program::analyze(exe.view(), exe.pc0, gp);
        // function -> (stores, lowest and highest address, distinct 4 KB pages)
        let mut by: std::collections::BTreeMap<u32, (u64, u32, u32, std::collections::BTreeSet<u32>)> =
            Default::default();
        for &(pc, addr, _) in &log {
            let addr = addr & 0x1f_ffff | 0x8000_0000;
            let f = a.func_of(pc).map_or(pc, |f| f.start);
            let e = by.entry(f).or_insert((0, u32::MAX, 0, Default::default()));
            e.0 += 1;
            e.1 = e.1.min(addr);
            e.2 = e.2.max(addr);
            e.3.insert(addr >> 12);
        }
        // HWTR_WRITES_IN=LO-HI: also list, for that range, which functions
        // wrote each 64-byte block.
        if let Some((lo, hi)) = std::env::var("HWTR_WRITES_IN").ok().and_then(|v| {
            let (a, b) = v.split_once('-')?;
            Some((u32::from_str_radix(a, 16).ok()?, u32::from_str_radix(b, 16).ok()?))
        }) {
            let mut blocks: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, u64>> = Default::default();
            for &(pc, addr, _) in &log {
                let addr = addr & 0x1f_ffff | 0x8000_0000;
                if addr >= lo && addr < hi {
                    let f = a.func_of(pc).map_or(pc, |f| f.start);
                    *blocks.entry(addr & !63).or_default().entry(f).or_default() += 1;
                }
            }
            for (b, fs) in &blocks {
                let fs: Vec<String> = fs.iter().map(|(f, n)| format!("{f:08x}x{n}")).collect();
                println!("{b:08x}: {}", fs.join(" "));
            }
        }
        let mut rows: Vec<_> = by.into_iter().collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.1.0));
        println!("{:>8} {:>9}  {:>8}..{:<8} pages", "function", "stores", "low", "high");
        for (f, (n, lo, hi, pages)) in rows.iter().take(60) {
            println!("{f:08x} {n:>9}  {lo:08x}..{hi:08x} {}", pages.len());
        }
    }
    for &(a, n) in &peeks {
        for row in (0..n).step_by(16) {
            let words: Vec<String> = (0..4).map(|k| format!("{:08x}", hle.m.bus.read_u32(a + row + 4 * k))).collect();
            println!("{:08x}: {}", a + row, words.join(" "));
        }
    }
    if let Some(p) = &save {
        let bytes = hle.save();
        std::fs::write(p, &bytes).expect("write state");
        tracing::info!("saved {} ({} bytes)", p.display(), bytes.len());
    }
    // Where the game's flow is: fsm_main's current state.
    {
        let b = &mut hle.m.bus;
        let table = b.read_u32(0x800c_5bdc);
        let cur = b.read_u32(0x800c_5bdc + 8);
        let index = (0..349u32).find(|&i| b.read_u32(table + 4 * i) == cur);
        let phase = b.read(0x800c_5bdc + 0x14, 2).unwrap_or(0);
        tracing::info!("fsm_main: state {index:?}, phase {phase}");
    }
    let hw = hle.hw.borrow();
    png(&shots.join("vram"), 1024, 512, &hw.gpu.vram_rgba());
    tracing::info!(
        "{} frames in {:.1?}, {} CD reads, unhandled ports {:x?}",
        hle.frames.get(),
        start.elapsed(),
        hle.reads.borrow().len(),
        hw.unhandled
    );
}
