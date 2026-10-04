//! The race, as far as it is ported: the track and cars from the disc, the
//! player's car driven by the ported game logic, a camera behind it.

use std::path::Path;
use std::time::Duration;

use hwtr_game::car::{Tuning, handling};
use hwtr_game::collision::Scp;
use hwtr_game::hud::{Font, Sprite};
use hwtr_game::math::Tables;
use hwtr_game::pad::{ACCEPT, Mapping, PadKind, PadReader, PadState, START};
use hwtr_game::race::{Buttons, Driver, Entrant, RaceSetup, STEP_MS};
use hwtr_render::scene::Layout;
use hwtr_render::{Renderer, Scene};
use rrt::glam::{self, Mat3, Mat4, Quat, Vec3};
use rrt::input::Pad;
use rrt::wgpu;

/// The cars on the grid, the player's first.
const CARS: [&str; 6] = ["deora", "twinmill", "rocket", "bisector", "snake", "hw500"];

/// The view-projection from a game camera: at its position, looking along
/// its forward axis with its up axis up; its field of view across the
/// screen (the game's near plane, 10 inches, and a far one past the track).
fn camera_matrix(eye: Vec3, rot: Mat3, fov: f32, aspect: f32) -> Mat4 {
    let tall = 2.0 * ((fov / 2.0).tan() / aspect).atan();
    let view = glam::camera::rh::view::look_to_mat4(eye, rot.y_axis, rot.z_axis);
    rrt::gpu::projection(tall, aspect, 10.0, 300_000.0) * view
}

/// A 4.12 rotation's columns as axes.
fn axes(rot: &hwtr_game::math::Matrix) -> Mat3 {
    let col = |j: usize| Vec3::from_array(rot.map(|row| row[j] as f32 / 4096.0));
    Mat3::from_cols(col(0), col(1), col(2))
}

fn world(v: hwtr_game::math::Vec3) -> Vec3 {
    Vec3::from_array(v.map(|c| c as f32 / 4096.0))
}

/// What is drawn of a race step: each car's pose and the camera's.
#[derive(Clone)]
struct Shown {
    cars: Vec<(Vec3, Quat)>,
    eye: Vec3,
    look: Quat,
    fov: f32,
}

impl Shown {
    /// Between `self` (a step back) and `now`, `t` of the way.
    fn toward(&self, now: &Shown, t: f32) -> Shown {
        Shown {
            cars: self.cars.iter().zip(&now.cars).map(|(a, b)| (a.0.lerp(b.0, t), a.1.slerp(b.1, t))).collect(),
            eye: self.eye.lerp(now.eye, t),
            look: self.look.slerp(now.look, t),
            fov: self.fov + (now.fov - self.fov) * t,
        }
    }
}

pub struct Race {
    scene: Scene,
    race: hwtr_game::race::Race,
    renderer: Option<(Renderer, wgpu::TextureFormat)>,
    /// How far the race's clock runs ahead of the real one, milliseconds
    /// (race_frame, 0x80033ed8, steps while it is behind).
    ahead: i64,
    /// Milliseconds since the pad was last read.
    since_read: u32,
    /// Microseconds of real time not yet counted as whole milliseconds.
    carry_us: u32,
    /// The race as drawn a step before its last, and how far between the two
    /// the real clock is: the game steps 40 times a second, the display at
    /// its own rate, so what is drawn is eased between steps (the game's
    /// logic is untouched).
    before: Option<Shown>,
    /// The effects' quads for this frame.
    effects: Vec<hwtr_game::effects::EffectQuad>,
    between: f32,
    /// The countdown's number, stepped once a game frame (a thirtieth of a
    /// second, the race's usual rate on the console: two vertical blanks
    /// a frame; the original's follows its drawing), and the time toward
    /// the next step.
    count: Option<hwtr_game::hud::CountSprite>,
    count_ms: u32,
    /// Player one's controller, read once a frame as the game reads it.
    reader: PadReader,
    mapping: Mapping,
    /// The pad in analog mode (the DualShock's ANALOG light on).
    pub analog: bool,
    /// The HUD's fonts, each with the texture page and palette its glyphs
    /// draw with (as `Vtx::mode`).
    fonts: Vec<(Font, u32)>,
    /// The race's effects, if there is sound.
    sound: Option<RaceSound>,
}

/// The race's effects (bank 0, `MAINSFX2`, from the track's archive), the
/// sound chip, and the effects volume.
struct RaceSound {
    spu: std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>,
    effects: crate::spu::Effects,
    volume: i32,
    /// The effects volume in 127ths (0x800d24c8), which the engines are
    /// heard at.
    effects_volume: i32,
    /// The first voice effects may take (0x800d24d2: ten past the cars'
    /// count, the voices below kept for the engines).
    first: usize,
    /// The engines, the hits and the tyres, with each effect voice's
    /// importance.
    engines: hwtr_game::engines::Engines,
    /// Each car's engine bank and its samples.
    banks: Vec<Option<(hwtr_game::snd::Bank, std::sync::Arc<[u8]>)>>,
    /// The crashes bank (libsnd's VAB 1) and the dialog bank (VAB 3), with
    /// their samples.
    crashes: Option<(hwtr_game::snd::Bank, std::sync::Arc<[u8]>)>,
    dialog: Option<(hwtr_game::snd::Bank, std::sync::Arc<[u8]>)>,
    /// What each voice was keyed with by the engines (libsnd's record of
    /// it, which a bend checks): the car, program, tone and note.
    owners: [Option<(usize, u8, u8, u8)>; 24],
}

impl RaceSound {
    /// 0x800157f8: effect `id` on a voice from 0x80019abc (the first free
    /// one from `first` to 19, else the first playing something less
    /// important, let go), remembering its importance.
    fn play(&mut self, id: u8, importance: u8) {
        let Ok(mut spu) = self.spu.lock() else { return };
        let Some((v, stolen)) = self.engines.voice(importance, &|v| spu.active(v)) else { return };
        if stolen {
            spu.key_off(v);
        }
        if let Some(voice) = self.effects.voice(id, self.volume) {
            spu.key_on(v, &self.effects.samples, &voice);
            self.engines.importance[v] = importance;
            self.owners[v] = None;
        }
    }

