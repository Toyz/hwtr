//! hwtr: Hot Wheels Turbo Racing on the PC, the port and only the port.
//!
//! Everything it runs is ported Rust code; the original game, run in the
//! interpreter, stays in `hwtr-hle` and the tests, where the port is checked
//! against it. The window, the loop, the pad and the GPU plumbing are
//! [retro_rt](https://github.com/Toyz/retro_rt)'s.
//!
//! What it does so far: the boot screens from the disc (Start or Cross steps
//! through them), then the race: the track and the cars on the start grid,
//! drawn natively, with a camera behind the player's car (Select goes back).
//! The player's car drives on the ported physics and collision, through the
//! original's controller read: with a gamepad, a DualShock in analog mode
//! (left stick steers, right stick accelerates and brakes); with the
//! keyboard, a digital pad (the arrows steer, Z accelerates, A brakes, 1 is
//! the handbrake). The other cars wait until computer cars are ported.
//!
//! ```text
//! hwtr [--cue DISC.cue] [--track NAME] [--shot OUT.png [--frames N] [--press SCRIPT]]
//! ```
//!
//! `--track` goes straight to a race on that track (DESERT1 by default);
//! `--shot` runs that race with no window for `--frames` frames (60 a second)
//! with the pad driven by `--press` (`FRAME:BUTTONS[:FRAMES],...`; by default
//! Cross held throughout), and writes the last frame as PNG.

#![forbid(unsafe_code)]

mod race;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rrt::prelude::*;
use rrt::wgpu;

use race::Race;

/// What the original's clock adds each vertical blank (0x8001224c): the
/// race runs on it, a little ahead of real time, as it did on the console.
const VBLANK_MS: Duration = Duration::from_millis(17);

struct Hwtr {
    screens: Vec<(String, Picture)>,
    screen: usize,
    cue: PathBuf,
    track: String,
    /// The race, once the boot screens are through.
    race: Option<Race>,
    /// The window's format without sRGB: the game's colours are already
    /// display-encoded.
    plain_format: wgpu::TextureFormat,
}

impl Hwtr {
    fn title(&self) -> String {
        match &self.race {
            Some(_) => format!("hwtr - {}", self.track),
            None => format!("hwtr - {}", self.screens[self.screen].0),
        }
    }

    fn start_race(&mut self) {
        match Race::load(&self.cue, &self.track) {
            Ok(r) => self.race = Some(r),
            Err(e) => rrt::tracing::error!("{e}"),
        }
    }
}

impl Game for Hwtr {
    fn init(&mut self, ctx: &mut Init<'_>) {
        self.plain_format = ctx.plain_format;
        if let Some(window) = ctx.window {
            window.set_title(&self.title());
        }
    }

    fn tick(&mut self, t: &mut Tick) {
        let pressed = t.pressed();
        if let Some(race) = &mut self.race {
            if pressed.contains(Buttons::SELECT) {
                self.race = None;
                self.screen = 0;
                t.set_title(self.title());
                return;
            }
            race.frame(&t.pad, VBLANK_MS);
        } else if pressed.intersects(Buttons::CROSS | Buttons::START) {
            if self.screen + 1 == self.screens.len() {
                self.start_race();
            } else {
                self.screen += 1;
            }
            t.set_title(self.title());
        }
    }

    fn draw(&mut self, d: &mut Draw<'_>) {
        if let Some(race) = &mut self.race {
            let commands = race.draw(&d.gpu.device, &d.gpu.queue, self.plain_format, d.plain, (d.width, d.height));
            d.submit(commands);
        } else {
            d.present_picture(&self.screens[self.screen].1, None);
        }
    }
}

/// The boot screens, read off the disc: the legal screen, the title, the
/// main menu's background.
fn screens(cue: &Path) -> Result<Vec<(String, Picture)>, String> {
    let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
    let iso = disc.iso().map_err(|e| e.to_string())?;
    let read = |p: &str| iso.find(p).and_then(|e| iso.read(&e)).map_err(|e| e.to_string());
    let big = read("CCCPSX.BIG")?;
    let big = hwtr_data::Big::parse(&big).map_err(|e| e.to_string())?;
    let pic = |name: &str, bytes: &[u8]| -> Result<(String, Picture), String> {
        let tim = hwtr_data::Tim::parse(bytes).map_err(|e| e.to_string())?;
        Ok((name.to_string(), Picture { width: tim.width() as u32, height: tim.height() as u32, rgba: tim.to_rgba(0) }))
    };
    let mut out = vec![pic("PSXLEGAL.TIM", &read("PSXLEGAL.TIM")?)?];
    for name in ["PSXRFA1TIM", "PSXMAIN1TIM", "PSXLOAD1TIM"] {
        let data = big.lookup(&format!("SCREENSBIG/{name}")).ok_or(format!("{name} missing from SCREENS.BIG"))?;
        out.push(pic(name, data)?);
    }
    Ok(out)
}

fn main() {
    let mut config = Config::default().title("hwtr").hz(rrt::app::rate::NTSC);
    let mut args = std::env::args().skip(1);
    let (mut cue, mut track, mut shot, mut frames, mut press) = (None, None, None, 0u64, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cue" => cue = args.next().map(PathBuf::from),
            "--track" => track = args.next(),
            "--shot" => shot = args.next().map(PathBuf::from),
            "--frames" => frames = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            "--press" => press = args.next(),
            _ => {
                eprintln!("usage: hwtr [--cue DISC.cue] [--track NAME] [--shot OUT.png [--frames N] [--press SCRIPT]]");
                std::process::exit(2);
            }
        }
    }
    let race = track.is_some() || shot.is_some();
    let track = track.unwrap_or_else(|| "DESERT1".into()).to_uppercase();
    let make = move || -> Result<Hwtr, String> {
        let cue = match cue {
            Some(c) => c,
            None => rrt::disc::Image::find(Path::new("work/disc")).map_err(|e| e.to_string())?,
        };
        let mut game = Hwtr {
            screens: screens(&cue)?,
            screen: 0,
            cue,
            track,
            race: None,
            plain_format: wgpu::TextureFormat::Rgba8Unorm,
        };
        if race {
            game.start_race();
        }
        Ok(game)
    };
    let Some(out) = shot else {
        rrt::app::launch(config, make);
        return;
    };
    rrt::app::init_logging(&config.log);
    let script = press.unwrap_or_else(|| format!("0:cross:{}", frames.max(1)));
    let written = rrt::input::Script::parse(&script)
        .map_err(|e| e.to_string())
        .and_then(|script| {
            config = config.clone().script(script);
            make()
        })
        .and_then(|mut game| {
            let picture = rrt::app::headless(&mut game, &config, frames, 1280, 960).map_err(|e| e.0)?;
            std::fs::write(&out, picture.to_png()).map_err(|e| e.to_string())?;
            Ok(game.race.as_ref().map(Race::position))
        });
    match written {
        Ok(pos) => rrt::tracing::info!("-> {} (after {frames} frames the car is at {pos:?})", out.display()),
        Err(e) => {
            rrt::tracing::error!("{e}");
            std::process::exit(1);
        }
    }
}
