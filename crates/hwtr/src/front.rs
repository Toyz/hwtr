//! The front end on screen: the ported menus ([`hwtr_game::front`]) drawn
//! as the console draws them, the background picture where the game loads
//! it in VRAM (640, 0) with its palette at (640, 457), the screen font at
//! (960, 0) with its palette at (640, 458), on a 640 by 240 screen.
//!
//! The memory cards are folders, `work/memcard/slot1` and `slot2`, each
//! holding the game's card file under its card name,
//! `BASLUS-00964HTWHEELS`, byte for byte. Slot 1 always has a card (with
//! no file there, the game offers to make one, as on a console); slot 2
//! has one when its folder is there.

use std::path::Path;

use rrt::wgpu;
use hwtr_game::front::{card, Files, Frame, Front};
use hwtr_game::hud::Font;
use hwtr_game::pad::PadState;
use hwtr_render::mesh::Vram;
use hwtr_render::{Renderer, Vtx};

const SCREEN: (f32, f32) = (640.0, 240.0);
const PICTURE_AT: (u16, u16) = (640, 0);
const PICTURE_CLUT: (u16, u16) = (640, 457);
const FONT_AT: (u16, u16) = (960, 0);
const FONT_CLUT: (u16, u16) = (640, 458);

/// The memory cards in slots 1 and 2.
pub const CARD_DIRS: [&str; 2] = ["work/memcard/slot1", "work/memcard/slot2"];

pub struct FrontEnd {
    pub front: Front,
    /// `CCCPSX.BIG`, where the pictures are.
    big: Vec<u8>,
    vram: Vram,
    font: Font,
    font_mode: u32,
    /// The picture in VRAM, and its size.
    picture: Option<String>,
    picture_size: (u16, u16),
    renderer: Option<(Renderer, wgpu::TextureFormat)>,
    stale: bool,
    /// The card file's title frame, ready for a save.
    card_header: [u8; card::HEADER_SIZE],
    /// The 3D pieces' models and textures, and the trigonometry they turn by.
    scr: hwtr_data::scr::Scr,
    textures: crate::pieces::Textures,
    tables: hwtr_game::math::Tables,
    /// The menus' sounds: the sound chip, `HWMENU`'s bank in it, the note
    /// tables, and each effect's tone, program and note (0x8011aec0).
    spu: Option<std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>>,
    effects: crate::spu::Effects,
    /// The music bank the boot chose (0x800180ac: one of six at random).
    music: hwtr_game::snd::Bank,
    music_samples: std::sync::Arc<[u8]>,
    /// How far each car's preview is shifted to centre it (from
    /// `CWHS.BMF`).
    car_centres: Vec<[i32; 2]>,
    /// Every car's preview model and skin, parsed once at load.
    cars: Vec<Option<(hwtr_data::car::Model, hwtr_data::Tim)>>,
    /// The car each player's preview has in VRAM, with its model.
    previews: [Option<(u8, crate::pieces::PreviewCar)>; 2],
    /// Every car's picture (`DECALS.BMF`, 192 by 64 15-bit pixels), and the
    /// car each slot has in VRAM.
    decals: Vec<Vec<u16>>,
    decal_slots: [Option<u8>; 2],
}

/// Where each player's preview skin goes in VRAM (the PlayStation's frame
/// buffers, unused here), and its palette.
const PREVIEW_AT: [(u16, u16); 2] = [(0, 0), (128, 0)];
const PREVIEW_CLUT: [(u16, u16); 2] = [(0, 480), (0, 481)];

/// Where the car pictures go in VRAM, a slot under the other (0x800d0d24),
/// and their size.
const DECAL_AT: (u16, u16) = (640, 256);
const DECAL_SIZE: (u16, u16) = (192, 64);