    /// Whether each voice plays.
    fn alive(&self) -> [bool; 24] {
        match self.spu.lock() {
            Ok(spu) => std::array::from_fn(|v| spu.active(v)),
            Err(_) => [false; 24],
        }
    }

    /// The engines' asks of the sound chip, as libsnd carries them out.
    fn apply(&mut self, changes: &[hwtr_game::engines::Change]) {
        use hwtr_game::engines::Change;
        let Ok(mut spu) = self.spu.lock() else { return };
        for &c in changes {
            match c {
                Change::KeyOn { voice, bank, program, tone, note, fine, left, right } => {
                    use hwtr_game::engines::Bank;
                    let (b, samples) = match bank {
                        Bank::Car(car) => match self.banks.get(car) {
                            Some(Some((b, s))) => (b, s),
                            _ => continue,
                        },
                        Bank::Effects => (&self.effects.bank, &self.effects.samples),
                        Bank::Crashes => match &self.crashes {
                            Some((b, s)) => (b, s),
                            None => continue,
                        },
                        Bank::Dialog => match &self.dialog {
                            Some((b, s)) => (b, s),
                            None => continue,
                        },
                    };
                    let keyed = hwtr_game::snd::key_on(
                        b,
                        &self.effects.notes,
                        program as usize,
                        tone as usize,
                        note as i32,
                        fine as i32,
                        left as i32,
                        right as i32,
                        self.effects.mono,
                        0,
                    );
                    if let (Some(v), Some(owner)) = (keyed, self.owners.get_mut(voice)) {
                        spu.key_on(voice, samples, &v);
                        *owner = match bank {
                            Bank::Car(car) => Some((car, program, tone, note)),
                            _ => None,
                        };
                    }
                }
                Change::KeyOff { voice } => spu.key_off(voice),
                Change::Bend { voice, program, bend } => {
                    let Some(Some((car, p, tone, note))) = self.owners.get(voice).copied() else { continue };
                    let Some(Some((bank, _))) = self.banks.get(car) else { continue };
                    if p != program {
                        continue;
                    }
                    let pitch = hwtr_game::snd::bend_pitch(
                        bank,
                        &self.effects.notes,
                        p as usize,
                        tone as usize,
                        note as i32,
                        bend,
                    );
                    if let Some(pitch) = pitch {
                        spu.set_pitch(voice, pitch);
                    }
                }
                Change::Volume { voice, left, right } => spu.set_volume(voice, [left as u16 * 129, right as u16 * 129]),
            }
        }
    }

    /// Every car's engine keyed (0x80015ebc for each, as 0x800350d4 and
    /// 0x80036758 do), or let go (0x80036634).
    fn engines_on(&mut self, on: bool) {
        let changes: Vec<_> = (0..self.engines.cars.len())
            .flat_map(|slot| if on { self.engines.key_on(slot) } else { self.engines.key_off(slot) })
            .collect();
        self.apply(&changes);
    }
}

impl Drop for RaceSound {
    /// The race over: its voices let go (0x80033aa0).
    fn drop(&mut self) {
        if let Ok(mut spu) = self.spu.lock() {
            for v in 0..24 {
                spu.key_off(v);
            }
        }
    }
}

impl RaceSound {
    /// The voices and banks for the race's engines: each car's from the
    /// executable's tables by name, its bank from the track's archive.
    fn load_engines(
        &mut self,
        byte: &dyn Fn(u32) -> u8,
        names: &[String],
        players: usize,
        get: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) {
        let kinds = hwtr_game::engines::kinds(byte);
        let engines: Vec<_> = names.iter().map(|n| hwtr_game::engines::car_engine(byte, &n.to_lowercase())).collect();
        let [tone, program, note] = self.effects.record(0);
        let effect = hwtr_game::engines::Effect { program: program as u8, tone: tone as u8, note: note as u8 };
        self.engines = hwtr_game::engines::Engines::new(kinds, &engines, players, effect);
        self.engines.first = self.first;
        self.engines.effects = (0..61)
            .map(|id| {
                let [tone, program, note] = self.effects.record(id);
                hwtr_game::engines::Effect { program: program as u8, tone: tone as u8, note: note as u8 }
            })
            .collect();
        self.engines.hits = hwtr_game::engines::HitTables::read(byte);
        self.banks = (0..names.len())
            .map(|slot| {
                engines[slot]?;
                let bank = self.engines.kind(slot)?.bank.to_uppercase();
                let vh = get(&format!("{bank}VH"))?;
                let vb = get(&format!("{bank}VB"))?;
                tracing::info!("car {slot}'s engine: {bank}");
                Some((hwtr_game::snd::Bank::from_vh(&vh, 0)?, std::sync::Arc::from(vb)))
            })
            .collect();
        self.engines_on(true);
    }

    /// 0x80015990: the pause menu's effects, always on voice 19.
    fn play_menu(&mut self, id: u8) {
        let Ok(mut spu) = self.spu.lock() else { return };
        if let Some(voice) = self.effects.voice(id, self.volume) {
            spu.key_on(19, &self.effects.samples, &voice);
        }
    }
}

/// The pad as the original's controller read takes it: a DualShock in
/// analog mode (when asked for, and a gamepad's sticks are there) or a
/// digital pad, as a DualShock is when switched on.
fn pad_state(pad: &Pad, analog: bool) -> PadState {
    PadState {
        kind: if analog && pad.analog { PadKind::Analog } else { PadKind::Digital },
        buttons: pad.buttons.bits(),
        left: [pad.left.x, pad.left.y],
        right: [pad.right.x, pad.right.y],
    }
}

