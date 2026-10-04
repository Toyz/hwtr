//! hwtr: Hot Wheels Turbo Racing on the PC, the port and only the port.
//!
//! Everything it runs is ported Rust code; the original game, run in the
//! interpreter, stays in `hwtr-hle` and the tests, where the port is checked
//! against it. The window, the loop, the pad and the GPU plumbing are
//! [retro_rt](https://github.com/Toyz/retro_rt)'s.
//!
//! What it does so far: the boot executable's intro (EA's logo movie, the
//! attract movie, which any button skips, and the legal screens), then the
//! front end as the game runs it: the title, the main menu (the car, the
//! track and the mode are chosen there), and from it a race with the cars
//! and track chosen; Start pauses it (continue, restart, abort), and when
//! the race is over the menu comes back. In the race the track and the cars
//! are drawn natively, with a camera behind the player's car.
//! The player's car drives on the ported physics and collision, through the
//! original's controller read, as a digital pad by default (the d-pad
//! steers, Cross accelerates, Square brakes, L2 is the handbrake; on the
//! keyboard the arrows, Z, A and 1), or with `--analog` a DualShock in
//! analog mode (the left stick steers, the right stick accelerates and
//! brakes); the computer cars drive their routes.
//!
//! ```text
//! hwtr [--cue DISC.cue] [--track NAME] [--analog] [--shot OUT.png [--frames N] [--press SCRIPT] [--intro] [--race-frames N]]
//! ```
//!
//! `--track` goes straight to a race on that track (DESERT1 by default);
//! `--shot` runs that race with no window for `--frames` frames (60 a second)
//! with the pad driven by `--press` (`FRAME:BUTTONS[:FRAMES],...`; by default
//! Cross held throughout), and writes the last frame as PNG; it starts at
//! the front end, without the intro, unless `--intro` is given. With
//! `--race-frames N` each race of a shot ends after N frames as if run in
//! full with the player first, to reach the screens that follow a race.

#![forbid(unsafe_code)]

mod cd;
mod front;
mod intro;
mod pieces;
mod race;
mod spu;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rrt::prelude::*;
use rrt::wgpu;

use front::FrontEnd;
use hwtr_game::front::RaceEnd;
use hwtr_game::pad::PadState;
use race::Race;

/// What the original's clock adds each vertical blank (0x8001224c): the
/// race runs on it, a little ahead of real time, as it did on the console.
const VBLANK_MS: Duration = Duration::from_millis(17);

struct Hwtr {
    /// The boot executable's movies and legal screens, until they are over.
    intro: Option<intro::Intro>,
    front: Option<FrontEnd>,
    cue: PathBuf,
    track: String,
    /// The race under way, and whether it came from the front end.
    race: Option<Race>,
    from_front: bool,
    /// The motors are to be stopped at the next tick; ticks left of a
    /// front-end buzz (both motors full, half a second, 0x8001d5a8).
    stop_motors: bool,
    buzz_ticks: u32,
    /// The pad in analog mode.
    analog: bool,
    /// The sound chip, playing on the default output while `audio` lives
    /// (none when there is no device, or with no window).
    spu: Option<std::sync::Arc<std::sync::Mutex<spu::Spu>>>,
    _audio: Option<rrt::audio::Output>,
    /// The CD's music, with sound.
    cd: Option<cd::CdPlayer>,
    /// Shots only: races end after this many frames, the player first; and
    /// the frames the race under way has run.
    race_frames: Option<u32>,
    race_ran: u32,
    /// The window's format without sRGB: the game's colours are already
    /// display-encoded.
    plain_format: wgpu::TextureFormat,
}

impl Hwtr {
    fn title(&self) -> String {
        match &self.race {
            Some(_) => format!("hwtr - {}", self.track),
            None => "hwtr".to_string(),
        }
    }

    fn start_race(&mut self) {
        match Race::load(&self.cue, &self.track, self.spu.clone()) {
            Ok(mut r) => {
                r.analog = self.analog;
                // The settings' defaults (0x80088544).
                let s = hwtr_game::front::Settings::new(0);
                r.set_volumes(hwtr_game::pause::Volumes { effects: s.volume, music: s.music, voice: s.effects });
                self.race = Some(r);
                self.from_front = false;
            }
            Err(e) => rrt::tracing::error!("{e}"),
        }
    }

