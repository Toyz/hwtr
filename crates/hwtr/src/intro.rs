//! What the boot executable (`SLUS_009.64`, 0x80101a40) shows before the
//! game: EA's logo movie, the attract movie (any button skips it once four
//! pictures are up), the legal screen (in over 16 frames, five seconds,
//! out), and the next screen faded in while the game loads.

use std::path::Path;

use rrt::image::Picture;

/// One step of the sequence.
enum Stage {
    /// A movie: its file, its codes, where its pictures are, whether a
    /// button stops it, and the picture shown.
    Movie { bytes: Vec<u8>, codes: hwtr_data::wve::Codes, skippable: bool, shown: Option<usize>, ticks: u32 },
    /// A picture fading in (0 to 128 by 8 a frame), held `hold` ms, then
    /// out (none when `hold` is 0: the game takes over).
    Still { picture: Picture, level: i32, held: u32, hold: u32, out: bool },
}

pub struct Intro {
    stages: Vec<Stage>,
    /// The picture to show this frame.
    pub picture: Option<Picture>,
    spu: Option<std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>>,
    started: bool,
    /// Ticks so far.
    ticks: u32,
}

/// A movie's 320 x 224 picture as RGBA.
fn rgba(rgb: &[u8], width: u16, height: u16) -> Picture {
    let rgba = rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    Picture { width: width as u32, height: height as u32, rgba }
}

impl Intro {
    pub fn load(cue: &Path, spu: Option<std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>>) -> Result<Intro, String> {
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let read = |p: &str| iso.find(p).and_then(|e| iso.read(&e)).map_err(|e| e.to_string());
        let slus = read("SLUS_009.64")?;
        let byte = |a: u32| a.checked_sub(0x8010_0000).and_then(|o| slus.get(o as usize + 0x800)).copied().unwrap_or(0);
        let mut stages = Vec::new();
        for (name, skippable) in [("EA_LOGO.WVE", false), ("ONLINE.WVE", true)] {
            let bytes = read(name)?;
            let codes = {
                let w = hwtr_data::wve::Wve::parse(&bytes).ok_or(format!("{name} does not parse"))?;
                hwtr_data::wve::Codes::new(&byte, &w.values).ok_or(format!("{name}'s codes do not build"))?
            };
            stages.push(Stage::Movie { bytes, codes, skippable, shown: None, ticks: 0 });
        }
        for (name, hold) in [("PSXLEGAL.TIM", 5000), ("PSXRFA1.TIM", 0)] {
            let tim = hwtr_data::Tim::parse(&read(name)?).map_err(|e| e.to_string())?;
            let picture = Picture { width: tim.width() as u32, height: tim.height() as u32, rgba: tim.to_rgba(0) };
            stages.push(Stage::Still { picture, level: 0, held: 0, hold, out: false });
        }
        stages.reverse();
        Ok(Intro { stages, picture: None, spu, started: false, ticks: 0 })
    }

    /// Whether the sequence is over.
    pub fn done(&self) -> bool {
        self.stages.is_empty()
    }

    /// One tick (a vertical blank, `ms` long), with any button `held`.
    pub fn tick(&mut self, held: bool, ms: u32) {
        let Some(stage) = self.stages.last_mut() else { return };
        self.ticks += 1;
        let mut next = false;
        match stage {
            Stage::Movie { bytes, codes, skippable, shown, ticks } => {
                let Some(w) = hwtr_data::wve::Wve::parse(bytes) else {
                    self.stages.pop();
                    return;
                };
                if !self.started {
                    self.started = true;
                    if let Some(spu) = &self.spu
                        && let Ok(mut s) = spu.lock()
                    {
                        s.play_stream(w.sound(), hwtr_data::wve::RATE);
                    }
                }
                // The pictures go by the sound's clock (0x80103f60 against
                // 0x80103f20), or by the ticks with no sound.
                *ticks += 1;
                let ms_in = match &self.spu {
                    Some(spu) => spu.lock().map_or(0, |s| s.stream_ms()),
                    None => *ticks as u64 * ms as u64,
                };
                let k = (ms_in * hwtr_data::wve::FPS as u64 / 1000) as usize;
                if k >= w.pictures.len() || (*skippable && shown.is_some_and(|s| s >= 4) && held) {
                    next = true;
                } else if *shown != Some(k) {
                    *shown = Some(k);
                    let p = &w.pictures[k];
                    if let Some(rgb) = hwtr_data::wve::decode(p, codes) {
                        self.picture = Some(rgba(&rgb, p.width, p.height));
                    }
                }
                if next
                    && let Some(spu) = &self.spu
                    && let Ok(mut s) = spu.lock()
                {
                    s.stop_stream();
                }
            }
            Stage::Still { picture, level, held: held_ms, hold, out } => {
                if !*out {
                    if *level < 128 {
                        *level += 8;
                    } else if *hold == 0 {
                        next = true;
                    } else {
                        *held_ms += ms;
                        if *held_ms > *hold {
                            *out = true;
                        }
                    }
                } else {
                    *level -= 8;
                    if *level < 0 {
                        next = true;
                    }
                }
                let k = (*level).clamp(0, 128) as u32;
                let rgba = picture
                    .rgba
                    .chunks_exact(4)
                    .flat_map(|p| {
                        [
                            (p[0] as u32 * k / 128) as u8,
                            (p[1] as u32 * k / 128) as u8,
                            (p[2] as u32 * k / 128) as u8,
                            255,
                        ]
                    })
                    .collect();
                self.picture = Some(Picture { width: picture.width, height: picture.height, rgba });
            }
        }
        if next {
            rrt::tracing::debug!("intro stage over at tick {}", self.ticks);
            self.stages.pop();
            self.started = false;
        }
    }
}
