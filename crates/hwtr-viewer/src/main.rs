//! hwtr-viewer: fly around a track as the game draws it.
//!
//! ```text
//! hwtr-viewer TRACK [--mirror] [--sky] [--cue DISC.cue] [--cars A,B,...]
//!             [--at X,Y,Z] [--look YAW,PITCH] [--shot OUT.png]
//! ```
//!
//! TRACK is DESERT1..3, GLACIAL1..3, VOLCANO1..3 or HAUNTED2..3. `--mirror`
//! loads the mirrored world (`.DLW`), `--sky` the sky (`.WLB`) instead of the
//! track. Keys: W A S D move, R F up and down, arrows turn, Shift faster; on a
//! pad the left stick moves and the right stick turns. `--shot` renders one
//! frame offscreen and writes it as PNG.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Vec3};
use hwtr_input::{Input, buttons};
use hwtr_render::scene::{Layout, plain_format};
use hwtr_render::{Renderer, Scene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

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
        hwtr_render::renderer::projection(60f32.to_radians(), aspect, 16.0, 300_000.0) * view
    }
}

fn parse3(s: &str) -> Option<Vec<f32>> {
    s.split(',').map(|v| v.trim().parse().ok()).collect()
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
}

struct App {
    scene: Option<Scene>,
    gpu: Option<Gpu>,
    display: winit::event_loop::OwnedDisplayHandle,
    camera: Camera,
    input: Input,
    keys: std::collections::HashSet<KeyCode>,
    last: Instant,
}

impl App {
    fn step(&mut self, dt: f32) {
        let pad = self.input.read();
        let held = |k: KeyCode| self.keys.contains(&k);
        let axis = |b: u8| {
            let v = (b as f32 - 128.0) / 127.5;
            if v.abs() < 0.15 { 0.0 } else { v }
        };
        let fast = held(KeyCode::ShiftLeft) || pad.held(buttons::R1);
        let speed = if fast { 12000.0 } else { 3000.0 } * dt;
        let mut fwd = -axis(pad.ly);
        let mut side = axis(pad.lx);
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
        if held(KeyCode::KeyR) || pad.held(buttons::R2) {
            up += 1.0;
        }
        if held(KeyCode::KeyF) || pad.held(buttons::L2) {
            up -= 1.0;
        }
        let mut turn = -axis(pad.rx) * 2.0;
        let mut tilt = -axis(pad.ry) * 1.5;
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("hwtr-viewer")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 960.0)),
                )
                .expect("window"),
        );
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(self.display.clone())));
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).expect("device");
        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("config");
        config.format = plain_format(&surface.get_capabilities(&adapter).formats);
        surface.configure(&device, &config);
        let scene = self.scene.take().expect("scene");
        let mut renderer = scene.renderer(&device, &queue, config.format);
        renderer.set_moving(&device, &queue, &scene.car_triangles());
        self.gpu = Some(Gpu { window, surface, config, device, queue, renderer });
        self.last = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => {
                if let Some(g) = &mut self.gpu
                    && s.width > 0
                    && s.height > 0
                {
                    g.config.width = s.width;
                    g.config.height = s.height;
                    g.surface.configure(&g.device, &g.config);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    if k == KeyCode::Escape {
                        event_loop.exit();
                    }
                    if k == KeyCode::KeyP && event.state == ElementState::Pressed {
                        let c = &self.camera;
                        tracing::info!(
                            "--at {:.0},{:.0},{:.0} --look {:.3},{:.3}",
                            c.pos.x,
                            c.pos.y,
                            c.pos.z,
                            c.yaw,
                            c.pitch
                        );
                    }
                    if event.state == ElementState::Pressed {
                        self.keys.insert(k);
                    } else {
                        self.keys.remove(&k);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.1);
                self.last = now;
                self.step(dt);
                let Some(g) = &mut self.gpu else { return };
                let frame = match g.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => {
                        g.surface.configure(&g.device, &g.config);
                        return;
                    }
                };
                let view = frame.texture.create_view(&Default::default());
                let size = (g.config.width, g.config.height);
                let mvp = self.camera.matrix(size.0 as f32 / size.1 as f32);
                let cmd = g.renderer.draw(&g.device, &g.queue, &view, size, mvp);
                g.queue.submit([cmd]);
                g.window.pre_present_notify();
                g.queue.present(frame);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(g) = &self.gpu {
            g.window.request_redraw();
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
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
            tracing::error!("{e}");
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
            std::fs::write(&out, hwtr_data::png::encode(w as usize, h as usize, &rgba)).map_err(|e| e.to_string())
        });
        if let Err(e) = written {
            tracing::error!("{e}");
            std::process::exit(1);
        }
        tracing::info!("-> {}", out.display());
        return;
    }
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        scene: Some(scene),
        gpu: None,
        display: event_loop.owned_display_handle(),
        camera,
        input: Input::new(),
        keys: Default::default(),
        last: Instant::now(),
    };
    if let Err(e) = event_loop.run_app(&mut app) {
        tracing::error!("{e}");
    }
}
