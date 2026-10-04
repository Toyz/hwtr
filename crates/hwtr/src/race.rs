//! The race, as far as it is ported: the track and cars from the disc, the
//! player's car driven by the ported game logic, a camera behind it.

use std::path::Path;
use std::time::Duration;

use glam::{Mat3, Mat4, Vec3};
use hwtr_game::car::{Controls, Tuning, handling};
use hwtr_game::collision::Scp;
use hwtr_game::math::Tables;
use hwtr_game::race::{Driver, Entrant, RaceSetup, STEP_MS};
use hwtr_input::{Pad, buttons};
use hwtr_render::scene::Layout;
use hwtr_render::{Renderer, Scene};

/// The cars on the grid, the player's first.
const CARS: [&str; 6] = ["deora", "twinmill", "rocket", "bisector", "snake", "hw500"];

/// A camera that follows a car from behind and above, turned about it by
/// the right stick.
#[derive(Default)]
struct Chase {
    /// Radians about the car, 0 behind it.
    around: f32,
}

impl Chase {
    const DISTANCE: f32 = 480.0;
    const HEIGHT: f32 = 170.0;
    const LOOK_ABOVE: f32 = 60.0;

    /// The view-projection from behind a car at `pos` facing `forward`.
    fn matrix(&self, pos: Vec3, forward: Vec3, aspect: f32) -> Mat4 {
        let flat = Vec3::new(forward.x, forward.y, 0.0).normalize_or(Vec3::Y);
        let back = glam::Quat::from_rotation_z(self.around) * -flat;
        let eye = pos + back * Self::DISTANCE + Vec3::Z * Self::HEIGHT;
        let view = glam::camera::rh::view::look_at_mat4(eye, pos + Vec3::Z * Self::LOOK_ABOVE, Vec3::Z);
        hwtr_render::renderer::projection(60f32.to_radians(), aspect, 16.0, 300_000.0) * view
    }
}

pub struct Race {
    scene: Scene,
    race: hwtr_game::race::Race,
    renderer: Option<(Renderer, wgpu::TextureFormat)>,
    camera: Chase,
    /// Time not yet stepped.
    pending: Duration,
}

/// A pad as the race's actions (0 to 255 each): the left stick or the
/// d-pad steers, Cross or R2 accelerates, Square or L2 brakes, Circle or R1
/// is the handbrake, and the left stick is also the stick in the air.
fn controls(pad: &Pad) -> Controls {
    let held = |b: u16| if pad.held(b) { 255 } else { 0 };
    let side = |v: u8, positive: bool| -> u8 {
        let d = v as i32 - 128;
        let d = if positive { d } else { -d };
        (d.max(0) * 2).min(255) as u8
    };
    Controls {
        steer_right: side(pad.lx, true).max(held(buttons::RIGHT)),
        steer_left: side(pad.lx, false).max(held(buttons::LEFT)),
        accelerate: held(buttons::CROSS).max(held(buttons::R2)),
        brake: held(buttons::SQUARE).max(held(buttons::L2)),
        stick_across: [side(pad.lx, true), side(pad.lx, false)],
        stick_along: [side(pad.ly, true), side(pad.ly, false)],
        handbrake: (held(buttons::CIRCLE) | held(buttons::R1)) & 1,
        ..Controls::default()
    }
}