impl Race {
    /// A quick race on `track` ("DESERT1"): the player first on the grid
    /// in the Deora, five computer cars behind.
    pub fn load(
        cue: &Path,
        track: &str,
        spu: Option<std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>>,
    ) -> Result<Race, String> {
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let exe = iso.find("CCCPSX.EXE").and_then(|e| iso.read(&e)).map_err(|e| e.to_string())?;
        let exe = hwtr_psx::Exe::parse(&exe).map_err(|e| e.to_string())?;
        let tables = Tables::from_exe(&exe);
        let t = track.to_uppercase();
        let (letters, number) = t.split_at(t.len() - 1);
        let world = hwtr_game::race::WORLDS
            .iter()
            .find(|w| w.eq_ignore_ascii_case(letters))
            .map_or(letters.to_string(), |w| w.to_string());
        let track_number = number.parse().unwrap_or(1);
        let setup = RaceSetup {
            flags: 0,
            checkpoints: tables.checkpoints(&world, track_number),
            track: world,
            track_number,
            // The grid is behind the line: crossing it the first time is
            // the first of four, as the front end sets it.
            laps: 4,
            options: 0,
            time_limit: 0,
            cars: CARS
                .iter()
                .enumerate()
                .map(|(k, name)| Entrant {
                    name: name.to_string(),
                    driver: if k == 0 { Driver::PlayerOne } else { Driver::Computer },
                    car_id: k as u8,
                    player: 0,
                    grid: k as u8,
                })
                .collect(),
            difficulty: 128,
            best_line: None,
            names: ["Player 1".to_string(), "Player 2".to_string()],
        };
        Race::load_setup(cue, setup, spu, 200)
    }

