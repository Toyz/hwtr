//! The race, as far as it is ported: the track and cars from the disc, the
//! player's car driven by the ported game logic, a camera behind it.

use std::path::Path;
use std::time::Duration;

use rrt::glam::{self, Mat3, Mat4, Vec3};
use rrt::input::Pad;
use rrt::wgpu;
use hwtr_game::car::{Tuning, handling};
use hwtr_game::collision::Scp;
use hwtr_game::hud::{Font, Sprite};
use hwtr_game::math::Tables;
use hwtr_game::pad::{ACCEPT, Mapping, PadKind, PadReader, PadState, START};
use hwtr_game::race::{Buttons, Driver, Entrant, RaceSetup, STEP_MS};
use hwtr_render::scene::Layout;
use hwtr_render::{Renderer, Scene};

/// The cars on the grid, the player's first.
const CARS: [&str; 6] = ["deora", "twinmill", "rocket", "bisector", "snake", "hw500"];

/// The view-projection from a game camera: at its position, looking along
/// its forward axis with its up axis up; its field of view across the
/// screen (the game's near plane, 10 inches, and a far one past the track).
fn camera_matrix(camera: &hwtr_game::camera::Camera, aspect: f32) -> Mat4 {
    let col = |j: usize| Vec3::from_array(camera.rot.map(|row| row[j] as f32 / 4096.0));
    let eye = Vec3::from_array(camera.pos.map(|c| c as f32 / 4096.0));
    let across = camera.fov as f32 / 4096.0;
    let tall = 2.0 * ((across / 2.0).tan() / aspect).atan();
    let view = glam::camera::rh::view::look_to_mat4(eye, col(1), col(2));
    rrt::gpu::projection(tall, aspect, 10.0, 300_000.0) * view
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
    /// Player one's controller, read once a frame as the game reads it.
    reader: PadReader,
    mapping: Mapping,
    /// The pad in analog mode (the DualShock's ANALOG light on).
    pub analog: bool,
    /// The HUD's fonts, each with the texture page and palette its glyphs
    /// draw with (as `Vtx::mode`).
    fonts: Vec<(Font, u32)>,
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
    pub fn load(cue: &Path, track: &str) -> Result<Race, String> {
        let cars: Vec<String> = CARS.iter().map(|c| c.to_string()).collect();
        let mut scene = Scene::load(cue, track, Layout::Normal, &cars)?;
        if scene.cars.is_empty() {
            return Err(format!("{track} has no start grid"));
        }
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let read = |name: &str| iso.find(name).and_then(|e| iso.read(&e)).map_err(|e| e.to_string());
        let exe = hwtr_psx::Exe::parse(&read("CCCPSX.EXE")?).map_err(|e| e.to_string())?;
        let big = read("CCCPSX.BIG")?;
        let big = hwtr_data::Big::parse(&big).map_err(|e| e.to_string())?;
        let t = track.to_uppercase();
        let get = |name: &str| big.lookup(&format!("{t}BIG/{name}")).ok_or(format!("{name} is not in {t}.BIG"));
        let scp = Scp::parse(get(&format!("{t}SCP"))?).ok_or("the SCP does not parse")?;
        let tuning = Tuning::from_prm(get("TUNINGPRM")?);
        // The player on the first grid place, computer cars behind.
        let mut parts = Vec::new();
        for name in CARS {
            let bmf = get(&format!("{}BMF", name.to_uppercase()))?;
            let cwh = hwtr_data::car::CarBmf::parse(bmf).map_err(|e| e.to_string())?.cwh;
            parts.push(handling::parse_cwh(cwh).ok_or(format!("{name}'s CWH does not parse"))?);
        }
        let (letters, number) = t.split_at(t.len() - 1);
        let tables = Tables::from_exe(&exe);
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
            laps: 3,
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
        };
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
            let mode = ((cy as u32) << 6 | (cx as u32) >> 4) | (((x / 64) | ((y / 256) << 4) | (depth << 7)) as u32) << 16;
            fonts.push((font, mode));
        }
        let line = hwtr_game::line::BestLine::parse(get(&format!("{t}BLD"))?).ok_or("the best line does not parse")?;
        let race = hwtr_game::race::Race::new(setup, scp, line, &parts, tables, tuning);
        tracing::info!("race on {t}: {} car(s) ported, flyby of {} keyframes, countdown from {} ms", race.cars.len(), race.collision.scp.flyby.len(), race.countdown_from);
        Ok(Race {
            scene,
            race,
            renderer: None,
            ahead: 0,
            since_read: 0,
            reader: PadReader::default(),
            mapping: Mapping::default(),
            analog: false,
            fonts,
        })
    }

    /// One display frame of `elapsed`, as race_frame (0x80033ed8) runs it:
    /// race steps of 25 ms until the race's clock passes the real one (a
    /// frame counting for at most 50 ms), each reading the pad first (the
    /// time since the last read going to the first) and driving the
    /// player's car with it.
    pub fn frame(&mut self, pad: &Pad, elapsed: Duration) {
        let ms = elapsed.as_millis().min(u32::MAX as u128) as u32;
        self.since_read = self.since_read.saturating_add(ms);
        self.ahead -= ms.min(50) as i64;
        let state = pad_state(pad, self.analog);
        while self.ahead < 0 {
            self.reader.read(&state, &self.mapping, std::mem::take(&mut self.since_read));
            self.race.step(&[self.reader.controls()]);
            self.ahead += STEP_MS as i64;
        }
        let buttons = Buttons { accept: state.holds(&self.mapping, ACCEPT), start: state.holds(&self.mapping, START) };
        self.race.frame(buttons);
        for event in self.race.events.drain(..) {
            tracing::info!("{event:?} at {} ms", self.race.time);
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
        let pos = Vec3::from_array(at.map(|c| c as f32 / 4096.0));
        let col = |j: usize| Vec3::from_array(body.rot.map(|row| row[j] as f32 / 4096.0));
        (pos, Mat3::from_cols(col(0), col(1), col(2)))
    }

    fn view(&self, aspect: f32) -> Mat4 {
        match self.race.cameras.first() {
            Some(camera) => camera_matrix(camera, aspect),
            None => Mat4::IDENTITY,
        }
    }

    fn car_triangles(&self) -> Vec<hwtr_render::Vtx> {
        let mut tris = Vec::new();
        for (slot, look) in self.scene.cars.iter().enumerate().take(self.race.cars.len()) {
            let (pos, rot) = self.pose(slot);
            tris.extend(hwtr_render::mesh::car_triangles(&look.model, look.clut, look.tpage, pos, rot));
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
        let tris = self.car_triangles();
        let hud = self.race.hud(0);
        let overlay = self.overlay(&hud);
        let mvp = self.view(size.0 as f32 / size.1 as f32);
        let (renderer, _) = self.renderer.as_mut().unwrap();
        renderer.set_moving(device, queue, &tris);
        renderer.set_overlay(device, queue, &overlay);
        renderer.draw(device, queue, target, size, mvp)
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

    /// Where the player's car is drawn, in world units.
    pub fn position(&self) -> [f32; 3] {
        self.pose(0).0.to_array()
    }
}
