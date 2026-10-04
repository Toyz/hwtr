//! The race's pause menu: the game's second state machine (`fsm_second`,
//! 0x800c6640, 37 states) over five little screens (0x800c60d4): the menu
//! itself (Continue, Restart Race, Options, Abort Race), Options, Sound
//! Volumes, Boom Box, and "Are You Sure?". Its labels are the front end's
//! 24-byte widgets, laid out across the race's 384-pixel screen in the
//! race's text font and drawn over the stopped race: boxes blue, labels
//! white, the chosen one red.
//!
//! The Boom Box plays the CD's songs, the one playing named by artist and
//! title; changing it asks the CD (`cd`). Sound Volumes steps the effects,
//! the music and the voice-over by tenths, the effects and music shown as
//! percentages; the race applies them as they change.

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::front::font::ScreenFont;
use crate::front::screen::{Entry, Label, Letter, Screen, Style};
use crate::front::strings::Strings;
use crate::fsm::{self, Def, Fsm, Kit, Poster, Registry};
use crate::hud::{Font, Sprite};
use crate::math::fx;
use crate::pad::{Mapping, PadState};

/// The pause machine's record and its screens.
pub const FSM_SECOND: u32 = 0x800c_6640;
const SCREENS: u32 = 0x800c_60d4;
const SCREEN_COUNT: u32 = 5;
/// The race's screen width, to which the labels' strips are cut.
const WIDTH: i16 = 384;
/// At most this many steps a blank when none draws.
const STEPS_A_BLANK: usize = 64;

/// The volumes the pause menu sets, 0 to 255 (the settings' +0x876,
/// +0x877 and +0x87a).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Volumes {
    pub effects: u8,
    pub music: u8,
    pub voice: u8,
}

/// A volume's percentage, in tenths (0x8009ef94).
fn percent(v: i32) -> i32 {
    v * 100 / 255 / 10 * 10
}

/// 0x8009f388: a volume a tenth up (255 past 90%), and whether it moved.
pub fn volume_up(v: u8) -> (u8, bool) {
    let p = percent(v as i32);
    if p >= 100 {
        return (255, false);
    }
    let to = p + 10;
    if to >= 100 {
        return (255, true);
    }
    let mut a = v as i32;
    while percent(a) < to {
        a += 1;
    }
    if a < 256 { (a as u8, true) } else { (255, false) }
}

/// 0x8009f4a4: a volume a tenth down (0 under 10%), and whether it moved.
pub fn volume_down(v: u8) -> (u8, bool) {
    let p = percent(v as i32);
    if p <= 0 {
        return (0, false);
    }
    let to = p - 10;
    if to <= 0 {
        return (0, true);
    }
    let mut a = v as i32;
    loop {
        a -= 1;
        if to >= percent(a) {
            break;
        }
    }
    if a <= 0 {
        (0, false)
    } else if a < 255 {
        (a as u8, true)
    } else {
        (255, false)
    }
}

/// What the pause menu was left by (0x8009cb74's result).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseEnd {
    Continue,
    Restart,
    Abort,
}

/// What every pause shares, read once: the machine, the screens with their
/// strings, and how their letters are set.
#[derive(Clone)]
pub struct PauseKit {
    def: Rc<Def<u32>>,
    actions: Rc<Registry<Pause>>,
    screens: Vec<Screen>,
    style: RaceText,
    /// The CD's songs, artist and title.
    songs: Rc<Vec<(String, String)>>,
}

/// The pause menu's way with letters: the race's text font, its glyph
/// widths (0x8001dd4c, upper case) and the front end's kerning; every strip
/// centred and cut to the race's screen; labels come on from 384 pixels to
/// the right; letters slide either way and jump home within 10 pixels.
#[derive(Clone)]
pub struct RaceText {
    widths: Vec<u16>,
    kerning: ScreenFont,
}

impl RaceText {
    fn width(&self, c: u8) -> i32 {
        self.widths.get(c.to_ascii_uppercase() as usize).copied().unwrap_or(0) as i32
    }
}

