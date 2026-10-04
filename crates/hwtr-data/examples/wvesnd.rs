//! A movie's sound as WAV, and how smooth each channel is: `wvesnd FILE.WVE OUT.wav`.
fn main() {
    let mut args = std::env::args().skip(1);
    let b = std::fs::read(args.next().unwrap()).unwrap();
    let w = hwtr_data::wve::Wve::parse(&b).unwrap();
    let s = w.sound();
    let jump = |c: usize| s.chunks(2).collect::<Vec<_>>().windows(2).map(|p| (p[1][c] as i64 - p[0][c] as i64).abs()).sum::<i64>() / (s.len() as i64 / 2);
    let peak = s.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    println!("{} frames, mean step L {} R {}, peak {}", s.len() / 2, jump(0), jump(1), peak);
    // Stereo WAV.
    let mut out = Vec::new();
    let len = (s.len() * 2) as u32;
    out.extend(b"RIFF"); out.extend((36 + len).to_le_bytes()); out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes()); out.extend(1u16.to_le_bytes()); out.extend(2u16.to_le_bytes());
    out.extend(22050u32.to_le_bytes()); out.extend((22050u32 * 4).to_le_bytes()); out.extend(4u16.to_le_bytes()); out.extend(16u16.to_le_bytes());
    out.extend(b"data"); out.extend(len.to_le_bytes());
    for v in &s { out.extend(v.to_le_bytes()); }
    std::fs::write(args.next().unwrap(), out).unwrap();
}
