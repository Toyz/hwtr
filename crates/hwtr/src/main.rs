//! hwtr: Hot Wheels Turbo Racing on the PC, the port and only the port.
//!
//! Everything it runs is ported Rust code; the original game, run in the
//! interpreter, stays in `hwtr-hle` and the tests, where the port is checked
//! against it.
//!
//! What it does so far: the boot screens from the disc (Start or Cross steps
//! through them), then the race scene: the track and the cars on the start
//! grid, drawn natively, with a camera behind the player's car (the right
//! stick turns it, Select goes back). The player's car drives on the ported
//! physics and wheel collision (left stick or d-pad steers, Cross or R2
//! accelerates, Square or L2 brakes, Circle or R1 the handbrake); the other
//! cars wait until computer cars are ported, and there are no walls yet.
//!
//! ```text
//! hwtr [--cue DISC.cue] [--track NAME] [--shot OUT.png [--steps N]]
//! ```
//!
//! `--track` goes straight to a race on that track (DESERT1 by default);
//! `--shot` renders that race offscreen as PNG and exits, after `--steps`
//! race steps (25 ms each) with the accelerator held.

#![forbid(unsafe_code)]

mod present;
mod race;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hwtr_input::{Input, Pad, buttons};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use present::{Picture, Presenter};
use race::Race;

/// The PlayStation's NTSC field rate.
const FRAME: Duration = Duration::from_nanos(1_000_000_000_000 / 59_940);

struct Display {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    presenter: Presenter,
}

struct App {
    win: Option<Display>,
    instance: Option<wgpu::Instance>,
    display: winit::event_loop::OwnedDisplayHandle,
    input: Input,
    pad: Pad,
    screens: Vec<(String, Picture)>,
    screen: usize,
    next_frame: Instant,
    rumbling: bool,
    cue: PathBuf,
    track: String,
    /// The race, once the boot screens are through.
    race: Option<Race>,
}

fn key_button(code: KeyCode) -> u16 {
    match code {
        KeyCode::ArrowUp => buttons::UP,
        KeyCode::ArrowDown => buttons::DOWN,
        KeyCode::ArrowLeft => buttons::LEFT,
        KeyCode::ArrowRight => buttons::RIGHT,
        KeyCode::KeyZ => buttons::CROSS,
        KeyCode::KeyX => buttons::CIRCLE,
        KeyCode::KeyA => buttons::SQUARE,
        KeyCode::KeyS => buttons::TRIANGLE,
        KeyCode::KeyQ => buttons::L1,
        KeyCode::KeyW => buttons::R1,
        KeyCode::Digit1 => buttons::L2,
        KeyCode::Digit2 => buttons::R2,
        KeyCode::Enter => buttons::START,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => buttons::SELECT,
        _ => 0,
    }
}

impl App {
    /// One frame of the game at the PlayStation's rate.
    fn frame(&mut self) {
        let pad = self.input.read();
        let pressed = pad.buttons & !self.pad.buttons;
        if pad.buttons != self.pad.buttons {
            tracing::debug!(
                "pad {:016b} left ({:3}, {:3}) right ({:3}, {:3})",
                pad.buttons,
                pad.lx,
                pad.ly,
                pad.rx,
                pad.ry
            );
        }
        self.pad = pad;
        if let Some(race) = &mut self.race {
            if pressed & buttons::SELECT != 0 {
                self.race = None;
                self.screen = 0;
                self.set_title();
                return;
            }
            race.frame(&pad, FRAME);
        } else if pressed & (buttons::CROSS | buttons::START) != 0 {
            if self.screen + 1 == self.screens.len() {
                self.start_race();
            } else {
                self.screen += 1;
            }
            self.set_title();
        }
        // Circle runs the motors, to check the pad's rumble end to end.
        let want = pad.held(buttons::CIRCLE);
        if want != self.rumbling {
            self.input.rumble(if want { 0xc0 } else { 0 }, want);
            self.rumbling = want;
        }
    }

    fn set_title(&self) {
        if let Some(w) = &self.win {
            let title = match &self.race {
                Some(_) => format!("hwtr - {}", self.track),
                None => format!("hwtr - {}", self.screens[self.screen].0),
            };
            w.window.set_title(&title);
        }
    }

    fn start_race(&mut self) {
        match Race::load(&self.cue, &self.track) {
            Ok(r) => self.race = Some(r),
            Err(e) => tracing::error!("{e}"),
        }
    }