impl Style for RaceText {
    fn advance(&self, a: u8, b: u8) -> i32 {
        self.width(a) + self.kerning.kerning(a, b) as i32
    }

    fn last(&self, c: u8) -> i32 {
        self.width(c)
    }

    fn starts_left(&self, _: Entry) -> bool {
        false
    }

    fn cut_at(&self) -> Option<i16> {
        Some(WIDTH)
    }

    /// 0x8009d040: in place, or (entry 1) from the right edge. Entry 2,
    /// from a random spot, no pause screen uses.
    fn comes_from(&self, entry: Entry, to: (i16, i16)) -> Option<(i16, i16)> {
        match entry {
            Entry::InPlace => Some(to),
            Entry::FromLeft => Some((to.0.wrapping_add(WIDTH), to.1)),
            _ => None,
        }
    }

    /// 0x8009d3e4, 0x8009d38c.
    fn slide(&self, g: &mut Letter, f: i32) {
        let near = |a: i16, b: i16| (a as i32) < b as i32 + 10 && (b as i32) < a as i32 + 10;
        if near(g.at.0, g.to.0) && near(g.at.1, g.to.1) {
            g.at = g.to;
            return;
        }
        let dx = fx((g.to.0 as i32 - g.at.0 as i32) << 12, f) >> 12;
        let dy = fx((g.to.1 as i32 - g.at.1 as i32) << 12, f) >> 12;
        g.at = (g.at.0.wrapping_add(dx as i16), g.at.1.wrapping_add(dy as i16));
    }
}

impl PauseKit {
    /// The race's way with letters, which the results' table uses too.
    pub fn style(&self) -> &RaceText {
        &self.style
    }

    /// From the executable, the string table, the race's text font
    /// (`ACTNFNT`) and the front end's kerning.
    pub fn new(byte: &dyn Fn(u32) -> u8, strings: &Strings, font: &Font, kerning: ScreenFont) -> Option<PauseKit> {
        let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let def = fsm::read_def_bytes(byte, FSM_SECOND)?;
        let screens = (0..SCREEN_COUNT)
            .map(|k| {
                let mut s = Screen::read(&word, byte, word(SCREENS + 4 * k));
                // 0x8009d918, 0x8009da2c: the strings, upper case.
                for l in s.boxes.iter_mut().chain(s.labels.iter_mut()) {
                    l.fill(strings);
                    for g in &mut l.letters {
                        g.ch = g.ch.to_ascii_uppercase();
                    }
                }
                s
            })
            .collect();
        let style = RaceText { widths: font.glyphs.iter().map(|g| g.w).collect(), kerning };
        let songs = Rc::new(crate::cd::songs(byte));
        Some(PauseKit { def: Rc::new(def), actions: Rc::new(registry()), screens, style, songs })
    }

    /// The menu's actions not ported yet.
    pub fn unported_actions(&self) -> BTreeSet<u32> {
        self.actions.missing(&self.def)
    }
}

pub struct Pause {
    kit: PauseKit,
    fsm: Fsm,
    screens: Vec<Screen>,
    /// The pad of the player who paused (0x800d2804), and both pads now.
    pub pad: usize,
    pads: [PadState; 2],
    mapping: Mapping,
    /// Actions 14 to 27 held at the last look (0x800c5e30), and Start let
    /// go since the menu came up (0x800d22b4).
    held: [[bool; 14]; 2],
    start_free: [bool; 2],
    /// The slide clock (0x800d22b0).
    moved: u32,
    clock: u32,
    restart: bool,
    abort: bool,
    frame_done: bool,
    drawing: Vec<Sprite>,
    /// What is on screen, and the sounds asked for this blank.
    pub sprites: Vec<Sprite>,
    pub sounds: Vec<u8>,
    pub unported: BTreeSet<u32>,
    /// The song playing (0x800d2484, as the CD reports it), and the CD
    /// asked for this blank.
    pub song: u8,
    pub cd: Vec<crate::cd::CdAsk>,
    /// The volumes, and whether the effects' changed this blank; left and
    /// right as last read (0x800d22b8) and their repeat counts (0x800d22c4,
    /// 0x800d22c8).
    pub volumes: Volumes,
    pub effects_changed: bool,
    effects_level: u8,
    sides: u32,
    repeat: [i32; 2],
}