    /// The front end, loaded the first time it is needed.
    fn front(&mut self) -> Option<&mut FrontEnd> {
        if self.front.is_none() {
            match FrontEnd::load(&self.cue, self.spu.clone()) {
                Ok(f) => self.front = Some(f),
                Err(e) => rrt::tracing::error!("{e}"),
            }
        }
        self.front.as_mut()
    }

    /// The race the front end set up, if it has.
    fn race_from_front(&mut self) {
        let Some(setup) = self.front.as_mut().and_then(|f| f.front.race.take()) else { return };
        self.track = format!("{}{}", setup.track.to_uppercase(), setup.track_number);
        rrt::tracing::info!("race: {} with {:?}", self.track, setup.cars.iter().map(|c| &c.name).collect::<Vec<_>>());
        let volume = self.front.as_ref().map_or(200, |f| f.front.settings.volume);
        match Race::load_setup(&self.cue, setup, self.spu.clone(), volume) {
            Ok(mut r) => {
                r.analog = self.analog;
                if let Some(f) = &self.front {
                    r.set_mono(f.front.settings.other[1] == 0);
                    r.set_mapping(f.front.mappings[0]);
                    let s = f.front.settings;
                    r.set_volumes(hwtr_game::pause::Volumes { effects: s.volume, music: s.music, voice: s.effects });
                }
                if let Some(cd) = &mut self.cd {
                    r.set_song(cd.song);
                    if let Some(f) = &self.front {
                        cd.set_volume(f.front.settings.music);
                    }
                }
                self.race = Some(r);
                self.from_front = true;
            }
            Err(e) => {
                rrt::tracing::error!("{e}");
                if let Some(f) = self.front.as_mut() {
                    f.front.race_over(hwtr_game::front::RaceResult::ended(RaceEnd::Other(0)));
                }
            }
        }
    }