    /// The race `setup` describes, as the front end sets it up, with sound
    /// through `spu` at the settings' effects volume `volume` (0 to 255).
    pub fn load_setup(
        cue: &Path,
        setup: RaceSetup,
        spu: Option<std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>>,
        volume: u8,
    ) -> Result<Race, String> {
        let track = format!("{}{}", setup.track.to_uppercase(), setup.track_number);
        let cars: Vec<String> = setup.cars.iter().map(|c| c.name.clone()).collect();
        let mut scene = Scene::load(cue, &track, Layout::Normal, &cars)?;
        if scene.cars.is_empty() {
            return Err(format!("{track} has no start grid"));
        }
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let read = |name: &str| iso.find(name).and_then(|e| iso.read(&e)).map_err(|e| e.to_string());
        let exe = hwtr_psx::Exe::parse(&read("CCCPSX.EXE")?).map_err(|e| e.to_string())?;
        let big = read("CCCPSX.BIG")?;
        let big = hwtr_data::Big::parse(&big).map_err(|e| e.to_string())?;
        let t = track.clone();
        let get = |name: &str| big.lookup(&format!("{t}BIG/{name}")).ok_or(format!("{name} is not in {t}.BIG"));
        let scp = Scp::parse(get(&format!("{t}SCP"))?).ok_or("the SCP does not parse")?;
        let tuning = Tuning::from_prm(get("TUNINGPRM")?);
        let mut parts = Vec::new();
        for name in &cars {
            let bmf = get(&format!("{}BMF", name.to_uppercase()))?;
            let cwh = hwtr_data::car::CarBmf::parse(bmf).map_err(|e| e.to_string())?.cwh;
            parts.push(handling::parse_cwh(cwh).ok_or(format!("{name}'s CWH does not parse"))?);
        }
        let tables = Tables::from_exe(&exe);
        let mut fonts = Vec::new();
        for (k, name) in hwtr_game::hud::FONTS.iter().enumerate() {
            let font = Font::parse(get(&format!("{name}OVL"))?).ok_or(format!("{name}.OVL does not parse"))?;
            let tim = hwtr_data::Tim::parse(&font.tim).map_err(|e| e.to_string())?;
            let ((x, y), (cx, cy)) = (hwtr_game::hud::FONT_IMAGE[k], hwtr_game::hud::FONT_CLUT[k]);
            scene.vram.load(x, y, tim.rect.w, tim.rect.h, &tim.data);
            if let Some((r, colours)) = &tim.clut {
                scene.vram.load(cx, cy, r.w, 1, &colours[..r.w as usize]);
            }
            let depth = match tim.mode {
                hwtr_data::tim::Mode::Bpp4 => 0,
                hwtr_data::tim::Mode::Bpp8 => 1,
                _ => 2,
            };
            let mode =
                ((cy as u32) << 6 | (cx as u32) >> 4) | (((x / 64) | ((y / 256) << 4) | (depth << 7)) as u32) << 16;
            fonts.push((font, mode));
        }
        let line_name = setup.best_line.as_ref().map_or(t.clone(), |n| n.to_uppercase());
        let line =
            hwtr_game::line::BestLine::parse(get(&format!("{line_name}BLD"))?).ok_or("the best line does not parse")?;
        // The world's place in the table of names at 0x800c5c24, and the
        // track's number in it.
        let world_number = (setup.track_number as u32).max(1);
        let world_index = {
            let view = exe.view();
            let word = |a: u32| u32::from_le_bytes(std::array::from_fn(|k| view.u8(a + k as u32).unwrap_or(0)));
            let name = |a: u32| {
                (0..16).map(|i| view.u8(a + i).unwrap_or(0)).take_while(|&c| c != 0).map(char::from).collect::<String>()
            };
            (0..4u32).find(|&k| name(word(0x800c_5c24 + 4 * k)).eq_ignore_ascii_case(&setup.track)).unwrap_or(0)
        };
        let mut race = hwtr_game::race::Race::new(setup, scp, line, &parts, tables, tuning);
        // 0x80067418: the pickups, each power-up they name loaded once
        // (`<name>.PUP`), and the track's two cars to unlock (the tables at
        // 0x800c5cd0 and 0x800c5cdc, by world and number).
        {
            let world = &scene.world;
            let mut defs: Vec<hwtr_game::powerup::PowerUp> = Vec::new();
            let mut spots = Vec::new();
            for p in &world.pickups {
                let lower = p.name.to_ascii_lowercase();
                if lower != "random" && !defs.iter().any(|d| d.name.eq_ignore_ascii_case(&lower)) {
                    match get(&format!("{}PUP", p.name.to_uppercase()))
                        .ok()
                        .and_then(hwtr_game::powerup::PowerUp::parse)
                    {
                        Some(def) => defs.push(def),
                        None => rrt::tracing::warn!("PwrupLoadPowerUp : error loading file {lower}.pup"),
                    }
                }
                spots.push((p.name.clone(), p.pos, p.unknown_0c as i32, p.object));
            }
            let view = exe.view();
            let at = |base: u32| view.u8(base + world_index * 3 + world_number - 1).unwrap_or(0);
            race.set_power_ups(&spots, defs, [at(0x800c_5cd0), at(0x800c_5cdc)]);
            // 0x8007eff4: the moving objects.
            let anims = world
                .anims
                .iter()
                .filter_map(|a| {
                    Some(hwtr_game::world_anim::ObjectAnim {
                        object: a.object?,
                        period: a.period,
                        keys: a.keys.iter().map(|k| (k.pos, k.quat)).collect(),
                        time: 0,
                    })
                })
                .collect();
            race.anims = hwtr_game::world_anim::WorldAnims::new(anims);
            // 0x8006b2a8: the collision volumes; one that follows its object
            // takes the object's place (20.12) and rotation.
            let volumes: Vec<_> = world
                .volumes
                .iter()
                .map(|v| {
                    let (centre, rot) = match v.object.filter(|_| v.flags & 8 != 0).and_then(|o| world.objects.get(o)) {
                        Some(o) => (o.pos.map(|c| c << 12), o.rot),
                        None => (v.pos, v.rot),
                    };
                    (v.flags, v.object, centre, rot, v.size, v.heft, v.extra[0] as u8)
                })
                .collect();
            race.set_volumes(&volumes);
            // 0x8002e27c: a knocked prop's object's quads fly off.
            let quads = world
                .volumes
                .iter()
                .map(|v| {
                    v.object.and_then(|o| world.objects.get(o)).map_or(Vec::new(), |o| {
                        o.mesh
                            .iter()
                            .map(|q| hwtr_game::effects::ChunkFace {
                                verts: q.v.map(|p| [p.x, p.y, p.z]),
                                uv: q.uv,
                                clut: q.clut,
                                tpage: q.tpage,
                            })
                            .collect()
                    })
                })
                .collect();
            race.set_volume_quads(quads);
            // The cars' models' root faces, which a wreck throws off
            // (0x8002e574: corners, the texels at +10, +14, +8, +12).
            race.car_faces = scene
                .cars
                .iter()
                .map(|c| {
                    let root = &c.model.root;
                    root.faces
                        .iter()
                        .map(|f| hwtr_game::effects::ChunkFace {
                            verts: f.v.map(|i| root.verts.get(i as usize).map_or([0; 3], |v| [v.x, v.y, v.z])),
                            uv: [f.uv[0], f.uv[1], f.uv[3], f.uv[2]],
                            clut: c.clut,
                            tpage: c.tpage,
                        })
                        .collect()
                })
                .collect();
            for (car, faces) in race.cars.iter_mut().zip(&race.car_faces) {
                car.model_faces = faces.len().min(0xffff) as u16;
            }
            // 0x80021390: the trackside cameras.
            race.camera_spots = world
                .cameras
                .iter()
                .map(|c| hwtr_game::camera::Spot { flags: c.flags, fov: c.fov as i32, rot: c.rot, pos: c.pos })
                .collect();
        }
        // The pause menu: the string table and the front end's kerning (its
        // font loaded once), and the race's text font.
        let screens =
            |name: &str| big.lookup(&format!("SCREENSBIG/{name}")).ok_or(format!("{name} is not in SCREENS.BIG"));
        let strings =
            hwtr_game::front::strings::Strings::parse(screens("ENGLISHHWT")?).ok_or("ENGLISH.HWT does not parse")?;
        let screen_font = Font::parse(screens("SCRNFNTOVL")?).ok_or("SCRNFNT.OVL does not parse")?;
        let view = exe.view();
        let byte = |a: u32| view.u8(a).unwrap_or(0);
        let mut kerning = hwtr_game::front::font::ScreenFont::new(&screen_font, byte);
        kerning.prepare();
        race.pause_kit = hwtr_game::pause::PauseKit::new(&byte, &strings, &fonts[0].0, kerning);
        // The results' heads and the cars' names (0x800c5d8c).
        let word = |a: u32| u32::from_le_bytes(std::array::from_fn(|k| byte(a + k as u32)));
        race.results_text = hwtr_game::hud::ResultsText {
            time: strings.get(289).to_string(),
            best: strings.get(290).to_string(),
            points: strings.get(291).to_string(),
            no_time: strings.get(215).to_string(),
            demo: strings.get(214).to_string(),
            wrong_way: strings.get(216).to_string(),
            strings: (0..hwtr_game::front::strings::STRING_COUNT).map(|k| strings.get(k).to_string()).collect(),
            stunt_words: hwtr_game::hud::StuntWords {
                points: strings.get(217).to_string(),
                turbo: strings.get(292).to_string(),
                turbos: strings.get(293).to_string(),
            },
            cars: (0..42u32)
                .map(|k| {
                    let at = word(0x800c_5d8c + 4 * k);
                    (0..24).map(|i| byte(at + i)).take_while(|&b| b != 0).map(char::from).collect()
                })
                .collect(),
        };
        if let Some(kit) = &race.pause_kit {
            let missing = kit.unported_actions();
            if !missing.is_empty() {
                rrt::tracing::warn!("pause menu: {} actions not ported: {missing:x?}", missing.len());
            }
        }
        // 0x8001924c: the effects bank; 0x8001a6dc: the volume in 127ths,
        // then 0x800157f8 plays at three eighths of it.
        let cars = race.cars.len();
        let cars_names: Vec<String> = race.setup.cars.iter().map(|c| c.name.clone()).collect();
        let sound = match (spu, get("MAINSFX2VH"), get("MAINSFX2VB")) {
            (Some(spu), Ok(vh), Ok(vb)) => hwtr_game::snd::Bank::from_vh(vh, 0).map(|bank| RaceSound {
                spu,
                effects: crate::spu::Effects::new(&byte, bank, vb),
                volume: (volume as i32 * 127 / 255) * 3 / 8,
                effects_volume: volume as i32 * 127 / 255,
                first: cars + 10,
                engines: Default::default(),
                banks: Vec::new(),
                crashes: None,
                dialog: None,
                owners: [None; 24],
            }),
            _ => None,
        };
        // 0x8001924c and 0x800350d4: the engines' banks, and each keyed.
        let mut sound = sound;
        if let Some(s) = &mut sound {
            let players = race.setup.cars.iter().filter(|e| e.driver.is_player()).count();
            s.load_engines(&byte, &cars_names, players, &|name| get(name).ok().map(<[u8]>::to_vec));
            // 0x8001924c: the crashes bank the race drew, as VAB 1.
            let n = race.crash_bank;
            if let (Ok(vh), Ok(vb)) = (get(&format!("CRASHES{n}VH")), get(&format!("CRASHES{n}VB"))) {
                s.crashes = hwtr_game::snd::Bank::from_vh(vh, 0).map(|b| (b, std::sync::Arc::from(vb)));
                tracing::info!("crashes bank {n}");
            }
            // ... and the dialog bank, as VAB 3.
            let n = race.dialog_bank;
            if let (Ok(vh), Ok(vb)) = (get(&format!("DIALOG{n}VH")), get(&format!("DIALOG{n}VB"))) {
                s.dialog = hwtr_game::snd::Bank::from_vh(vh, 0).map(|b| (b, std::sync::Arc::from(vb)));
                tracing::info!("dialog bank {n}");
            }
        }
        tracing::info!(
            "race on {t}: {} car(s) ported, flyby of {} keyframes, countdown from {} ms",
            race.cars.len(),
            race.collision.scp.flyby.len(),
            race.countdown_from
        );
        Ok(Race {
            scene,
            race,
            renderer: None,
            ahead: 0,
            since_read: 0,
            carry_us: 0,
            before: None,
            effects: Vec::new(),
            between: 1.0,
            count: None,
            count_ms: 0,
            reader: PadReader::default(),
            mapping: Mapping::default(),
            analog: false,
            fonts,
            sound,
        })
    }