impl Pause {
    /// 0x80014cd0: the menu coming up for the player on `pad`.
    pub fn new(kit: &PauseKit, pad: usize) -> Pause {
        Pause {
            fsm: Fsm::new(&kit.def),
            screens: kit.screens.clone(),
            kit: kit.clone(),
            pad,
            pads: [PadState::default(); 2],
            mapping: Mapping::default(),
            held: [[false; 14]; 2],
            start_free: [false; 2],
            moved: 0,
            clock: 0,
            restart: false,
            abort: false,
            frame_done: false,
            drawing: Vec::new(),
            sprites: Vec::new(),
            sounds: Vec::new(),
            unported: BTreeSet::new(),
            song: 0,
            cd: Vec::new(),
            volumes: Volumes::default(),
            effects_changed: false,
            effects_level: 0,
            sides: 0,
            repeat: [0; 2],
        }
    }

    /// One blank at `clock` (0x8009cb74, stepped until a frame is drawn as
    /// the main loop does): the menu's end once its machine is through.
    pub fn tick(&mut self, pads: [PadState; 2], clock: u32) -> Option<PauseEnd> {
        self.pads = pads;
        self.clock = clock;
        self.sounds.clear();
        self.effects_changed = false;
        self.frame_done = false;
        if !self.run_blank(STEPS_A_BLANK) {
            return None;
        }
        Some(if self.abort {
            PauseEnd::Abort
        } else if self.restart {
            PauseEnd::Restart
        } else {
            PauseEnd::Continue
        })
    }

    /// 0x8009d2b8: the boxes and labels laid out and sent to where they
    /// come on from.
    fn enter(&mut self, k: usize) {
        let style = &self.kit.style;
        let screen = &mut self.screens[k];
        for (l, label) in screen.boxes.iter_mut().map(|b| (b, false)).chain(screen.labels.iter_mut().map(|l| (l, true)))
        {
            l.lay_out(style, label);
            l.start_entry(style);
        }
    }

    /// 0x8009d6ec: a frame's slide, by the same fraction as the front end's
    /// (its own clock); a letter within 10 pixels each way jumps home.
    fn slide(&mut self, k: usize) {
        let dt = self.clock.wrapping_sub(self.moved);
        let f = if self.moved == 0 {
            0xf000 * 4096 / 0x6_4000
        } else if dt < 101 {
            ((dt as i32) << 12) / 100
        } else {
            4096 / 10
        };
        self.moved = self.clock;
        let style = &self.kit.style;
        let screen = &mut self.screens[k];
        for l in screen.boxes.iter_mut().chain(screen.labels.iter_mut()) {
            l.slide(f, style);
        }
    }

    /// 0x8009dcd8, 0x8009dfbc, 0x8009ddb4: screen `k` with line `chosen` in
    /// red.
    fn draw(&mut self, k: usize, chosen: usize) {
        self.drawing.clear();
        let screen = &self.screens[k];
        for b in &screen.boxes {
            self.drawing.extend(glyphs(b, [0, 0, 127]));
        }
        for (i, l) in screen.labels.iter().enumerate() {
            let colour = if i == chosen { [127, 0, 0] } else { [127, 127, 127] };
            self.drawing.extend(glyphs(l, colour));
        }
        if k == 3 {
            // 0x8009f98c: the song's artist and title, centred, white.
            if let Some((artist, title)) = self.kit.songs.get(self.song as usize).cloned() {
                self.text(&artist, 192, 155);
                self.text(&title, 192, 175);
            }
        }
        self.sprites = std::mem::take(&mut self.drawing);
        self.frame_done = true;
    }

