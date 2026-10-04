//! hwtr-viewer: fly around a track as the game draws it.
//!
//! ```text
//! hwtr-viewer TRACK [--mirror] [--sky] [--cue DISC.cue] [--cars A,B,...]
//!             [--at X,Y,Z] [--look YAW,PITCH] [--shot OUT.png]
//! ```
//!
//! TRACK is DESERT1..3, GLACIAL1..3, VOLCANO1..3 or HAUNTED2..3. `--mirror`
//! loads the mirrored world (`.DLW`), `--sky` the sky (`.WLB`) instead of the
//! track. Keys: W A S D move, R F up and down, arrows turn, Shift faster, P
//! logs where the camera is; on a pad the left stick moves, the right stick
//! turns, R1 is faster, R2 and L2 up and down. `--shot` renders one
//! frame offscreen and writes it as PNG.

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::PathBuf;

use hwtr_render::scene::Layout;
use hwtr_render::{Renderer, Scene};
use rrt::glam::{self, Mat4, Vec3};
use rrt::prelude::*;
use rrt::winit::event::{ElementState, WindowEvent};
use rrt::winit::keyboard::{KeyCode, PhysicalKey};

struct Camera {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Camera {
    /// The world's ground is x, y with z up.
    fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.cos() * self.pitch.cos(), self.yaw.sin() * self.pitch.cos(), self.pitch.sin())
    }

    fn matrix(&self, aspect: f32) -> Mat4 {
        let view = glam::camera::rh::view::look_to_mat4(self.pos, self.forward(), Vec3::Z);
        rrt::gpu::projection(60f32.to_radians(), aspect, 16.0, 300_000.0) * view
    }
}

fn parse3(s: &str) -> Option<Vec<f32>> {
    s.split(',').map(|v| v.trim().parse().ok()).collect()
}

struct Viewer {
    scene: Scene,
    renderer: Option<Renderer>,
    camera: Camera,
    /// Keys held, for the free camera's own keys (the pad sees them too).
    keys: HashSet<KeyCode>,
}

impl Game for Viewer {
    fn init(&mut self, ctx: &mut Init<'_>) {
        let (device, queue) = (&ctx.gpu.device, &ctx.gpu.queue);
        let mut renderer = self.scene.renderer(device, queue, ctx.plain_format);
        renderer.set_moving(device, queue, &self.scene.car_triangles());
        self.renderer = Some(renderer);
    }

    fn event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::KeyboardInput { event, .. } = event
            && let PhysicalKey::Code(k) = event.physical_key
        {
            if event.state == ElementState::Pressed {
                if k == KeyCode::KeyP {
                    let c = &self.camera;
                    rrt::tracing::info!(
                        "--at {:.0},{:.0},{:.0} --look {:.3},{:.3}",
                        c.pos.x,
                        c.pos.y,
                        c.pos.z,
                        c.yaw,
                        c.pitch
                    );
                }
                self.keys.insert(k);
            } else {
                self.keys.remove(&k);
            }
        }
        false
    }

    fn tick(&mut self, t: &mut Tick) {
        let dt = t.dt.as_secs_f32();
        let pad = t.pad;
        let held = |k: KeyCode| self.keys.contains(&k);
        let axis = |b: u8| {
            let v = (b as f32 - 128.0) / 127.5;
            if v.abs() < 0.15 { 0.0 } else { v }
        };
        let fast = held(KeyCode::ShiftLeft) || pad.held(Buttons::R1);
        let speed = if fast { 12000.0 } else { 3000.0 } * dt;
        let mut fwd = -axis(pad.left.y);
        let mut side = axis(pad.left.x);
        let mut up = 0.0;
        if held(KeyCode::KeyW) {
            fwd += 1.0;
        }
        if held(KeyCode::KeyS) {
            fwd -= 1.0;
        }
        if held(KeyCode::KeyD) {
            side += 1.0;
        }
        if held(KeyCode::KeyA) {
            side -= 1.0;
        }
        if held(KeyCode::KeyR) || pad.held(Buttons::R2) {
            up += 1.0;
        }
        if held(KeyCode::KeyF) || pad.held(Buttons::L2) {
            up -= 1.0;
        }
        let mut turn = -axis(pad.right.x) * 2.0;
        let mut tilt = -axis(pad.right.y) * 1.5;
        if held(KeyCode::ArrowLeft) {
            turn += 1.5;
        }
        if held(KeyCode::ArrowRight) {
            turn -= 1.5;
        }
        if held(KeyCode::ArrowUp) {
            tilt += 1.0;
        }
        if held(KeyCode::ArrowDown) {
            tilt -= 1.0;
        }
        let c = &mut self.camera;
        c.yaw += turn * dt;
        c.pitch = (c.pitch + tilt * dt).clamp(-1.5, 1.5);
        let right = Vec3::new(c.yaw.sin(), -c.yaw.cos(), 0.0);
        c.pos += (c.forward() * fwd + right * side + Vec3::Z * up) * speed;
    }

    fn draw(&mut self, d: &mut Draw<'_>) {
        let Some(renderer) = &mut self.renderer else { return };
        let size = (d.width, d.height);
        let mvp = self.camera.matrix(size.0 as f32 / size.1 as f32);
        let cmd = renderer.draw(&d.gpu.device, &d.gpu.queue, d.plain, size, mvp);
        d.submit(cmd);
    }
}