    /// One display frame of `elapsed`, as race_frame (0x80033ed8) runs it:
    /// race steps of 25 ms until the race's clock passes the real one (a
    /// frame counting for at most 50 ms), each reading the pad first (the
    /// time since the last read going to the first) and driving the
    /// player's car with it.
    pub fn frame(&mut self, pad: &Pad, elapsed: Duration) {
        let us = (elapsed.as_micros().min(u32::MAX as u128) as u32).saturating_add(self.carry_us);
        let ms = us / 1000;
        self.carry_us = us % 1000;
        self.since_read = self.since_read.saturating_add(ms);
        self.ahead -= ms.min(50) as i64;
        let state = pad_state(pad, self.analog);
        if self.race.paused.is_some() {
            // Stopped: no steps, and none owed when it goes on (0x800346fc
            // sets the race's clock to now).
            self.ahead = 0;
            let elapsed = std::mem::take(&mut self.since_read);
            self.race.motors[0].fade(self.mapping.vibration(), elapsed, false, self.race.clock);
            self.race.pause_tick([state, PadState::default()]);
            if let Some(level) = self.race.effects_asked.take()
                && let Some(s) = &mut self.sound
            {
                // 0x8001a6dc: in 127ths, the effects at three eighths.
                s.effects_volume = level as i32 * 127 / 255;
                s.volume = s.effects_volume * 3 / 8;
            }
        }
        while self.ahead < 0 && self.race.paused.is_none() {
            self.before = Some(self.shown());
            let elapsed = std::mem::take(&mut self.since_read);
            self.reader.read(&state, &self.mapping, elapsed);
            let running = self.race.phase == hwtr_game::race::Phase::Racing;
            self.race.motors[0].fade(self.mapping.vibration(), elapsed, running, self.race.clock);
            // The attract race has no players: the pad drives nothing.
            if self.race.demo() {
                self.race.step(&[]);
            } else {
                self.race.step(&[self.reader.controls()]);
            }
            self.ahead += STEP_MS as i64;
        }
        self.between = (1.0 - self.ahead as f32 / STEP_MS as f32).clamp(0.0, 1.0);
        let buttons = Buttons { accept: state.holds(&self.mapping, ACCEPT), start: state.holds(&self.mapping, START) };
        // 0x800354ac, after the steps: the commentator.
        self.race.sound_frame(ms);
        self.race.frame(buttons);
        self.effects = self.race.effects_frame(ms);
        self.count_ms += ms;
        while self.count_ms >= 33 {
            self.count_ms -= 33;
            self.count = self.race.hud.countdown.frame(self.race.hud.players);
        }
        for event in std::mem::take(&mut self.race.events) {
            use hwtr_game::race::RaceEvent;
            if let RaceEvent::Hit(hit) = event {
                tracing::debug!("{hit:?} at {} ms", self.race.time);
                if let Some(s) = &mut self.sound {
                    let alive = s.alive();
                    let changes = s.engines.hit(&self.race.tables, &hit, &|v| alive.get(v).copied().unwrap_or(false));
                    s.apply(&changes);
                }
                continue;
            }
            if let RaceEvent::Dialog { tone } = event {
                tracing::info!("the commentator's tone {tone} at {} ms", self.race.time);
                if let Some(s) = &mut self.sound {
                    let alive = s.alive();
                    let changes = s.engines.dialog(tone, s.volume as u8, &|v| alive.get(v).copied().unwrap_or(false));
                    s.apply(&changes);
                }
                continue;
            }
            tracing::info!("{event:?} at {} ms", self.race.time);
            // What each sounds (the calls to 0x800157f8): the countdown,
            // the start, a player's checkpoints, laps and wrong way, a
            // player's wreck, the pause.
            use hwtr_game::laps::LapEvent;
            let effect = match event {
                RaceEvent::Count(3) => Some((6, 1)),
                RaceEvent::Count(2) => Some((7, 1)),
                RaceEvent::Count(1) => Some((8, 1)),
                RaceEvent::Go => Some((9, 1)),
                RaceEvent::Lap { event: LapEvent::Checkpoint, .. } => Some((10, 0)),
                RaceEvent::Lap { event: LapEvent::WrongWay, .. } => Some((11, 0)),
                RaceEvent::Lap { event: LapEvent::Lap { .. }, .. } => Some((12, 0)),
                RaceEvent::Pause => Some((14, 0)),
                RaceEvent::Wreck { .. } => Some((29, 1)),
                RaceEvent::PowerUp { .. } => Some((24, 0)),
                RaceEvent::Knock { sound, .. } => Some((sound, 0)),
                RaceEvent::Effect { id, importance } => Some((id, importance)),
                _ => None,
            };
            if let (Some((id, importance)), Some(s)) = (effect, &mut self.sound) {
                s.play(id, importance);
            }
            if let Some(s) = &mut self.sound {
                match event {
                    RaceEvent::Wrecked { car } => {
                        let changes = s.engines.key_off(car as usize);
                        s.apply(&changes);
                    }
                    // 0x80041384: a player's car back on the road sounds 13.
                    RaceEvent::Reset { car } => {
                        if self.race.cars.get(car as usize).is_some_and(|c| c.flags & 1 != 0) {
                            s.play(13, 0);
                        }
                        let changes = s.engines.key_on(car as usize);
                        s.apply(&changes);
                    }
                    RaceEvent::Pause => s.engines_on(false),
                    RaceEvent::Resume => s.engines_on(true),
                    _ => {}
                }
            }
        }
        // 0x800354ac: the engines, once a frame after the steps.
        if let Some(s) = &mut self.sound {
            let inputs: Vec<_> = self
                .race
                .cars
                .iter()
                .enumerate()
                .map(|(slot, car)| {
                    hwtr_game::engines::input_of(
                        car,
                        if slot == 0 { self.reader.eased } else { 0 },
                        &s.engines.hits.priority,
                    )
                })
                .collect();
            let listener = self.race.cameras.first().map(hwtr_game::engines::Listener::of).unwrap_or_default();
            let alive = s.alive();
            let changes = s.engines.frame(&self.race.tables, &inputs, &listener, s.effects_volume, ms, &|v| {
                alive.get(v).copied().unwrap_or(false)
            });
            s.apply(&changes);
        }
        if let (Some(p), Some(s)) = (&self.race.paused, &mut self.sound) {
            for &id in &p.sounds {
                s.play_menu(id);
            }
        }
        // The vertical blank at the frame's end advances the system clock.
        self.race.clock = self.race.clock.wrapping_add(ms);
    }