    /// 0x8009f1d0, 0x8009f2f8, 0x8009f260: Sound Volumes, the effects and
    /// the music as percentages, the one chosen (0 effects, 1 music, none
    /// on the voice-over) in red.
    fn draw_volumes(&mut self, chosen_label: usize, red: Option<usize>) {
        self.draw(2, chosen_label);
        let v = self.volumes;
        for (i, (value, y)) in [(v.effects, 105), (v.music, 130)].into_iter().enumerate() {
            let colour = if red == Some(i) { [127, 0, 0] } else { [127, 127, 127] };
            let text = format!("{}%", percent(value as i32));
            self.text_in(&text, 240, y, colour);
            self.sprites.append(&mut self.drawing);
        }
    }

    /// 0x8009f0e8: left and right on a volume: a press steps it at once
    /// (events 11 down, 10 up), held it steps again after eleven blanks,
    /// then every four.
    fn volume_repeat(&mut self, p: &mut Poster) {
        let pad = if self.pad == 0 { 0 } else { 1 };
        // 0x8009ea28: left held (1), right held (8), and each changed (2, 16).
        let mut s = self.pads[pad].holds(&self.mapping, 16) as u32;
        if self.pads[pad].holds(&self.mapping, 17) {
            s |= 8;
        }
        if s & 1 != self.sides & 1 {
            s |= 2;
        }
        if s & 8 != self.sides & 8 {
            s |= 16;
        }
        self.sides = s;
        let mut step = |k: usize, changed: bool, event: i16, p: &mut Poster| {
            if changed {
                p.post(event);
                self.repeat[k] = -8;
            } else {
                let was = self.repeat[k];
                self.repeat[k] += 1;
                if was >= 3 {
                    p.post(event);
                    self.repeat[k] = 0;
                }
            }
        };
        match s & 9 {
            1 => step(0, s & 2 != 0, 11, p),
            8 => step(1, s & 16 != 0, 10, p),
            _ => {}
        }
    }

    /// A volume stepped (0x8009f5b8 and its neighbours), with a click when
    /// it moved: the effects', the voice-over's, or the music's (which, as
    /// in the original, sets the effects to the music's level too).
    fn volume_step(&mut self, which: usize, up: bool) {
        let v = &mut self.volumes;
        let value = match which {
            0 => &mut v.effects,
            1 => &mut v.voice,
            _ => &mut v.music,
        };
        if up && *value == 255 {
            return;
        }
        if !up && *value == 0 {
            return;
        }
        let (now, moved) = if up { volume_up(*value) } else { volume_down(*value) };
        *value = now;
        match which {
            0 => {
                self.effects_changed = true;
                self.effects_level = now;
            }
            2 => {
                self.effects_changed = true;
                self.effects_level = now;
                self.cd.push(crate::cd::CdAsk::Volume(now));
            }
            _ => {}
        }
        if moved {
            self.sounds.push(49);
        }
    }

    /// The volumes to start from, the effects' as the race has them.
    pub fn set_volumes(&mut self, volumes: Volumes, effects_level: u8) {
        self.volumes = volumes;
        self.effects_level = effects_level;
    }

    /// The effects' level as the race uses it: the music's after a music
    /// change (0x8001a6dc was given it last).
    pub fn effects_level(&self) -> u8 {
        self.effects_level
    }

    /// 0x8009e160: `text` in the race's font centred on `x` at `y`, each
    /// letter by its width and the kerning to the next (0x8009e0a0).
    fn text(&mut self, text: &str, x: i16, y: i16) {
        self.text_in(text, x, y, [255; 3]);
    }

    fn text_in(&mut self, text: &str, x: i16, y: i16, colour: [u8; 3]) {
        let style = &self.kit.style;
        let b = text.as_bytes();
        let width: i32 = (0..b.len())
            .map(|i| style.width(b[i]) + b.get(i + 1).map_or(0, |&n| style.kerning.kerning(b[i], n) as i32))
            .sum();
        let mut at = x as i32 - width / 2;
        for (i, &c) in b.iter().enumerate() {
            // The glyph upper case (0x8001ddc4), the kerning on the letters
            // as they are.
            self.drawing.push(Sprite { font: 0, glyph: c.to_ascii_uppercase(), x: at as i16, y, colour, layer: 0 });
            at += style.width(c) + b.get(i + 1).map_or(0, |&n| style.kerning.kerning(c, n) as i32);
        }
    }