/// A texture page and palette as `Vtx::mode` takes them.
fn mode(at: (u16, u16), clut: (u16, u16), depth: u32) -> u32 {
    let page = (at.0 as u32 / 64) | ((at.1 as u32 / 256) << 4) | (depth << 7);
    ((clut.1 as u32) << 6 | (clut.0 as u32) >> 4) | page << 16
}

impl FrontEnd {
    pub fn load(cue: &Path, spu: Option<std::sync::Arc<std::sync::Mutex<crate::spu::Spu>>>) -> Result<FrontEnd, String> {
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
        let iso = disc.iso().map_err(|e| e.to_string())?;
        let read = |p: &str| iso.find(p).and_then(|e| iso.read(&e)).map_err(|e| e.to_string());
        let exe = hwtr_psx::Exe::parse(&read("CCCPSX.EXE")?).map_err(|e| e.to_string())?;
        let big_bytes = read("CCCPSX.BIG")?;
        let big = hwtr_data::Big::parse(&big_bytes).map_err(|e| e.to_string())?;
        let get = |name: &str| big.lookup(&format!("SCREENSBIG/{name}")).ok_or(format!("{name} is not in SCREENS.BIG"));
        let font = Font::parse(get("SCRNFNTOVL")?).ok_or("SCRNFNT.OVL does not parse")?;
        let mut vram = Vram::new();
        let tim = hwtr_data::Tim::parse(&font.tim).map_err(|e| e.to_string())?;
        vram.load(FONT_AT.0, FONT_AT.1, tim.rect.w, tim.rect.h, &tim.data);
        if let Some((r, colours)) = &tim.clut {
            vram.load(FONT_CLUT.0, FONT_CLUT.1, r.w, 1, &colours[..r.w as usize]);
        }
        let depth = match tim.mode {
            hwtr_data::tim::Mode::Bpp4 => 0,
            hwtr_data::tim::Mode::Bpp8 => 1,
            _ => 2,
        };
        let view = exe.view();
        let byte = |a: u32| view.u8(a).unwrap_or(0);
        let card_files = CARD_DIRS.map(|dir| {
            let path = Path::new(dir).join(card::FILE_NAME);
            let file = std::fs::read(&path).ok();
            if file.is_some() {
                rrt::tracing::info!("memory card: {}", path.display());
            }
            file
        });
        let icons = [get("MEM1TIM")?, get("MEM2TIM")?, get("MEM3TIM")?];
        let card_header = card::header(&byte, icons);
        let scr = hwtr_data::scr::Scr::parse(get("SCREENSSCR")?).ok_or("SCREENS.SCR does not parse")?;
        let textures = crate::pieces::Textures::load(get("SCREENSGLM")?, &byte, &mut vram);
        let tables = hwtr_game::math::Tables::from_exe(&exe);
        let bank = hwtr_game::snd::Bank::from_vh(get("HWMENUVH")?, 0).ok_or("HWMENU.VH does not parse")?;
        let effects = crate::spu::Effects::new(&byte, bank, get("HWMENUVB")?);
        let files = Files {
            strings: get("ENGLISHHWT")?,
            tuning: get("TUNINGPRM")?,
            cars: get("ENGCARSCDT")?,
            font: &font,
            card: card_files[0].as_deref(),
            card2: card_files[1].as_deref(),
            // Slot 1 always has a card; slot 2 when its folder is there.
            card_slots: [true, Path::new(CARD_DIRS[1]).is_dir()],
            cwhs: get("CWHSBMF").ok(),
            name_keys: get("ENGNAMECHM").ok(),
            password_keys: get("ENGPWDCHM").ok(),
        };
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_millis() as u32);
        let mut front = Front::new(&byte, files, seed).ok_or("the front end does not load")?;
        // 0x800180ac: the music bank, by the table at 0x800bdbac.
        let k = front.rand.below(6);
        let name_at = u32::from_le_bytes([byte(0x800b_dbac + 4 * k), byte(0x800b_dbad + 4 * k), byte(0x800b_dbae + 4 * k), byte(0x800b_dbaf + 4 * k)]);
        let name: String = (0..16).map(|i| byte(name_at + i)).take_while(|&c| c != 0).map(|c| (c as char).to_ascii_uppercase()).collect();
        let music_vh = big.lookup(&format!("SCREENSBIG/{name}VH")).ok_or(format!("{name}.VH is not in SCREENS.BIG"))?;
        let music = hwtr_game::snd::Bank::from_vh(music_vh, 0).ok_or("the music bank does not parse")?;
        let music_samples: std::sync::Arc<[u8]> = big.lookup(&format!("SCREENSBIG/{name}VB")).unwrap_or_default().into();
        rrt::tracing::info!("music: {name}");
        let view = exe.view();
        let word = |a: u32| u32::from_le_bytes(std::array::from_fn(|k| view.u8(a + k as u32).unwrap_or(0)));
        let car_names: Vec<String> = (0..41u32)
            .map(|k| {
                let at = word(0x800c_5ce8 + 4 * k);
                (0..16).map(|i| view.u8(at + i).unwrap_or(0)).take_while(|&b| b != 0).map(char::from).collect()
            })
            .collect();
        let car_centres: Vec<[i32; 2]> = get("CWHSBMF")
            .ok()
            .and_then(|b| hwtr_data::car::bmf_parts(b).ok())
            .map(|parts| {
                parts
                    .iter()
                    .map(|p| {
                        let at = |o: usize| p.get(o..o + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
                        [at(4 + 0x80), at(4 + 0x84)]
                    })
                    .collect()
            })
            .unwrap_or_default();
        let cars = car_names
            .iter()
            .map(|name| {
                let n = name.to_uppercase();
                let bmf = get(&format!("{n}BMF")).ok()?;
                let model = hwtr_data::car::CarBmf::parse(bmf).ok().and_then(|b| hwtr_data::car::Model::parse(b.models[0]).ok())?;
                let tim = hwtr_data::Tim::parse(get(&format!("{n}TIM")).ok()?).ok()?;
                Some((model, tim))
            })
            .collect();
        let decals = get("DECALSBMF")
            .ok()
            .and_then(|b| hwtr_data::car::bmf_parts(b).ok())
            .map(|parts| parts.iter().map(|p| hwtr_data::car::unpack_decal(p).unwrap_or_default()).collect())
            .unwrap_or_default();
        drop(big);
        Ok(FrontEnd {
            decals,
            decal_slots: [None, None],
            front,
            big: big_bytes,
            car_centres,
            cars,
            previews: [None, None],
            vram,
            font_mode: mode(FONT_AT, FONT_CLUT, depth),
            font,
            picture: None,
            picture_size: (0, 0),
            renderer: None,
            stale: true,
            card_header,
            scr,
            textures,
            tables,
            spu,
            effects,
            music,
            music_samples,
        })
    }