    /// Car `slot` where the game draws it (0x80049ecc): its body's
    /// position and centre less its turned wheel origin, in world units; its
    /// rotation's columns its axes.
    fn pose(&self, slot: usize) -> (Vec3, Mat3) {
        let car = &self.race.cars[slot];
        let body = &car.body;
        let turned = body
            .rot
            .map(|row| (0..3).fold(0i32, |s, k| s.wrapping_add(hwtr_game::math::fx(row[k] as i32, car.origin[k]))));
        let at = hwtr_game::math::sub(hwtr_game::math::add(body.pos, body.centre), turned);
        (world(at), axes(&body.rot))
    }

    /// The race as its last step left it.
    fn shown(&self) -> Shown {
        let cars = (0..self.race.cars.len()).map(|k| {
            let (pos, rot) = self.pose(k);
            (pos, Quat::from_mat3(&rot).normalize())
        });
        let (eye, look, fov) = match self.race.cameras.first() {
            Some(c) => (world(c.pos), Quat::from_mat3(&axes(&c.rot)).normalize(), c.fov as f32 / 4096.0),
            None => (Vec3::ZERO, Quat::IDENTITY, 1.0),
        };
        Shown { cars: cars.collect(), eye, look, fov }
    }

    /// What to draw now: between the step before and the last.
    fn drawn(&self) -> Shown {
        let now = self.shown();
        match &self.before {
            // Standing still, the results' snapshots cut from one to the next.
            _ if self.race.frozen => now,
            Some(before) if before.cars.len() == now.cars.len() => before.toward(&now, self.between),
            _ => now,
        }
    }

    fn car_triangles(&self, shown: &Shown) -> Vec<hwtr_render::Vtx> {
        let mut tris = Vec::new();
        for (k, (look, &(pos, rot))) in self.scene.cars.iter().zip(&shown.cars).enumerate() {
            // 0x80068540: not the car the camera rides, nor during a
            // reset's blink.
            if !self.race.car_shown(k) {
                continue;
            }
            // A wrecked car's model is blackened (0x8002e51c).
            let rgb = self.race.effects.root_colour.get(k).copied().unwrap_or(0x80_8080);
            // Each wheel steered, turned and lifted as last posed
            // (0x80020a14): at twice its record's mount, raised by twice
            // its lift.
            let wheels: Vec<(Mat3, Vec3)> = self
                .race
                .wheel_poses
                .get(k)
                .and_then(|p| p.as_ref())
                .map(|poses| {
                    poses
                        .iter()
                        .zip(&look.model.wheels)
                        .map(|(p, rec)| {
                            let col = |j: usize| Vec3::from_array([0, 1, 2].map(|i| p.rot[i][j] as f32 / 4096.0));
                            let at = Vec3::new(
                                2.0 * rec[0] as f32,
                                2.0 * rec[1] as f32,
                                2.0 * (rec[2] as f32 + p.lift as f32 / 4096.0),
                            );
                            (Mat3::from_cols(col(0), col(1), col(2)), at)
                        })
                        .collect()
                })
                .unwrap_or_default();
            tris.extend(hwtr_render::mesh::car_triangles(
                &look.model,
                look.clut,
                look.tpage,
                pos,
                Mat3::from_quat(rot),
                rgb,
                &wheels,
            ));
        }
        tris
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) -> wgpu::CommandBuffer {
        if self.renderer.as_ref().is_none_or(|(_, f)| *f != format) {
            self.renderer = Some((self.scene.renderer(device, queue, format), format));
        }
        let shown = self.drawn();
        let mut tris = self.car_triangles(&shown);
        tris.extend(self.apart_triangles());
        let semi = self.effect_triangles();
        tris.extend(semi.iter().copied());
        let hud = match &self.race.paused {
            Some(p) => p.sprites.clone(),
            None => self.race.hud(0),
        };
        let mut overlay = self.overlay(&hud);
        overlay.extend(self.count.map(|c| self.count_quad(&c)).unwrap_or_default());
        let mvp = camera_matrix(shown.eye, Mat3::from_quat(shown.look), shown.fov, size.0 as f32 / size.1 as f32);
        let (renderer, _) = self.renderer.as_mut().unwrap();
        renderer.set_moving(device, queue, &tris);
        let semi: Vec<_> = semi.into_iter().filter(|v| v.mode >> 31 == 1).collect();
        renderer.set_semi(device, queue, &semi);
        renderer.set_overlay(device, queue, &overlay);
        renderer.draw(device, queue, target, size, mvp)
    }