fn main() {
    let config = Config::default().title("hwtr-viewer").size((1280, 960)).hz(60.0);
    rrt::app::init_logging(&config.log);
    let mut args = std::env::args().skip(1);
    let (mut track, mut layout, mut cue, mut at, mut look, mut out) = (None, Layout::Normal, None, None, None, None);
    let mut cars: Vec<String> =
        ["deora", "twinmill", "rocket", "bisector", "snake", "hw500"].iter().map(|s| s.to_string()).collect();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--mirror" => layout = Layout::Mirror,
            "--sky" => layout = Layout::Sky,
            "--cue" => cue = args.next().map(PathBuf::from),
            "--at" => at = args.next().and_then(|s| parse3(&s)),
            "--look" => look = args.next().and_then(|s| parse3(&s)),
            "--shot" => out = args.next().map(PathBuf::from),
            "--cars" => {
                cars = args
                    .next()
                    .map(|s| s.split(',').filter(|c| !c.is_empty()).map(str::to_string).collect())
                    .unwrap_or_default()
            }
            t if !t.starts_with('-') => track = Some(t.to_string()),
            _ => {
                eprintln!(
                    "usage: hwtr-viewer TRACK [--mirror] [--sky] [--at X,Y,Z] [--look YAW,PITCH] [--shot OUT.png]"
                );
                std::process::exit(2);
            }
        }
    }
    let track = track.unwrap_or_else(|| "DESERT1".into());
    let cue: Option<PathBuf> = cue;
    let loaded = cue
        .map(Ok)
        .unwrap_or_else(|| rrt::disc::Image::find(std::path::Path::new("work/disc")).map_err(|e| e.to_string()))
        .and_then(|cue| Scene::load(&cue, &track, layout, &cars));
    let scene = match loaded {
        Ok(s) => s,
        Err(e) => {
            rrt::tracing::error!("{e}");
            std::process::exit(1);
        }
    };
    let mut camera = Camera { pos: scene.start, yaw: std::f32::consts::FRAC_PI_2, pitch: -0.1 };
    if let Some(p) = at.filter(|p| p.len() == 3) {
        camera.pos = Vec3::new(p[0], p[1], p[2]);
    }
    if let Some(l) = look.filter(|l| l.len() == 2) {
        camera.yaw = l[0];
        camera.pitch = l[1];
    }
    if let Some(out) = out {
        let (w, h) = (1280, 960);
        let written = scene.shot(w, h, camera.matrix(w as f32 / h as f32)).and_then(|rgba| {
            std::fs::write(&out, rrt::image::png::encode(w, h, &rgba)).map_err(|e| e.to_string())
        });
        if let Err(e) = written {
            rrt::tracing::error!("{e}");
            std::process::exit(1);
        }
        rrt::tracing::info!("-> {}", out.display());
        return;
    }
    let viewer = Viewer { scene, renderer: None, camera, keys: Default::default() };
    if let Err(e) = rrt::app::run(config, viewer) {
        rrt::tracing::error!("{e}");
        std::process::exit(1);
    }
}