    /// One vertical blank of `ms`.
    pub fn tick(&mut self, pad: PadState, ms: u32) {
        let state = self.front.state();
        self.front.tick([pad, PadState::default()]);
        if self.front.state() != state {
            rrt::tracing::debug!("front state {} at {} ms", self.front.state(), self.front.clock);
        }
        self.front.clock = self.front.clock.wrapping_add(ms);
        self.play_sounds();
        for (k, dir) in CARD_DIRS.iter().enumerate() {
            if !std::mem::take(&mut self.front.save_due[k]) {
                continue;
            }
            let Some(save) = &self.front.slots[k] else { continue };
            let path = Path::new(dir).join(card::FILE_NAME);
            let written = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, card::file(&self.card_header, save)));
            match written {
                Ok(()) => rrt::tracing::info!("saved to {}", path.display()),
                Err(e) => rrt::tracing::error!("{}: {e}", path.display()),
            }
        }
    }

    /// 0x80015908: each effect asked for on voice 1 of bank 0, at full
    /// volume (a new one cuts the last).
    fn play_sounds(&mut self) {
        let Some(spu) = &self.spu else { return };
        let Ok(mut spu) = spu.lock() else { return };
        // 0x80018118: the music, one long sample on voice 0 (bank 1, program
        // 0, tone 0, note 60) at two fifths of the music volume (127);
        // 0x800181ac lets it go.
        match self.front.music {
            Some(true) => {
                let level = 127 * 2 / 5;
                if let Some(v) = hwtr_game::snd::key_on(&self.music, &self.effects.notes, 0, 0, 60, 0, level, level, false, 0) {
                    spu.key_on(0, &self.music_samples, &v);
                }
            }
            Some(false) => spu.key_off(0),
            None => {}
        }
        for &(id, volume) in &self.front.sounds {
            self.effects.mono = self.front.settings.other[1] == 0;
            if let Some(v) = self.effects.voice(id, volume as i32) {
                spu.key_on(1, &self.effects.samples, &v);
            }
        }
    }

    /// The picture `name` into VRAM, if it is not there.
    fn picture(&mut self, name: &str) {
        if self.picture.as_deref() == Some(name) {
            return;
        }
        let Ok(big) = hwtr_data::Big::parse(&self.big) else { return };
        let key = format!("SCREENSBIG/{}TIM", name.to_uppercase());
        let Some(tim) = big.lookup(&key).and_then(|b| hwtr_data::Tim::parse(b).ok()) else {
            rrt::tracing::warn!("{key} is not on the disc");
            self.picture = Some(name.to_string());
            return;
        };
        self.vram.load(PICTURE_AT.0, PICTURE_AT.1, tim.rect.w, tim.rect.h, &tim.data);
        if let Some((r, colours)) = &tim.clut {
            self.vram.load(PICTURE_CLUT.0, PICTURE_CLUT.1, r.w, 1, &colours[..r.w as usize]);
        }
        // An 8-bit picture is two pixels a VRAM word.
        self.picture_size = (tim.rect.w * 2, tim.rect.h);
        self.picture = Some(name.to_string());
        self.stale = true;
    }

    /// Player `k`'s preview model and skin for car `car`, if not already
    /// there: the skin into the player's place in VRAM. (The original
    /// streams `<car>.BMF` and `<car>.TIM` off the CD, 0x80023128; every
    /// car is in memory here.)
    fn load_preview(&mut self, k: usize, car: u8) {
        let k = k & 1;
        if self.previews[k].as_ref().is_some_and(|(c, _)| *c == car) {
            return;
        }
        let Some(Some((model, tim))) = self.cars.get(car as usize) else {
            self.previews[k] = None;
            return;
        };
        let model = model.clone();
        let (at, clut) = (PREVIEW_AT[k], PREVIEW_CLUT[k]);
        self.vram.load(at.0, at.1, tim.rect.w, tim.rect.h, &tim.data);
        if let Some((r, colours)) = &tim.clut {
            self.vram.load(clut.0, clut.1, r.w, 1, &colours[..r.w as usize]);
        }
        let centre = self.car_centres.get(car as usize).copied().unwrap_or_default();
        self.previews[k] = Some((car, crate::pieces::PreviewCar { model, mode: mode(at, clut, 1), centre }));
        self.stale = true;
    }

    /// Car `car`'s picture into VRAM slot `slot`, if not already there (the
    /// original unpacks it off `DECALS.BMF` as the car is chosen,
    /// 0x80023540).
    fn load_decal(&mut self, slot: u8, car: u8) {
        let slot = slot & 1;
        if self.decal_slots[slot as usize] == Some(car) {
            return;
        }
        let Some(pixels) = self.decals.get(car as usize).filter(|p| !p.is_empty()) else { return };
        self.vram.load(DECAL_AT.0, DECAL_AT.1 + DECAL_SIZE.1 * slot as u16, DECAL_SIZE.0, DECAL_SIZE.1, pixels);
        self.decal_slots[slot as usize] = Some(car);
        self.stale = true;
    }

    fn triangles(&self, frame: &Frame) -> Vec<Vtx> {
        let mut out = Vec::new();
        let quad = |x: f32, y: f32, w: f32, h: f32, us: [u8; 4], vs: [u8; 4], colour: u32, mode: u32| -> [Vtx; 6] {
            let corner = |k: usize, px: f32, py: f32| Vtx {
                pos: [px, py, 0.0],
                colour,
                uv: us[k] as u32 | (vs[k] as u32) << 8,
                mode,
                window: Vtx::window_of(us, vs),
            };
            let c = [corner(0, x, y), corner(1, x + w, y), corner(2, x, y + h), corner(3, x + w, y + h)];
            [c[0], c[1], c[2], c[1], c[3], c[2]]
        };
        if let Some((_, shade)) = &frame.background {
            let shade = *shade as u32;
            let colour = shade | shade << 8 | shade << 16;
            let (w, h) = self.picture_size;
            // In strips of 128 pixels, two to an 8-bit texture page.
            for k in 0..w.div_ceil(128) {
                let px = k * 128;
                let sw = (w - px).min(128);
                let page = mode((PICTURE_AT.0 + (px / 256) * 128, PICTURE_AT.1), PICTURE_CLUT, 1);
                let u0 = (px % 256) as u8;
                let u1 = u0 + (sw - 1) as u8;
                let v1 = (h.min(256) - 1) as u8;
                out.extend(quad(px as f32, 0.0, sw as f32, h as f32, [u0, u1, u0, u1], [0, 0, v1, v1], colour, page));
            }
        }
        out.extend(crate::pieces::triangles(&frame.pieces, frame.track.as_ref(), &self.scr, &self.textures, &self.tables));
        for d in &frame.cars {
            if let Some((car, model)) = &self.previews[d.player as usize & 1]
                && *car == d.car
            {
                out.extend(crate::pieces::car_triangles(d, model, &self.tables));
            }
        }
        // 0x80023660: a textured quad, the colour at 255.
        let decal_mode = mode(DECAL_AT, (0, 0), 2);
        for d in &frame.decals {
            if self.decal_slots[d.slot as usize & 1] != Some(d.car) {
                continue;
            }
            let (w, h) = DECAL_SIZE;
            let (u0, u1) = (0, (w - 1) as u8);
            let v0 = (h * (d.slot as u16 & 1)) as u8;
            let v1 = v0 + (h - 1) as u8;
            out.extend(quad(d.x as f32, d.y as f32, w as f32, h as f32, [u0, u1, u0, u1], [v0, v0, v1, v1], 0xff_ffff, decal_mode));
        }
        for g in &frame.glyphs {
            let Some(glyph) = self.font.glyphs.get(g.ch as usize) else { continue };
            let colour = g.colour[0] as u32 | (g.colour[1] as u32) << 8 | (g.colour[2] as u32) << 16;
            out.extend(quad(g.x as f32, g.y as f32, glyph.w as f32, glyph.h as f32, glyph.u, glyph.v, colour, self.font_mode));
        }
        out
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) -> wgpu::CommandBuffer {
        let frame = self.front.shown.clone();
        if let Some((name, _)) = &frame.background {
            self.picture(name);
        }
        for d in &frame.cars {
            self.load_preview(d.player as usize, d.car);
        }
        for d in &frame.decals {
            self.load_decal(d.slot, d.car);
        }
        if self.stale || self.renderer.as_ref().is_none_or(|(_, f)| *f != format) {
            let mut r = Renderer::new(device, queue, format, &self.vram, &[]);
            r.screen = SCREEN;
            self.renderer = Some((r, format));
            self.stale = false;
        }
        let tris = self.triangles(&frame);
        let (renderer, _) = self.renderer.as_mut().unwrap();
        renderer.set_overlay(device, queue, &tris);
        renderer.draw(device, queue, target, size, rrt::glam::Mat4::IDENTITY)
    }
}
