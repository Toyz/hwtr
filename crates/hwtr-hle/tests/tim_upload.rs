//! tim_upload (0x80012a10) under cheat 8, the flat cars: the image it
//! fills before the upload, against `flat_skin`.

use hwtr_hle::original::Ram;

const TIM: u32 = 0x801c_0000;
const INFO: u32 = 0x801b_ff00;
/// The upload's fill is on (0x800d244c) and the race's cheats (0x800d2468).
const FILL_ON: u32 = 0x800d_244c;
const CHEATS: u32 = 0x800d_2468;
/// Draw tables still out (0x800d23d4), which the upload waits for first
/// (0x800117e0) on a clock that does not run inside a call.
const TABLES_OUT: u32 = 0x800d_23d4;

#[test]
fn the_flat_cars_fill_as_the_original() {
    // The upload waits on the GPU, so the whole machine, not the bare CPU.
    let Some(cue) = rrt::disc::Image::find(std::path::Path::new("../../work/disc")).ok() else {
        eprintln!("skipped: no disc");
        return;
    };
    let mut hle = hwtr_hle::Hle::new(std::rc::Rc::new(rrt::disc::Image::open(&cue).unwrap())).unwrap();
    let Ok(state) = std::fs::read("../../work/states/desert1-race.bin") else {
        eprintln!("skipped: no desert1-race state");
        return;
    };
    hle.load(&state).unwrap();
    hle.m.step_limit = 100_000_000;
    let m = &mut hle.m;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big/DESERT1BIG/DEORATIM");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipped: {} not found", path.display());
        return;
    };
    let tim = hwtr_data::Tim::parse(&bytes).unwrap();
    // The image's halfwords start after the header, the palette block and
    // the image block's own 12 bytes.
    let clut = if bytes[4] & 8 != 0 { u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize } else { 0 };
    let at = 8 + clut + 12;
    let start = m.bus.ram.clone();
    for (depth, keep, cheats, fill_on) in
        [(1, 0, 8, 1), (0, 0, 8, 1), (2, 0, 8, 1), (1, 1, 8, 1), (1, 0, 0, 1), (1, 0, 8, 0), (1, 0, 0xff, 1)]
    {
        m.bus.ram.copy_from_slice(&start);
        let mut ram = Ram(&mut m.bus.ram);
        for (k, &b) in bytes.iter().enumerate() {
            ram.set_u8(TIM + k as u32, b);
        }
        ram.set_u8(FILL_ON, fill_on);
        ram.set_u8(CHEATS, cheats);
        ram.set_i32(TABLES_OUT, 0);
        m.call(0x8001_2a10, &[TIM, bytes.len() as u32, INFO, depth, 640, 0, 0, 480, keep]).unwrap();
        let ram = Ram(&mut m.bus.ram);
        let theirs: Vec<u16> = (0..tim.data.len())
            .map(|k| u16::from_le_bytes([ram.u8(TIM + (at + 2 * k) as u32), ram.u8(TIM + (at + 2 * k + 1) as u32)]))
            .collect();
        let mut ours = tim.data.clone();
        if fill_on != 0 && cheats & 8 != 0 && keep == 0 {
            hwtr_game::car::draw::flat_skin(&mut ours, depth);
        }
        assert_eq!(theirs, ours, "depth {depth} keep {keep} cheats {cheats:#x} fill {fill_on}");
    }
    let mut flat = tim.data.clone();
    hwtr_game::car::draw::flat_skin(&mut flat, 1);
    assert_ne!(flat, tim.data, "the skin is not one colour already");
}