    fn draw(&mut self) {
        let Some(w) = &mut self.win else { return };
        let frame = match w.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                w.surface.configure(&w.device, &w.config);
                return;
            }
        };
        let commands = if let Some(race) = &mut self.race {
            // The game's colours are already display-encoded: drawn through a
            // view that stores them as they are.
            let plain = w.config.format.remove_srgb_suffix();
            let view =
                frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(plain), ..Default::default() });
            race.draw(&w.device, &w.queue, plain, &view, (w.config.width, w.config.height))
        } else {
            let view = frame.texture.create_view(&Default::default());
            let picture = &self.screens[self.screen].1;
            w.presenter.present(&w.device, &w.queue, picture, &view, w.config.width, w.config.height)
        };
        w.queue.submit([commands]);
        w.window.pre_present_notify();
        w.queue.present(frame);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        let attrs =
            Window::default_attributes().with_title("hwtr").with_inner_size(winit::dpi::LogicalSize::new(960.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let instance = self.instance.get_or_insert_with(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(self.display.clone())))
        });
        let result = (|| -> Result<Display, String> {
            let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            }))
            .map_err(|e| e.to_string())?;
            let (device, queue) = pollster::block_on(
                adapter.request_device(&wgpu::DeviceDescriptor { label: Some("hwtr"), ..Default::default() }),
            )
            .map_err(|e| e.to_string())?;
            let size = window.inner_size();
            let mut config = surface
                .get_default_config(&adapter, size.width.max(1), size.height.max(1))
                .ok_or("the surface is not supported by this adapter")?;
            let caps = surface.get_capabilities(&adapter);
            if let Some(f) = caps.formats.iter().find(|f| f.is_srgb()) {
                config.format = *f;
            }
            config.present_mode = wgpu::PresentMode::AutoVsync;
            config.view_formats = vec![config.format.remove_srgb_suffix()];
            surface.configure(&device, &config);
            let presenter = Presenter::new(&device, config.format);
            Ok(Display { window: window.clone(), surface, config, device, queue, presenter })
        })();
        match result {
            Ok(d) => self.win = Some(d),
            Err(e) => {
                tracing::error!("no GPU: {e}");
                event_loop.exit();
                return;
            }
        }
        self.next_frame = Instant::now();
        self.set_title();
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(w) = &mut self.win
                    && size.width > 0
                    && size.height > 0
                {
                    w.config.width = size.width;
                    w.config.height = size.height;
                    w.surface.configure(&w.device, &w.config);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape {
                        event_loop.exit();
                    }
                    let bit = key_button(code);
                    if event.state == ElementState::Pressed {
                        self.input.keyboard.buttons |= bit;
                    } else {
                        self.input.keyboard.buttons &= !bit;
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let mut steps = 0;
                while self.next_frame <= now && steps < 4 {
                    self.frame();
                    self.next_frame += FRAME;
                    steps += 1;
                }
                if self.next_frame < now {
                    self.next_frame = now;
                }
                self.draw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.win {
            w.window.request_redraw();
        }
    }
}

/// The boot screens, read off the disc: the legal screen, the title, the
/// main menu's background.
fn screens(cue: &std::path::Path) -> Result<Vec<(String, Picture)>, String> {
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
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let (mut cue, mut track, mut shot, mut steps) = (None, None, None, 0u32);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cue" => cue = args.next().map(PathBuf::from),
            "--track" => track = args.next(),
            "--shot" => shot = args.next().map(PathBuf::from),
            "--steps" => steps = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            _ => {
                eprintln!("usage: hwtr [--cue DISC.cue] [--track NAME] [--shot OUT.png [--steps N]]");
                std::process::exit(2);
            }
        }
    }
    let fail = |e: String| -> ! {
        tracing::error!("{e}");
        std::process::exit(1)
    };
    let cue = cue
        .map(Ok)
        .unwrap_or_else(|| rrt::disc::Image::find(std::path::Path::new("work/disc")).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail(e));
    let race = track.is_some() || shot.is_some();
    let track = track.unwrap_or_else(|| "DESERT1".into()).to_uppercase();
    if let Some(out) = shot {
        let (w, h) = (1280, 960);
        let written = Race::load(&cue, &track).and_then(|mut race| race.shot(w, h, steps)).and_then(|(rgba, pos)| {
            std::fs::write(&out, hwtr_data::png::encode(w as usize, h as usize, &rgba)).map_err(|e| e.to_string())?;
            Ok(pos)
        });
        match written {
            Ok(pos) => tracing::info!("-> {} (after {steps} steps the car is at {pos:?})", out.display()),
            Err(e) => fail(e),
        }
        return;
    }
    let screens = screens(&cue).unwrap_or_else(|e| fail(e));
    let input = Input::new();
    for name in input.gamepads() {
        tracing::info!("gamepad: {name}");
    }
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        win: None,
        instance: None,
        display: event_loop.owned_display_handle(),
        input,
        pad: Pad::default(),
        screens,
        screen: 0,
        next_frame: Instant::now(),
        rumbling: false,
        cue,
        track,
        race: None,
    };
    if race {
        app.start_race();
    }
    if let Err(e) = event_loop.run_app(&mut app) {
        tracing::error!("{e}");
    }
}