    /// The objects drawn apart from the track: each moving object at its
    /// pose now (0x8007f2a4), the rest where they stand; a pickup only while
    /// it is out (0x800216c4 sets its object's visible bit as it goes and
    /// comes back).
    fn apart_triangles(&mut self) -> Vec<hwtr_render::Vtx> {
        let mut out = Vec::new();
        for &(object, parent) in &self.scene.apart {
            if self.race.power_ups.pickups.iter().any(|p| p.object == Some(object) && !p.out)
                || self.race.knocked.contains(&object)
            {
                continue;
            }
            let anim = self.race.anims.anims.iter().position(|a| a.object == object);
            let pose = anim.and_then(|k| self.race.anims.pose(&self.race.tables, k));
            let (rot, pos) = match pose {
                Some((rot, pos)) => (rot, Vec3::new(pos[0] as f32, pos[1] as f32, pos[2] as f32) / 4096.0),
                None => {
                    let o = &self.scene.world.objects[object];
                    (o.rot, Vec3::new(o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32))
                }
            };
            out.extend(hwtr_render::mesh::object_posed(&self.scene.world, object, parent, rot, pos));
        }
        out
    }

    /// The effects' quads, both faces; a semi-transparent one's mode has
    /// bit 31.
    fn effect_triangles(&self) -> Vec<hwtr_render::Vtx> {
        let mut out = Vec::new();
        for q in &self.effects {
            let mode = q.clut as u32 | (q.tpage as u32) << 16 | (q.semi as u32) << 31;
            let colour = q.colour[0] as u32 | (q.colour[1] as u32) << 8 | (q.colour[2] as u32) << 16;
            let us = q.uv.map(|t| t[0]);
            let vs = q.uv.map(|t| t[1]);
            let window = hwtr_render::Vtx::window_of(us, vs);
            let corner = |k: usize| hwtr_render::Vtx {
                pos: world(q.corners[k]).to_array(),
                colour,
                uv: q.uv[k][0] as u32 | (q.uv[k][1] as u32) << 8,
                mode,
                window,
            };
            out.extend([corner(0), corner(1), corner(3), corner(1), corner(2), corner(3)]);
            out.extend([corner(0), corner(3), corner(1), corner(1), corner(3), corner(2)]);
        }
        out
    }

    /// The HUD's glyphs as triangles on the PlayStation's screen.
    fn overlay(&self, sprites: &[Sprite]) -> Vec<hwtr_render::Vtx> {
        let mut out = Vec::new();
        for s in &hwtr_game::hud::drawing_order(sprites) {
            let Some((font, mode)) = self.fonts.get(s.font as usize) else { continue };
            let Some(g) = font.glyphs.get(s.glyph as usize) else { continue };
            let (x, y, w, h) = (s.x as f32, s.y as f32, g.w as f32, g.h as f32);
            let colour = s.colour[0] as u32 | (s.colour[1] as u32) << 8 | (s.colour[2] as u32) << 16;
            let corner = |k: usize, px: f32, py: f32| hwtr_render::Vtx {
                pos: [px, py, 0.0],
                colour,
                uv: g.u[k] as u32 | (g.v[k] as u32) << 8,
                mode: *mode,
                window: hwtr_render::Vtx::window_of(g.u, g.v),
            };
            let c = [corner(0, x, y), corner(1, x + w, y), corner(2, x, y + h), corner(3, x + w, y + h)];
            out.extend([c[0], c[1], c[2], c[1], c[3], c[2]]);
        }
        out
    }

    /// The countdown's number over everything (ordering-table slot 1).
    fn count_quad(&self, c: &hwtr_game::hud::CountSprite) -> Vec<hwtr_render::Vtx> {
        let (clut, page) = self.race.tables.sprite_slots[hwtr_game::hud::COUNT_SLOT + c.number as usize % 4];
        let mode = clut as u32 | (page as u32) << 16;
        let (x, y, s) = (c.x as f32, c.y as f32, c.size as f32);
        let (u, v) = (c.u, c.v);
        let us = [u, u + 63, u, u + 63];
        let vs = [v, v, v + 63, v + 63];
        let corner = |k: usize, px: f32, py: f32| hwtr_render::Vtx {
            pos: [px, py, 0.0],
            colour: 0x80_80_80,
            uv: us[k] as u32 | (vs[k] as u32) << 8,
            mode,
            window: hwtr_render::Vtx::window_of(us, vs),
        };
        let q = [corner(0, x, y), corner(1, x + s, y), corner(2, x, y + s), corner(3, x + s, y + s)];
        vec![q[0], q[1], q[2], q[1], q[3], q[2]]
    }

