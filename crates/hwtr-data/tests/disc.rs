//! Every file of each format on the disc parses. Needs `hwtr-re big extract`
//! to have filled work/big; skipped otherwise.

use std::path::PathBuf;

fn big_dir() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big");
    if d.is_dir() {
        Some(d)
    } else {
        eprintln!("skipped: {} not found (run `hwtr-re big extract`)", d.display());
        None
    }
}

/// Every extracted member whose name ends with `ext`, sorted.
fn members(ext: &str) -> Vec<PathBuf> {
    let Some(dir) = big_dir() else { return Vec::new() };
    let mut out = Vec::new();
    for arch in std::fs::read_dir(dir).unwrap().flatten() {
        for f in std::fs::read_dir(arch.path()).unwrap().flatten() {
            if f.file_name().to_string_lossy().ends_with(ext) {
                out.push(f.path());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn worlds_parse() {
    use hwtr_data::world::World;
    let mut n = 0;
    for ext in ["WLD", "DLW", "WLB"] {
        for p in members(ext) {
            let w = World::parse(&std::fs::read(&p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            if ext == "WLB" {
                assert_eq!(w.grid_w * w.grid_h, 0, "{}: the sky has no grid", p.display());
                assert_eq!(w.flags & 1, 1);
            } else {
                assert!(w.poly_count() > 1000, "{}", p.display());
                // The bounds hold the grid: (w - 1) * 1024 < max - origin <= w * 1024.
                let span = (w.max[0] - w.origin[0], w.max[1] - w.origin[1]);
                assert!(span.0 > (w.grid_w as i32 - 1) * 1024 && span.0 <= w.grid_w as i32 * 1024, "{}", p.display());
                assert!(span.1 > (w.grid_h as i32 - 1) * 1024 && span.1 <= w.grid_h as i32 * 1024, "{}", p.display());
            }
            n += 1;
        }
    }
    if n > 0 {
        assert_eq!(n, 33);
    }
}

#[test]
fn texture_blocks_parse() {
    let mut n = 0;
    for ext in ["GLM", "GLB"] {
        for p in members(ext) {
            if p.ends_with("SCREENSGLM") {
                continue; // a different format: a list of TIMs
            }
            let blocks = hwtr_data::world::read_vram_blocks(&std::fs::read(&p).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert_eq!(blocks.len(), 2, "{}", p.display());
            n += 1;
        }
    }
    if n > 0 {
        assert_eq!(n, 33);
    }
}

#[test]
fn cars_parse() {
    use hwtr_data::car::{CarBmf, Model, cwh_blocks, fxp_counts};
    let mut n = 0;
    for p in members("BMF") {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&p).unwrap();
        if name == "DECALSBMF" {
            let parts = hwtr_data::car::bmf_parts(&bytes).unwrap();
            assert_eq!(parts.len(), 41);
            for part in parts {
                hwtr_data::car::unpack_decal(part).unwrap();
            }
            continue;
        }
        if name == "CWHSBMF" {
            let parts = hwtr_data::car::bmf_parts(&bytes).unwrap();
            assert_eq!(parts.len(), 41);
            for part in parts {
                cwh_blocks(part).unwrap();
            }
            continue;
        }
        let bmf = CarBmf::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let models: Vec<Model> =
            bmf.models.iter().map(|m| Model::parse(m).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).collect();
        // Full detail has the wheels and the most faces; then medium, then low.
        assert!(!models[0].children.is_empty() && models[1].children.is_empty() && models[2].children.is_empty());
        assert!(models[0].face_count() > models[1].face_count(), "{}", p.display());
        assert!(models[1].face_count() > models[2].face_count(), "{}", p.display());
        cwh_blocks(bmf.cwh).unwrap();
        fxp_counts(bmf.fxp).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        // The CAR file beside it is part 0.
        let car = p.with_file_name(format!("{}CAR", &name[..name.len() - 3]));
        assert_eq!(std::fs::read(&car).unwrap(), bmf.models[0], "{}", car.display());
        n += 1;
    }
    if n > 0 {
        assert_eq!(n, 492);
    }
}
