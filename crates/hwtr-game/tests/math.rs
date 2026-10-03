//! The trigonometry against 0x80010afc (cos) and 0x80010adc (sin).

mod common;

use hwtr_cpu::Machine;
use hwtr_game::math::Tables;

#[test]
fn cos_and_sin_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut m = Machine::with_exe(&exe);
    let mut rng = common::Rng(0x1234_5678_9abc_def0);
    // Every angle in two turns either way, then random ones across i32.
    let mut inputs: Vec<i32> = (-2 * 25736..=2 * 25736).step_by(7).collect();
    inputs.extend([0, 1, -1, 6434, 6435, 12868, 12869, 25735, 25736, 25737, i32::MAX, i32::MIN + 1]);
    inputs.extend((0..20_000).map(|_| rng.word() as i32));
    for x in inputs {
        let cos = m.call(0x8001_0afc, &[x as u32]).unwrap() as i32;
        assert_eq!(t.cos(x), cos, "cos({x})");
        let sin = m.call(0x8001_0adc, &[x as u32]).unwrap() as i32;
        assert_eq!(t.sin(x), sin, "sin({x})");
    }
}

#[test]
fn length_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut m = Machine::with_exe(&exe);
    let mut rng = common::Rng(0x0bad_cafe_d00d_f00d);
    let at = 0x8018_0000;
    let mut cases: Vec<[i32; 3]> = vec![[0, 0, 0], [1, 0, 0], [0, -1, 0], [3, 4, 0], [i32::MAX, 0, 0], [i32::MIN, i32::MIN, i32::MIN]];
    for _ in 0..30_000 {
        // Magnitudes spread over every bit length.
        let bits = rng.below(32);
        let mut c = || (rng.word() as i32) >> (31 - bits.min(31));
        cases.push([c(), c(), c()]);
    }
    for v in cases {
        for (k, c) in v.iter().enumerate() {
            m.bus.write_u32(at + 4 * k as u32, *c as u32);
        }
        let want = m.call(0x8002_6650, &[at]).unwrap() as i32;
        assert_eq!(t.length(v), want, "length({v:?})");
    }
}

#[test]
fn apply_matrix_lv_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut m = Machine::with_exe(&exe);
    let mut rng = common::Rng(0x005e_ed0f_ab1e);
    let (mat_at, v_at, out_at) = (0x8018_0000, 0x8018_0040, 0x8018_0080);
    for _ in 0..20_000 {
        let mut mat = [[0i16; 3]; 3];
        for row in &mut mat {
            for x in row.iter_mut() {
                *x = rng.word() as i16;
            }
        }
        let bits = rng.below(32);
        let v = [0; 3].map(|_: i32| (rng.word() as i32) >> (31 - bits.min(31)));
        // A MATRIX is nine halfwords and a pad, then the translation.
        for (k, x) in mat.iter().flatten().enumerate() {
            m.bus.write(mat_at + 2 * k as u32, 2, *x as u16 as u32);
        }
        for (k, x) in v.iter().enumerate() {
            m.bus.write_u32(v_at + 4 * k as u32, *x as u32);
        }
        m.call(0x800a_2ba8, &[mat_at, v_at, out_at]).unwrap();
        let want = [0, 1, 2].map(|k| m.bus.read_u32(out_at + 4 * k) as i32);
        assert_eq!(hwtr_game::math::apply_matrix_lv(&mat, v), want, "{mat:?} {v:?}");
    }
}
