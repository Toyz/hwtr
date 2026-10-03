//! hwtr: Hot Wheels Turbo Racing on the PC.
//!
//! `--original` runs the original game in the interpreter (`hwtr-hle`) in the
//! window instead, with the pad, as the reference the port is held to.
//!
//! Otherwise this is the platform bring-up: it opens the disc, puts a window up,
//! presents a PlayStation-sized picture at 4:3, and reads the pad. The
//! pictures are the boot screens read straight from the disc; the ported game
//! takes over from here as its parts land.

mod present;

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
    /// `--original`: the original game, run in the interpreter, as a
    /// reference to hold the port against.
    original: Option<hwtr_hle::Hle>,
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
        if let Some(hle) = &mut self.original {
            hle.pad = pad.buttons;
            hle.sticks = self.input.has_gamepad().then_some([pad.lx, pad.ly, pad.rx, pad.ry]);
            if let Err(e) = hle.frame() {
                tracing::error!("the original stopped: {e:x?}");
                self.original = None;
                return;
            }
            let (w, h, rgba) = hle.hw.borrow().gpu.screen();
            self.screens[0] = ("original".into(), Picture { width: w as u32, height: h as u32, rgba });
            self.screen = 0;
            return;
        }
        if pressed & (buttons::CROSS | buttons::START) != 0 {
            self.screen = (self.screen + 1) % self.screens.len();
            if let Some(w) = &self.win {
                w.window.set_title(&format!("hwtr - {}", self.screens[self.screen].0));
            }
        }
        // Circle runs the motors, to check the pad's rumble end to end.
        let want = pad.held(buttons::CIRCLE);
        if want != self.rumbling {
            self.input.rumble(if want { 0xc0 } else { 0 }, want);
            self.rumbling = want;
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
        let view = frame.texture.create_view(&Default::default());
        let picture = &self.screens[self.screen].1;
        let commands = w.presenter.present(&w.device, &w.queue, picture, &view, w.config.width, w.config.height);
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
        let attrs = Window::default_attributes()
            .with_title(format!("hwtr - {}", self.screens[self.screen].0))
            .with_inner_size(winit::dpi::LogicalSize::new(960.0, 720.0));
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
fn screens(cue: Option<PathBuf>) -> Result<Vec<(String, Picture)>, String> {
    let cue = match cue {
        Some(c) => c,
        None => {
            let dir = std::env::current_dir().map_err(|e| e.to_string())?.join("work/disc");
            hwtr_disc::Disc::find_cue(&dir).map_err(|e| e.to_string())?
        }
    };
    let disc = hwtr_disc::Disc::open(&cue).map_err(|e| e.to_string())?;
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
    let mut cue = None;
    let mut original = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cue" => cue = args.next().map(PathBuf::from),
            "--original" => original = true,
            _ => {
                eprintln!("usage: hwtr [--cue DISC.cue] [--original]");
                std::process::exit(2);
            }
        }
    }
    let original = if original {
        let path = match &cue {
            Some(c) => c.clone(),
            None => hwtr_disc::Disc::find_cue(std::path::Path::new("work/disc")).unwrap_or_default(),
        };
        match hwtr_disc::Disc::open(&path)
            .map_err(|e| e.to_string())
            .and_then(|d| hwtr_hle::Hle::new(std::rc::Rc::new(d)))
        {
            Ok(h) => Some(h),
            Err(e) => {
                tracing::error!("{e}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    let screens = match screens(cue) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };
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
        original,
    };
    if let Err(e) = event_loop.run_app(&mut app) {
        tracing::error!("{e}");
    }
}