impl Race {
    pub fn load(cue: &Path, track: &str) -> Result<Race, String> {
        let cars: Vec<String> = CARS.iter().map(|c| c.to_string()).collect();
        let scene = Scene::load(cue, track, Layout::Normal, &cars)?;
        if scene.cars.is_empty() {
            return Err(format!("{track} has no start grid"));
        }
        let disc = hwtr_disc::Disc::open(cue).map_err(|e| e.to_string())?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let read = |name: &str| iso.find(name).and_then(|e| iso.read(&e)).map_err(|e| e.to_string());
        let exe = hwtr_psx::Exe::parse(&read("CCCPSX.EXE")?).map_err(|e| e.to_string())?;
        let big = read("CCCPSX.BIG")?;
        let big = hwtr_data::Big::parse(&big).map_err(|e| e.to_string())?;
        let t = track.to_uppercase();
        let get = |name: &str| big.lookup(&format!("{t}BIG/{name}")).ok_or(format!("{name} is not in {t}.BIG"));
        let scp = Scp::parse(get(&format!("{t}SCP"))?).ok_or("the SCP does not parse")?;
        let tuning = Tuning::from_prm(get("TUNINGPRM")?);
        // The player alone for now, on the first grid place.
        let name = CARS[0];
        let bmf = get(&format!("{}BMF", name.to_uppercase()))?;
        let cwh = hwtr_data::car::CarBmf::parse(bmf).map_err(|e| e.to_string())?.cwh;
        let parts = handling::parse_cwh(cwh).ok_or("the CWH does not parse")?;
        let (letters, number) = t.split_at(t.len() - 1);
        let setup = RaceSetup {
            flags: 0,
            track: letters.to_string(),
            track_number: number.parse().unwrap_or(1),
            unknown_19: [0; 3],
            options: 0,
            cars: vec![Entrant { name: name.into(), driver: Driver::PlayerOne, car_id: 0, player: 0, grid: 0 }],
            difficulty: 128,
        };
        let race = hwtr_game::race::Race::new(setup, scp, &[parts], Tables::from_exe(&exe), tuning);
        tracing::info!("race on {t}: {} car(s) ported", race.cars.len());
        Ok(Race { scene, race, renderer: None, camera: Chase::default(), pending: Duration::ZERO })
    }

    /// One display frame of `elapsed`: the pad drives the player's car for
    /// as many race steps as have come due; the right stick turns the camera.
    pub fn frame(&mut self, pad: &Pad, elapsed: Duration) {
        let axis = (pad.rx as f32 - 128.0) / 127.5;
        if axis.abs() > 0.15 {
            self.camera.around -= axis * 0.05;
        }
        let step = Duration::from_millis(STEP_MS as u64);
        self.pending = (self.pending + elapsed).min(step * 8);
        let c = controls(pad);
        while self.pending >= step {
            self.race.step(&[c]);
            self.pending -= step;
        }
    }

    /// The player's car where the game draws it (0x80049ecc): its body's
    /// position and centre less its turned wheel origin, in world units; its
    /// rotation's columns its axes.
    fn player_pose(&self) -> (Vec3, Mat3) {
        let car = &self.race.cars[0];
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
        let (pos, rot) = self.player_pose();
        self.camera.matrix(pos, rot.y_axis, aspect)
    }

    fn car_triangles(&self) -> Vec<hwtr_render::Vtx> {
        let (pos, rot) = self.player_pose();
        let player = &self.scene.cars[0];
        let mut tris = hwtr_render::mesh::car_triangles(&player.model, player.clut, player.tpage, pos, rot);
        // The other cars wait on the grid until computer cars are ported.
        tris.extend(self.scene.cars[1..].iter().flat_map(|c| c.triangles()));
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
        let mvp = self.view(size.0 as f32 / size.1 as f32);
        let (renderer, _) = self.renderer.as_mut().unwrap();
        renderer.set_moving(device, queue, &tris);
        renderer.draw(device, queue, target, size, mvp)
    }

    /// After `steps` race steps with the accelerator down, one frame,
    /// offscreen, as RGBA; and where the player's car is.
    pub fn shot(&mut self, width: u32, height: u32, steps: u32) -> Result<(Vec<u8>, [f32; 3]), String> {
        let pad = Pad { buttons: buttons::CROSS, ..Pad::default() };
        for _ in 0..steps {
            self.race.step(&[controls(&pad)]);
        }
        let (pos, _) = self.player_pose();
        let tris = self.car_triangles();
        let rgba = self.scene.shot_with(width, height, self.view(width as f32 / height as f32), &tris)?;
        Ok((rgba, pos.to_array()))
    }
}