    /// 0x8009e2c8: whether `pad` has just pressed `action`; Start (26) is
    /// read as held.
    fn pressed(&mut self, pad: usize, action: u8) -> bool {
        let holds = self.pads[pad].holds(&self.mapping, action);
        if action == 26 {
            return holds;
        }
        let k = (action - 14) as usize;
        if holds {
            !std::mem::replace(&mut self.held[pad][k], true)
        } else {
            self.held[pad][k] = false;
            false
        }
    }

    /// 0x8009e39c: the pausing player's pad. Each press that the state takes
    /// becomes its event, with its sound; Start, once let go and pressed
    /// again, resumes.
    fn pads_read(&mut self, p: &mut Poster) {
        let pad = if self.pad == 0 { 0 } else { 1 };
        // (action, event, sound)
        for (action, event, sound) in [(14, 4, 32), (15, 5, 31), (18, 3, 33), (19, 9, 34), (17, 1, 49), (16, 2, 48)] {
            if self.pressed(pad, action) && p.takes(event) {
                self.sounds.push(sound);
                p.post(event);
            }
        }
        if self.pressed(pad, 26) {
            if self.start_free[pad] {
                p.post(6);
            }
        } else {
            self.start_free[pad] = true;
        }
    }
}

/// Every ported action of the pause menu.
fn registry() -> Registry<Pause> {
    let mut r = Registry::default();
    // the screens' strings: read with the kit
    r.add(0x8009_ec10, |_: &mut Pause, _: &mut Poster| {});
    r.add(0x8009_ec80, |_: &mut Pause, _: &mut Poster| {});
    r.add(0x8009_ecec, |_: &mut Pause, _: &mut Poster| {});
    r.add(0x8009_ec88, |_: &mut Pause, _: &mut Poster| {});
    r.add(0x8009_ecf4, |k: &mut Pause, _: &mut Poster| k.start_free = [false; 2]);
    r.add(0x8009_eb28, |_: &mut Pause, p: &mut Poster| p.post(0));
    // Each screen coming on.
    r.add(0x8009_ed04, |k: &mut Pause, _: &mut Poster| k.enter(0));
    r.add(0x8009_ee84, |k: &mut Pause, _: &mut Poster| k.enter(1));
    r.add(0x8009_ef44, |k: &mut Pause, _: &mut Poster| k.enter(2));
    // The Boom Box: on with the song playing (0x8009f910), the next and
    // the one before (0x8009fb24, 0x8009fba0).
    r.add(0x8009_f910, |k: &mut Pause, _: &mut Poster| {
        k.enter(3);
        k.cd.push(crate::cd::CdAsk::Play(k.song));
    });
    r.add(0x8009_fb24, |k: &mut Pause, _: &mut Poster| {
        k.song = (k.song + 1) % crate::cd::SONGS;
        k.cd.push(crate::cd::CdAsk::Stop);
        k.cd.push(crate::cd::CdAsk::Play(k.song));
    });
    r.add(0x8009_fba0, |k: &mut Pause, _: &mut Poster| {
        k.song = if k.song == 0 { crate::cd::SONGS - 1 } else { k.song - 1 };
        k.cd.push(crate::cd::CdAsk::Stop);
        k.cd.push(crate::cd::CdAsk::Play(k.song));
    });
    r.add(0x8009_ebe8, |k: &mut Pause, _: &mut Poster| k.enter(4));
    // Each screen sliding.
    r.add(0x8009_ed2c, |k: &mut Pause, _: &mut Poster| k.slide(0));
    r.add(0x8009_eeac, |k: &mut Pause, _: &mut Poster| k.slide(1));
    r.add(0x8009_ef6c, |k: &mut Pause, _: &mut Poster| k.slide(2));
    r.add(0x8009_f964, |k: &mut Pause, _: &mut Poster| k.slide(3));
    r.add(0x8009_ebc0, |k: &mut Pause, _: &mut Poster| k.slide(4));
    // Each screen drawn with a line chosen.
    r.add(0x8009_eda4, |k: &mut Pause, _: &mut Poster| k.draw(0, 0));
    r.add(0x8009_eddc, |k: &mut Pause, _: &mut Poster| k.draw(0, 1));
    r.add(0x8009_ee14, |k: &mut Pause, _: &mut Poster| k.draw(0, 2));
    r.add(0x8009_ee4c, |k: &mut Pause, _: &mut Poster| k.draw(0, 3));
    r.add(0x8009_eed4, |k: &mut Pause, _: &mut Poster| k.draw(1, 0));
    r.add(0x8009_ef0c, |k: &mut Pause, _: &mut Poster| k.draw(1, 1));
    r.add(0x8009_f1d0, |k: &mut Pause, _: &mut Poster| k.draw_volumes(0, Some(0)));
    r.add(0x8009_f2f8, |k: &mut Pause, _: &mut Poster| k.draw_volumes(1, Some(1)));
    r.add(0x8009_f260, |k: &mut Pause, _: &mut Poster| k.draw_volumes(1, None));
    r.add(0x8009_f0e8, |k: &mut Pause, p: &mut Poster| k.volume_repeat(p));
    r.add(0x8009_f5b8, |k: &mut Pause, _: &mut Poster| k.volume_step(0, true));
    r.add(0x8009_f63c, |k: &mut Pause, _: &mut Poster| k.volume_step(0, false));
    r.add(0x8009_f6c0, |k: &mut Pause, _: &mut Poster| k.volume_step(1, true));
    r.add(0x8009_f744, |k: &mut Pause, _: &mut Poster| k.volume_step(1, false));
    r.add(0x8009_f7c8, |k: &mut Pause, _: &mut Poster| k.volume_step(2, true));
    r.add(0x8009_f86c, |k: &mut Pause, _: &mut Poster| k.volume_step(2, false));
    r.add(0x8009_f98c, |k: &mut Pause, _: &mut Poster| k.draw(3, 0));
    r.add(0x8009_fa44, |k: &mut Pause, _: &mut Poster| k.draw(3, 1));
    r.add(0x8009_eb88, |k: &mut Pause, _: &mut Poster| k.draw(4, 0));
    r.add(0x8009_eb50, |k: &mut Pause, _: &mut Poster| k.draw(4, 1));
    r.add(0x8009_e39c, |k: &mut Pause, p: &mut Poster| k.pads_read(p));
    // Continue: the pad is there (0x8001bca4).
    r.add(0x8009_ed54, |_: &mut Pause, p: &mut Poster| p.post(7));
    r.add(0x8009_eb08, |k: &mut Pause, _: &mut Poster| k.restart = true);
    r.add(0x8009_eae8, |k: &mut Pause, _: &mut Poster| k.abort = true);
    r.add(0x8009_fafc, |k: &mut Pause, _: &mut Poster| k.cd.push(crate::cd::CdAsk::Stop));
    r
}

/// The letters as the race's text font draws them (0x8001e018, font 0).
fn glyphs(l: &Label, colour: [u8; 3]) -> impl Iterator<Item = Sprite> + '_ {
    l.letters.iter().map(move |g| Sprite { font: 0, glyph: g.ch, x: g.at.0, y: g.at.1, colour, layer: 0 })
}

impl Kit for Pause {
    fn def(&self) -> Rc<Def<u32>> {
        Rc::clone(&self.kit.def)
    }

    fn machine(&mut self) -> &mut Fsm {
        &mut self.fsm
    }

    fn actions(&self) -> Rc<Registry<Pause>> {
        Rc::clone(&self.kit.actions)
    }

    fn unported(&mut self, addr: u32) {
        if self.unported.insert(addr) {
            tracing::warn!("pause menu: action {addr:#010x} (state {}) is not ported", self.fsm.current);
        }
    }

    fn blank_over(&self) -> bool {
        self.frame_done
    }
}
