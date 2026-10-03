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

mod render;
mod scene;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Vec3};
use hwtr_data::world::World;
use hwtr_input::{Input, buttons};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use render::Renderer;
use scene::{Vram, Vtx};

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
        glam::camera::rh::proj::directx::perspective(60f32.to_radians(), aspect, 16.0, 300_000.0) * view
    }
}

struct Loaded {
    vram: Vram,
    tris: Vec<Vtx>,
    start: Vec3,
    background: [u8; 3],
}

fn load(track: &str, ext: &str, cars: &[String], cue: Option<PathBuf>) -> Result<Loaded, String> {
    let cue = match cue {
        Some(c) => c,
        None => hwtr_disc::Disc::find_cue(&std::env::current_dir().map_err(|e| e.to_string())?.join("work/disc"))
            .map_err(|e| e.to_string())?,
    };
    let disc = hwtr_disc::Disc::open(&cue).map_err(|e| e.to_string())?;
    let iso = disc.iso().map_err(|e| e.to_string())?;
    let big = iso.find("CCCPSX.BIG").and_then(|e| iso.read(&e)).map_err(|e| e.to_string())?;
    let big = hwtr_data::Big::parse(&big).map_err(|e| e.to_string())?;
    let t = track.to_uppercase();
    let get = |name: &str| big.lookup(&format!("{t}BIG/{name}")).ok_or(format!("{name} is not in {t}.BIG"));
    let mut vram = Vram::new();
    for tex in [format!("{t}GLM"), format!("{t}GLB"), "SFXGLM".to_string()] {
        for (x, y, w, h, data) in hwtr_data::world::read_vram_blocks(get(&tex)?).map_err(|e| e.to_string())? {
            vram.load(x, y, w, h, &data);
        }
    }
    let world = World::parse(get(&format!("{t}{ext}"))?).map_err(|e| e.to_string())?;
    tracing::info!(
        "{t}.{ext}: {}x{} cells, {} polygons, {} objects, {} pickups",
        world.grid_w,
        world.grid_h,
        world.poly_count(),
        world.objects.len(),
        world.pickups.len()
    );
    // The game's world is right-handed with z up, the viewer's too: no
    // conversion. (The polygons' front faces, by the game's own NCLIP rule,
    // point +z on four horizontal faces in five; worklog 12.)
    let mut tris = scene::world_triangles(&world);
    // Start on the race's first grid position (the SCP's first start point,
    // 20.12), a little above the road; otherwise above the middle.
    let start = match get(&format!("{t}SCP")) {
        Ok(scp) if ext != "WLB" && scp.len() >= 40 => {
            let c = |k: usize| i32::from_le_bytes(scp[24 + 4 * k..28 + 4 * k].try_into().unwrap()) as f32 / 4096.0;
            let x = if ext == "DLW" { -c(0) } else { c(0) };
            Vec3::new(x, c(1), c(2) + 300.0)
        }
        _ if world.cells.is_empty() => Vec3::ZERO,
        _ => {
            let c = scene::centre(&world);
            Vec3::new(c.x, c.y - 9000.0, c.z + 5000.0)
        }
    };
    // Cars on the start grid: the SCP's six start points and orientations
    // (quaternions x, y, z, w with 4096 as 1).
    if let Ok(scp) = get(&format!("{t}SCP"))
        && ext != "WLB"
    {
        let word = |at: usize| i32::from_le_bytes(scp[at..at + 4].try_into().unwrap()) as f32 / 4096.0;
        for (slot, name) in cars.iter().take(6).enumerate() {
            let n = name.to_uppercase();
            let tim = hwtr_data::Tim::parse(get(&format!("{n}TIM"))?).map_err(|e| e.to_string())?;
            let bmf = get(&format!("{n}BMF"))?;
            let model = hwtr_data::car::CarBmf::parse(bmf)
                .and_then(|b| hwtr_data::car::Model::parse(b.models[0]))
                .map_err(|e| e.to_string())?;
            let (clut, tpage) = scene::place_car_texture(&mut vram, &tim, slot);
            let p = 24 + 16 * slot;
            let q = 120 + 16 * slot;
            let mut pos = Vec3::new(word(p), word(p + 4), word(p + 8));
            let mut rot = glam::Quat::from_xyzw(word(q), word(q + 4), word(q + 8), word(q + 12)).normalize();
            if ext == "DLW" {
                pos.x = -pos.x;
                rot = glam::Quat::from_xyzw(rot.x, -rot.y, -rot.z, rot.w);
            }
            tris.extend(scene::car_triangles(&model, clut, tpage, pos, rot));
        }
    }
    Ok(Loaded { vram, tris, start, background: world.background })
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
    loaded: Option<Loaded>,
    gpu: Option<Gpu>,
    display: winit::event_loop::OwnedDisplayHandle,
    camera: Camera,
    input: Input,
    keys: std::collections::HashSet<KeyCode>,
    last: Instant,
}

/// A surface format that stores the shader's values as they are: the
/// PlayStation's colours are already display-encoded.
fn plain_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(formats[0])
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
        let loaded = self.loaded.take().expect("scene");
        let mut renderer = Renderer::new(&device, &queue, config.format, &loaded.vram, &loaded.tris);
        renderer.clear = loaded.background;
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

/// Renders one frame offscreen and writes it as PNG.
fn shot(loaded: &Loaded, camera: &Camera, out: &PathBuf) -> Result<(), String> {
    let (w, h) = (1280u32, 960u32);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).map_err(|e| e.to_string())?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&device, &queue, format, &loaded.vram, &loaded.tris);
    renderer.clear = loaded.background;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shot"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let cmd = renderer.draw(&device, &queue, &view, (w, h), camera.matrix(w as f32 / h as f32));
    let row = (w * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([cmd, enc.finish()]);
    readback.map_async(wgpu::MapMode::Read, .., |r| r.expect("map readback"));
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
    let data = readback.get_mapped_range(..).map_err(|e| e.to_string())?;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        rgba.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
    }
    std::fs::write(out, hwtr_data::png::encode(w as usize, h as usize, &rgba)).map_err(|e| e.to_string())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let (mut track, mut ext, mut cue, mut at, mut look, mut out) = (None, "WLD", None, None, None, None);
    let mut cars: Vec<String> =
        ["deora", "twinmill", "rocket", "bisector", "snake", "hw500"].iter().map(|s| s.to_string()).collect();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--mirror" => ext = "DLW",
            "--sky" => ext = "WLB",
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
    let loaded = match load(&track, ext, &cars, cue) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };
    let mut camera = Camera { pos: loaded.start, yaw: std::f32::consts::FRAC_PI_2, pitch: -0.1 };
    if let Some(p) = at.filter(|p| p.len() == 3) {
        camera.pos = Vec3::new(p[0], p[1], p[2]);
    }
    if let Some(l) = look.filter(|l| l.len() == 2) {
        camera.yaw = l[0];
        camera.pitch = l[1];
    }
    if let Some(out) = out {
        if let Err(e) = shot(&loaded, &camera, &out) {
            tracing::error!("{e}");
            std::process::exit(1);
        }
        tracing::info!("-> {}", out.display());
        return;
    }
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        loaded: Some(loaded),
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
