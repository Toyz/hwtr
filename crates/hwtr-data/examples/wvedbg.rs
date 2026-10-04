//! Writes some of a movie's pictures as PPM: `wvedbg FILE.WVE OUTDIR`.
fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().unwrap_or("work/fs/EA_LOGO.WVE".into());
    let out = std::path::PathBuf::from(args.next().unwrap_or("/tmp".into()));
    let b = std::fs::read(&file).unwrap();
    let slus = std::fs::read("work/fs/SLUS_009.64").unwrap();
    let byte = |a: u32| a.checked_sub(0x8010_0000).and_then(|o| slus.get(o as usize + 0x800)).copied().unwrap_or(0);
    let w = hwtr_data::wve::Wve::parse(&b).unwrap();
    let codes = hwtr_data::wve::Codes::new(&byte, &w.values).unwrap();
    for (k, p) in w.pictures.iter().enumerate() {
        println!("pic {} reach {:?}", p.number, hwtr_data::wve::decode_reach(p, &codes));
        if k % 15 == 0 {
            if let Some(rgb) = hwtr_data::wve::decode(p, &codes) {
                let mut f = format!("P6\n{} {}\n255\n", p.width, p.height).into_bytes();
                f.extend(rgb);
                std::fs::write(out.join(format!("p{:03}.ppm", k)), f).unwrap();
            }
        }
    }
}
