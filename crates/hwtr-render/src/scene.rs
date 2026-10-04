//! A race's scene from the disc: the track, its textures, and the cars on
//! the start grid.

use std::path::Path;

use hwtr_data::car::{CarBmf, Model};
use hwtr_data::world::World;
use rrt::glam::{self, Quat, Vec3};
use rrt::gpu::{Gpu, Target};
use rrt::wgpu;

use crate::mesh::{self, Vram, Vtx};
use crate::renderer::Renderer;

/// Which of a track's worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// The track (`.WLD`).
    Normal,
    /// The mirrored track (`.DLW`).
    Mirror,
    /// The sky (`.WLB`).
    Sky,
}

impl Layout {
    fn extension(self) -> &'static str {
        match self {
            Layout::Normal => "WLD",
            Layout::Mirror => "DLW",
            Layout::Sky => "WLB",
        }
    }
}

/// A car as the scene draws it.
pub struct SceneCar {
    pub model: Model,
    /// Its skin's palette and texture page in VRAM.
    pub clut: u16,
    pub tpage: u16,
    pub pos: Vec3,
    pub rot: Quat,
}

impl SceneCar {
    pub fn triangles(&self) -> Vec<Vtx> {
        mesh::car_triangles(&self.model, self.clut, self.tpage, self.pos, glam::Mat3::from_quat(self.rot), 0x80_80_80)
    }
}

pub struct Scene {
    /// The track's name and number ("DESERT1"), whose archive the scene
    /// came from.
    pub track: String,
    pub vram: Vram,
    /// The track's triangles, without the objects drawn apart: the
    /// pickups (drawn while out) and the moving objects, each with where it
    /// hangs, from the world kept for drawing them.
    pub track_tris: Vec<Vtx>,
    pub apart: Vec<(usize, mesh::Pose)>,
    pub world: World,
    /// The cars on the start grid, in grid order.
    pub cars: Vec<SceneCar>,
    /// Somewhere to look from: above the first grid place, or above the
    /// middle of a world with no grid.
    pub start: Vec3,
    pub background: [u8; 3],
}

impl Scene {
    /// Loads `track` (DESERT1..3, GLACIAL1..3, VOLCANO1..3, HAUNTED2..3) from
    /// the disc at `cue`, with `cars` (by name: deora, twinmill, ...) on the
    /// grid, up to six.
    pub fn load(cue: &Path, track: &str, layout: Layout, cars: &[String]) -> Result<Scene, String> {
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
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
        let ext = layout.extension();
        let world = World::parse(get(&format!("{t}{ext}"))?).map_err(|e| e.to_string())?;
        tracing::info!(
            "{t}.{ext}: {}x{} cells, {} polygons, {} objects, {} pickups",
            world.grid_w,
            world.grid_h,
            world.poly_count(),
            world.objects.len(),
            world.pickups.len()
        );
        // The game's world is right-handed with z up, like this renderer's:
        // no conversion (worklog 12).
        let mut skip: Vec<usize> = world.pickups.iter().filter_map(|p| p.object).collect();
        // The moving objects, and those a car can knock over.
        let knockable = world.volumes.iter().filter(|v| v.flags & 1 != 0).filter_map(|v| v.object);
        for o in world.anims.iter().filter_map(|a| a.object).chain(knockable) {
            if !skip.contains(&o) {
                skip.push(o);
            }
        }
        let (track_tris, parents) = mesh::world_triangles_apart(&world, &skip);
        let apart = skip.into_iter().zip(parents).collect();
        // The grid: the SCP's start points (20.12) from byte 24 and its
        // orientations (quaternions x, y, z, w) from byte 120, 16 bytes each.
        let scp = (layout != Layout::Sky).then(|| get(&format!("{t}SCP")).ok()).flatten().filter(|s| s.len() >= 216);
        let word = |s: &[u8], at: usize| i32::from_le_bytes(s[at..at + 4].try_into().unwrap()) as f32 / 4096.0;
        let mirror = layout == Layout::Mirror;
        let grid = |slot: usize| {
            scp.map(|s| {
                let (p, q) = (24 + 16 * slot, 120 + 16 * slot);
                let mut pos = Vec3::new(word(s, p), word(s, p + 4), word(s, p + 8));
                let mut rot = Quat::from_xyzw(word(s, q), word(s, q + 4), word(s, q + 8), word(s, q + 12)).normalize();
                if mirror {
                    pos.x = -pos.x;
                    rot = Quat::from_xyzw(rot.x, -rot.y, -rot.z, rot.w);
                }
                (pos, rot)
            })
        };
        let start = match grid(0) {
            Some((pos, _)) => pos + Vec3::Z * 300.0,
            None if world.cells.is_empty() => Vec3::ZERO,
            None => mesh::centre(&world) + Vec3::new(0.0, -9000.0, 5000.0),
        };
        let mut scene_cars = Vec::new();
        for (slot, name) in cars.iter().take(6).enumerate() {
            let Some((pos, rot)) = grid(slot) else { break };
            let n = name.to_uppercase();
            let tim = hwtr_data::Tim::parse(get(&format!("{n}TIM"))?).map_err(|e| e.to_string())?;
            let model = CarBmf::parse(get(&format!("{n}BMF"))?)
                .and_then(|b| Model::parse(b.models[0]))
                .map_err(|e| e.to_string())?;
            let (clut, tpage) = mesh::place_car_texture(&mut vram, &tim, slot);
            scene_cars.push(SceneCar { model, clut, tpage, pos, rot });
        }
        let background = world.background;
        Ok(Scene { track: t, vram, track_tris, apart, world, cars: scene_cars, start, background })
    }

    /// Every car's triangles.
    pub fn car_triangles(&self) -> Vec<Vtx> {
        self.cars.iter().flat_map(SceneCar::triangles).collect()
    }

    /// A renderer for this scene's track; the cars go in with `set_moving`.
    pub fn renderer(&self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Renderer {
        let mut renderer = Renderer::new(device, queue, format, &self.vram, &self.track_tris);
        renderer.clear = self.background;
        renderer
    }

    /// Renders one frame offscreen with the view-projection `mvp`, the cars
    /// where the scene has them, and returns it as RGBA.
    pub fn shot(&self, width: u32, height: u32, mvp: glam::Mat4) -> Result<Vec<u8>, String> {
        self.shot_with(width, height, mvp, &self.car_triangles())
    }

    /// The same with `moving` (the cars, wherever they are) instead.
    pub fn shot_with(&self, width: u32, height: u32, mvp: glam::Mat4, moving: &[Vtx]) -> Result<Vec<u8>, String> {
        let gpu = Gpu::headless().map_err(|e| e.to_string())?;
        let target = Target::new(&gpu.device, width, height);
        let mut renderer = self.renderer(&gpu.device, &gpu.queue, Target::FORMAT);
        renderer.set_moving(&gpu.device, &gpu.queue, moving);
        let cmd = renderer.draw(&gpu.device, &gpu.queue, &target.view, (width, height), mvp);
        gpu.queue.submit([cmd]);
        Ok(target.read_back(&gpu).rgba)
    }
}