    /// The race is over, how it ended: back to the front end.
    fn end_race(&mut self, end: hwtr_game::front::RaceResult) {
        let volumes = self.race.take().map(|r| r.volumes());
        self.stop_motors = true;
        if self.from_front {
            if let Some(f) = self.front.as_mut() {
                // 0x8008abac: the volumes the pause menu set kept.
                if let Some(v) = volumes {
                    f.front.settings.volume = v.effects;
                    f.front.settings.music = v.music;
                    f.front.settings.effects = v.voice;
                }
                f.front.race_over(end);
            }
        } else {
            self.intro = None;
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
        if std::mem::take(&mut self.stop_motors) {
            t.rumble(rrt::input::Motors::default());
        }
        if let Some(race) = &mut self.race {
            race.frame(&t.pad, VBLANK_MS);
            self.race_ran += 1;
            for ask in race.take_cd() {
                if let Some(cd) = &mut self.cd {
                    cd.ask(ask);
                }
            }
            t.rumble(race.motors());
            // 0x8009b1e0: the front end watches the attract race for a
            // button; leaving, it lets the race go (0x8009b510).
            if self.from_front && race.demo() {
                let pad = PadState { buttons: t.pad.buttons.bits(), ..PadState::default() };
                if let Some(f) = self.front.as_mut() {
                    f.tick(pad, VBLANK_MS.as_millis() as u32);
                    for ask in std::mem::take(&mut f.front.cd) {
                        if let Some(cd) = &mut self.cd {
                            cd.ask(ask);
                        }
                    }
                    if std::mem::take(&mut f.front.race_dropped) {
                        self.race = None;
                        self.race_ran = 0;
                        self.stop_motors = true;
                        t.set_title(self.title());
                        return;
                    }
                }
            }
            let cut_short = self.race_frames.is_some_and(|n| self.race_ran >= n);
            if race.over() || cut_short {
                let result = if cut_short { race.run_in_full() } else { race.result() };
                self.race_ran = 0;
                self.end_race(result);
                t.set_title(self.title());
            }
            return;
        }
        if let Some(intro) = &mut self.intro {
            intro.tick(!t.pad.buttons.is_empty(), VBLANK_MS.as_millis() as u32);
            if !intro.done() {
                return;
            }
            // On to the front end in the same tick, so no frame goes by
            // with nothing to draw.
            self.intro = None;
        }
        let pad = PadState { buttons: t.pad.buttons.bits(), ..PadState::default() };
        if let Some(f) = self.front() {
            f.tick(pad, VBLANK_MS.as_millis() as u32);
            if f.front.buzz.take() == Some(0) {
                self.buzz_ticks = (500 / VBLANK_MS.as_millis()) as u32;
            }
        }
        if self.buzz_ticks > 0 {
            self.buzz_ticks -= 1;
            t.rumble(rrt::input::Motors { small: true, large: 255 });
            if self.buzz_ticks == 0 {
                self.stop_motors = true;
            }
        }
        let asks = self.front.as_mut().map(|f| std::mem::take(&mut f.front.cd)).unwrap_or_default();
        for ask in asks {
            if let Some(cd) = &mut self.cd {
                cd.ask(ask);
            }
        }
        self.race_from_front();
        if self.race.is_some() {
            t.set_title(self.title());
        }
    }

    fn draw(&mut self, d: &mut Draw<'_>) {
        if let Some(race) = &mut self.race {
            let commands = race.draw(&d.gpu.device, &d.gpu.queue, self.plain_format, d.plain, (d.width, d.height));
            d.submit(commands);
        } else if let Some(picture) = self.intro.as_ref().and_then(|i| i.picture.as_ref()) {
            d.present_picture(picture, None);
        } else if let Some(f) = &mut self.front {
            let commands = f.draw(&d.gpu.device, &d.gpu.queue, self.plain_format, d.plain, (d.width, d.height));
            d.submit(commands);
        }
    }
}

fn main() {
    let mut config = Config::default().title("hwtr").hz(rrt::app::rate::NTSC);
    let mut args = std::env::args().skip(1);
    let (mut cue, mut track, mut shot, mut frames, mut press) = (None, None, None, 0u64, None);
    let mut analog = false;
    let mut with_intro = false;
    let mut race_frames = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cue" => cue = args.next().map(PathBuf::from),
            "--track" => track = args.next(),
            "--analog" => analog = true,
            "--shot" => shot = args.next().map(PathBuf::from),
            "--frames" => frames = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            "--press" => press = args.next(),
            "--intro" => with_intro = true,
            "--race-frames" => race_frames = args.next().and_then(|s| s.parse().ok()),
            _ => {
                eprintln!(
                    "usage: hwtr [--cue DISC.cue] [--track NAME] [--analog] [--shot OUT.png [--frames N] [--press SCRIPT] [--intro] [--race-frames N]]"
                );
                std::process::exit(2);
            }
        }
    }
    let race = track.is_some();
    let shot_mode = shot.is_some();
    let track = track.unwrap_or_else(|| "DESERT1".into()).to_uppercase();
    let make = move || -> Result<Hwtr, String> {
        let cue = match cue {
            Some(c) => c,
            None => rrt::disc::Image::find(Path::new("work/disc")).map_err(|e| e.to_string())?,
        };
        // Sound only with a window.
        let (spu, audio) = if shot_mode {
            (None, None)
        } else {
            let spu = std::sync::Arc::new(std::sync::Mutex::new(spu::Spu::default()));
            match rrt::audio::Output::open(spu.clone()) {
                Ok(out) => (Some(spu), Some(out)),
                Err(e) => {
                    rrt::tracing::warn!("no sound: {e}");
                    (None, None)
                }
            }
        };
        let intro =
            if race || (shot_mode && !with_intro) { None } else { Some(intro::Intro::load(&cue, spu.clone())?) };
        let cd = match &spu {
            Some(s) => cd::CdPlayer::new(&cue, s.clone()).map_err(|e| rrt::tracing::warn!("no CD music: {e}")).ok(),
            None => None,
        };
        let mut game = Hwtr {
            cd,
            spu,
            _audio: audio,
            intro,
            front: None,
            cue,
            track,
            race: None,
            from_front: false,
            stop_motors: false,
            buzz_ticks: 0,
            analog,
            race_frames: race_frames.filter(|_| shot_mode),
            race_ran: 0,
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
