//! The race, as far as it is ported: the scene from the disc and a camera
//! behind the player's car.

use std::path::Path;

use glam::{Mat4, Vec3};
use hwtr_input::Pad;
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
    renderer: Option<(Renderer, wgpu::TextureFormat)>,
    camera: Chase,
}

impl Race {
    pub fn load(cue: &Path, track: &str) -> Result<Race, String> {
        let cars: Vec<String> = CARS.iter().map(|c| c.to_string()).collect();
        let scene = Scene::load(cue, track, Layout::Normal, &cars)?;
        if scene.cars.is_empty() {
            return Err(format!("{track} has no start grid"));
        }
        Ok(Race { scene, renderer: None, camera: Chase::default() })
    }

    /// One frame: the right stick turns the camera about the car.
    pub fn frame(&mut self, pad: &Pad) {
        let axis = (pad.rx as f32 - 128.0) / 127.5;
        if axis.abs() > 0.15 {
            self.camera.around -= axis * 0.05;
        }
    }

    fn view(&self, aspect: f32) -> Mat4 {
        let player = &self.scene.cars[0];
        // A car model's nose points along its +y.
        self.camera.matrix(player.pos, player.rot * Vec3::Y, aspect)
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
            let mut renderer = self.scene.renderer(device, queue, format);
            renderer.set_moving(device, queue, &self.scene.car_triangles());
            self.renderer = Some((renderer, format));
        }
        let mvp = self.view(size.0 as f32 / size.1 as f32);
        let (renderer, _) = self.renderer.as_mut().unwrap();
        renderer.draw(device, queue, target, size, mvp)
    }

    /// The first frame, offscreen, as RGBA.
    pub fn shot(&self, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.scene.shot(width, height, self.view(width as f32 / height as f32))
    }
}