    /// Player one's pad's motors as the game last set them.
    pub fn motors(&self) -> rrt::input::Motors {
        let m = self.race.motors[0];
        rrt::input::Motors { small: m.small, large: m.large }
    }

    /// The race is over: its results are through.
    pub fn over(&self) -> bool {
        self.race.over
    }

    /// The attract race, which the front end watches as it runs.
    pub fn demo(&self) -> bool {
        self.race.demo()
    }

    /// How it ended, for the front end.
    /// What the race leaves the front end (0x8003379c's record): how it
    /// ended, and each car's time, best lap, laps, points and stunt score,
    /// by slot, and the cars the unlock pickups gave.
    pub fn result(&self) -> hwtr_game::front::RaceResult {
        let cars = self
            .race
            .cars
            .iter()
            .map(|c| {
                let s = self.race.standings.iter().find(|s| s.car == c.slot);
                hwtr_game::front::CarResult {
                    time: s.map_or(0, |s| s.time),
                    best: s.map_or(c.laps.best, |s| s.best),
                    laps: c.laps.done,
                    points: s.map_or(0, |s| s.points),
                    score: c.stunt_points.max(0) as u32,
                }
            })
            .collect();
        // 0x80067708: the cars the unlock pickups gave, each player's into
        // their words (cars 0 to 31, then 32 on).
        let mut unlocks = [0u32; 6];
        for (k, cars) in self.race.power_ups.unlocked.iter().enumerate() {
            for car in cars.iter().flatten() {
                let word = 3 * k + (*car as usize >= 32) as usize;
                unlocks[word] |= 1u32 << (*car % 32);
            }
        }
        hwtr_game::front::RaceResult { end: self.end(), cars, unlocks }
    }

    /// Shots only: the result as if the race had been run in full, the
    /// cars placed in slot order (the player first), each with every lap.
    pub fn run_in_full(&self) -> hwtr_game::front::RaceResult {
        let mut result = self.result();
        result.end = hwtr_game::front::RaceEnd::Finished;
        for (k, c) in result.cars.iter_mut().enumerate() {
            c.laps = self.race.setup.laps;
            c.points = [10, 8, 7, 6, 5, 4][k.min(5)];
            c.time = 60_000 + 1000 * k as u32;
            c.best = 15_000 + 100 * k as u32;
        }
        result
    }

    pub fn end(&self) -> hwtr_game::front::RaceEnd {
        match self.race.end {
            1 => hwtr_game::front::RaceEnd::Finished,
            2 => hwtr_game::front::RaceEnd::Restart,
            n => hwtr_game::front::RaceEnd::Other(n),
        }
    }

    /// The settings' volumes, the effects' as the race was loaded with.
    pub fn set_volumes(&mut self, volumes: hwtr_game::pause::Volumes) {
        self.race.volumes = volumes;
        self.race.effects_level = volumes.effects;
    }

    /// The volumes as the pause menu left them.
    pub fn volumes(&self) -> hwtr_game::pause::Volumes {
        self.race.volumes
    }

    /// Player one's buttons, as the front end has them.
    pub fn set_mapping(&mut self, mapping: hwtr_game::pad::Mapping) {
        self.mapping = mapping;
    }

    /// The Audio Mode: mono (libsnd's switch, for every voice keyed).
    pub fn set_mono(&mut self, mono: bool) {
        if let Some(s) = &mut self.sound {
            s.effects.mono = mono;
        }
    }

    /// The CD's song as the race begins (the pause menu's Boom Box starts
    /// from it).
    pub fn set_song(&mut self, song: u8) {
        self.race.song = song;
    }

    /// What the race asked of the CD since the last call.
    pub fn take_cd(&mut self) -> Vec<hwtr_game::cd::CdAsk> {
        std::mem::take(&mut self.race.cd)
    }

    /// Where the player's car is drawn, in world units.
    pub fn position(&self) -> [f32; 3] {
        self.pose(0).0.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rrt::audio::Source;

    /// A race's engines sound from the grid, louder and higher once the
    /// player's car is driven; the sound is written out to listen to.
    #[test]
    fn the_engines_sound() {
        let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work");
        let Ok(cue) = rrt::disc::Image::find(&work.join("disc")) else {
            eprintln!("skipped: no disc under work/disc");
            return;
        };
        let spu = std::sync::Arc::new(std::sync::Mutex::new(crate::spu::Spu::default()));
        let mut race = Race::load(&cue, "desert1", Some(spu.clone())).expect("the race loads");
        let mut out = Vec::new();
        let mut loudness = [0u64; 2];
        let mut pitches = Vec::new();
        for frame in 0..2400 {
            let pad = if frame < 1200 {
                Pad::default()
            } else {
                Pad { buttons: rrt::input::Buttons::CROSS, ..Pad::default() }
            };
            race.frame(&pad, Duration::from_micros(16_683));
            let mut chunk = vec![0i16; 2 * 735];
            let mut s = spu.lock().unwrap();
            s.render(&mut chunk);
            pitches.push(s.pitch(0));
            loudness[frame / 1200] += chunk.iter().map(|v| v.unsigned_abs() as u64).sum::<u64>();
            out.extend(chunk);
        }
        let tmp = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp");
        std::fs::create_dir_all(&tmp).unwrap();
        let mono: Vec<i16> = out.chunks(2).map(|f| ((f[0] as i32 + f[1] as i32) / 2) as i16).collect();
        std::fs::write(tmp.join("engines.wav"), hwtr_data::vab::wav(&mono, 44100)).unwrap();
        eprintln!(
            "loudness idle {} driven {}; player pitch {:?}",
            loudness[0],
            loudness[1],
            &pitches[..].iter().step_by(50).collect::<Vec<_>>()
        );
        assert!(loudness[0] > 1_000_000, "silent on the grid");
        let (low, high) = (pitches.iter().min().unwrap(), pitches.iter().max().unwrap());
        assert!(high > low, "the player's engine never changed pitch");
    }
}
