//! The race's HUD (0x80064b8c and the elements 0x800638ec draws): the
//! speed, the lap's time, the lap and the place, the turbo meter, drawn as
//! glyphs of the race's overlay fonts on the 384 by 240 screen.

use crate::car::Car;
use crate::math::{div_fx, fx};
use crate::race::RaceSetup;

/// A glyph of an overlay font: its size on screen, and the texture
/// coordinates of its corners (top left, top right, bottom left, bottom
/// right; the fonts' textures are stored turned a quarter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    pub w: u16,
    pub h: u16,
    pub u: [u8; 4],
    pub v: [u8; 4],
}

/// An overlay font (`<name>.ovl`, loaded by 0x8001db18): its glyphs, and the
/// TIM of its texture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Font {
    pub glyphs: Vec<Glyph>,
    pub tim: Vec<u8>,
}

impl Font {
    /// Parses an overlay: the glyph count, where the glyphs are (12 bytes
    /// each), the TIM's length and where it is.
    pub fn parse(b: &[u8]) -> Option<Font> {
        let word = |at: usize| Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize);
        let (count, glyphs_at, tim_len, tim_at) = (word(0)?, word(4)?, word(8)?, word(12)?);
        let glyphs = (0..count)
            .map(|k| {
                let g = b.get(glyphs_at + 12 * k..glyphs_at + 12 * k + 12)?;
                Some(Glyph {
                    w: u16::from_le_bytes([g[0], g[1]]),
                    h: u16::from_le_bytes([g[2], g[3]]),
                    u: [g[4], g[5], g[6], g[7]],
                    v: [g[8], g[9], g[10], g[11]],
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let tim = b.get(tim_at..(tim_at + tim_len).min(b.len()))?.to_vec();
        Some(Font { glyphs, tim })
    }
}

/// The race's fonts (0x80061e04): the text font, the HUD's panels and
/// digits, and the turbo meter (one player's or two players').
pub const FONTS: [&str; 3] = ["ACTNFNT", "ACTNOVL1", "ACTNOVL2"];
pub const FONTS_TWO_PLAYERS: [&str; 3] = ["ACTNFNT", "ACTNOVL1", "ACTNOVL3"];

/// Where each font's texture and palette go in VRAM (0x800bdcc8 to
/// 0x800bdd08).
pub const FONT_IMAGE: [(u16, u16); 3] = [(960, 0), (704, 256), (768, 256)];
pub const FONT_CLUT: [(u16, u16); 3] = [(384, 470), (384, 471), (384, 472)];

/// One glyph drawn: font `font`'s glyph `glyph` with its top left at
/// (`x`, `y`), its texels times `colour` (128 is 1), in ordering-table
/// layer `layer` (the panels 2, the text over them 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub font: u8,
    pub glyph: u8,
    pub x: i16,
    pub y: i16,
    pub colour: [u8; 3],
    pub layer: u8,
}

/// Sprites in the order the PlayStation draws them: the deeper layer
/// first, and within a layer the last added first (each is put at the head
/// of its ordering-table entry).
pub fn drawing_order(sprites: &[Sprite]) -> Vec<Sprite> {
    let mut out: Vec<Sprite> = sprites.iter().rev().copied().collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.layer));
    out
}

/// What a player's HUD shows (0x800d0e58).
pub mod show {
    pub const SPEED: u16 = 1;
    pub const LAP_TIME: u16 = 2;
    pub const LAP: u16 = 4;
    pub const PLACE: u16 = 8;
    pub const POWER_UP: u16 = 16;
    pub const TURBOS: u16 = 32;
    pub const SCORE: u16 = 512;
    pub const MESSAGES: u16 = 1024;
}

/// A finished lap's time, shown for five seconds (0x80064cf0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LapFlash {
    time: u32,
    best: bool,
    from: u32,
}

/// The turbo meter's last award (0x800becb0): when, the turbos before, how
/// many it added, the step of their arrival (none when `None`), and whether
/// the full meter's sound has played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Meter {
    from: u32,
    before: u8,
    added: u8,
    arriving: Option<i8>,
    flashing: bool,
    sounded: bool,
}

/// The turbo bars' colours (0x800becdc), and their heights over the meter
/// by player count (0x800becbe, all 0 in the game).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeterTables {
    pub colours: [[u8; 3]; 10],
    pub rise: [[u8; 10]; 3],
}

impl MeterTables {
    pub fn read(byte: &impl Fn(u32) -> u8) -> MeterTables {
        MeterTables {
            colours: std::array::from_fn(|k| std::array::from_fn(|c| byte(0x800b_ecdc + 3 * k as u32 + c as u32))),
            rise: std::array::from_fn(|p| std::array::from_fn(|k| byte(0x800b_ecbe + 10 * p as u32 + k as u32))),
        }
    }
}

/// The countdown's number on screen: from the effects sheet (SFX.GLM), a
/// 64-texel square drawn `size` pixels across, centred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountSprite {
    /// 3, 2, 1, or 0 for GO.
    pub number: u8,
    pub x: i16,
    pub y: i16,
    pub size: i16,
    /// Its texels' top left in the page.
    pub u: u8,
    pub v: u8,
}

/// Each number's texels in the effects sheet (0x800bdd08), GO first.
pub const COUNT_UV: [(u8, u8); 4] = [(192, 96), (96, 64), (160, 64), (128, 96)];
/// The effects sheet's sprite slot a number uses (slots 20 to 23).
pub const COUNT_SLOT: usize = 20;

/// The countdown's numbers (0x800d2510 to 0x800d2512, 0x800d250c): the one
/// called, the one last finished shrinking, the end after GO, and the size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Countdown {
    called: Option<u8>,
    shown: Option<u8>,
    over: bool,
    size: u16,
}

impl Default for Countdown {
    /// 0x8001dab0.
    fn default() -> Countdown {
        Countdown { called: None, shown: None, over: false, size: 140 }
    }
}

impl Countdown {
    /// 0x8001e054: the countdown calls `number` (0 for GO).
    pub fn call(&mut self, number: u8) {
        self.called = Some(number);
    }

    /// 0x8001e060, once a game frame: the number called shrinks from 140
    /// pixels by 8 a frame, centred on the screen (on the upper half's
    /// height for two players); once gone, it is done, and after GO the
    /// countdown is over.
    pub fn frame(&mut self, players: u8) -> Option<CountSprite> {
        let number = self.called?;
        if self.called == self.shown || self.over {
            return None;
        }
        self.size = self.size.wrapping_sub(8);
        if self.size >= 141 {
            self.shown = self.called;
            if number == 0 {
                self.over = true;
            }
            self.size = 140;
            return None;
        }
        let size = self.size as i16;
        let (w, h) = (384i16, 240i16);
        let tall = if players >= 2 { h / 2 - 1 } else { h };
        let (u, v) = COUNT_UV[number as usize % 4];
        Some(CountSprite { number, x: w / 2 - size / 2, y: tall / 2 - size / 2, size, u, v })
    }
}

/// The HUD for the race's players.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hud {
    /// The players (1 or 2; 0 for a race without one), the cars and the
    /// laps it counts against.
    pub players: u8,
    pub cars: u8,
    pub laps: u8,
    /// A race against the clock: its limit, ms.
    pub limit: Option<u32>,
    /// What each player's HUD shows.
    pub show: [u16; 2],
    pub meter: MeterTables,
    flashes: [Option<LapFlash>; 2],
    meters: [Meter; 2],
    /// Sounds the HUD asks for.
    pub sounds: Vec<u16>,
    pub countdown: Countdown,
    /// When each player's car was last seen going the wrong way, while the
    /// warning shows (0x800bead0 +0 and +236).
    wrong_way_from: [Option<u32>; 2],
}

/// The sound of a full turbo meter.
pub const SOUND_FULL: u16 = 60;

impl Hud {
    /// 0x80061e04: the HUD for `setup`. One player sees the speed, the
    /// power-up and the messages; a scoring race adds the score, other races
    /// the lap's time, and those with laps the lap and the place; all show
    /// the turbo meter.
    pub fn new(setup: &RaceSetup, meter: MeterTables) -> Hud {
        let driver = |k: usize| setup.cars.get(k).map(|e| e.driver.byte());
        let (players, mut show) = if setup.cars.len() == 2 && driver(0) == Some(1) && driver(1) == Some(1) {
            (2, 0xd91)
        } else if driver(0) == Some(1) {
            (1, 0xd11)
        } else {
            (0, 0)
        };
        let mut laps = 0;
        if setup.flags & 2 != 0 {
            show |= 0x220;
        } else {
            show |= 0x62;
            if setup.flags & 8 == 0 {
                laps = setup.laps;
                show |= 12;
            }
        }
        let limit = (setup.flags & 4 != 0).then_some(setup.time_limit);
        Hud {
            players,
            cars: setup.cars.len() as u8,
            laps,
            limit,
            show: [show; 2],
            meter,
            flashes: [None; 2],
            meters: [Meter::default(); 2],
            sounds: Vec::new(),
            countdown: Countdown::default(),
            wrong_way_from: [None; 2],
        }
    }

    /// 0x80064564: player `player`'s "WRONG WAY" (`text`, string 216) in
    /// red at (94, 18) (at y 124 for the second of two players), from the
    /// race clock `now` its car is going the wrong way until 200 ms after,
    /// hidden every other half second.
    pub fn wrong_way(
        &mut self,
        player: usize,
        car: &Car,
        now: u32,
        style: &impl crate::front::screen::Style,
        text: &str,
    ) -> Vec<Sprite> {
        let mut out = Vec::new();
        let Some(from) = self.wrong_way_from.get_mut(player) else { return out };
        if car.wrong_way {
            *from = Some(now);
        }
        let Some(since) = *from else { return out };
        if now.wrapping_sub(since) > 200 {
            *from = None;
            return out;
        }
        if ((now as u64 * 0x1062_4dd3) >> 37) & 1 == 1 {
            return out;
        }
        let y = if self.players == 2 && player != 0 { 124 } else { 18 };
        race_text(style, text, 94, y, half([255, 0, 0]), &mut out);
        out
    }

    /// 0x80064cf0: player `player` finished a lap in `time`, at `now`.
    pub fn lap_done(&mut self, player: usize, time: u32, best: bool, now: u32) {
        if let Some(f) = self.flashes.get_mut(player) {
            *f = Some(LapFlash { time, best, from: now });
        }
    }

    /// The meter's part of 0x80064dec (and 0x80065398): player `player`
    /// had `before` turbos and was given `added`, at `now`.
    pub fn turbos_given(&mut self, player: usize, before: u8, added: u8, now: u32) {
        if player >= self.players as usize {
            return;
        }
        if let Some(m) = self.meters.get_mut(player) {
            let added = if added as u32 + before as u32 >= 11 { 10u8.wrapping_sub(before) } else { added };
            *m = Meter { from: now, before, added, arriving: Some(9), flashing: true, sounded: false };
        }
    }

    /// 0x800638ec: player `player`'s HUD, following `car`, at race time
    /// `now`.
    pub fn draw(&mut self, player: usize, car: &Car, now: u32) -> Vec<Sprite> {
        let mut out = Vec::new();
        let show = self.show[player];
        let split = self.players == 2;
        // A y on the screen, or on the lower player's half.
        let at = |one: i16, upper: i16, lower: i16| {
            if !split {
                one
            } else if player != 0 {
                lower
            } else {
                upper
            }
        };
        let mut d = Draw { out: &mut out, colour: WHITE };
        if show & show::SPEED != 0 {
            d.colour = WHITE;
            d.glyph(1, 2, 4, at(21, 21, 127));
            d.colour = half([0, 255, 255]);
            let shown = if car.body.asleep { 0 } else { speed(car.body.speed) };
            let text = digits(&shown.to_string());
            let x = 40 - ((text.len() as i16 * 12) | 1) / 2;
            d.text(&text, x, at(18, 18, 124), |_| 12);
        }
        if show & show::POWER_UP != 0 && car.power_up != 0 {
            // 0x80062534: the power-up's icon, white.
            d.colour = WHITE;
            let y = if !split {
                178
            } else if player == 0 {
                70
            } else {
                178
            };
            d.glyph(1, car.power_up.wrapping_add(2), 314, y);
        }
        let mut flashing = false;
        if show & show::LAP_TIME != 0 {
            match self.limit {
                Some(limit) => self.time_left(&mut d, car, limit, now, at(21, 21, 127), at(18, 18, 124)),
                None => {
                    d.colour = WHITE;
                    d.glyph(1, 3, 259, at(21, 21, 127));
                    d.colour = half([0, 255, 0]);
                    let from = car.laps.done.checked_sub(1).and_then(|k| car.laps.ends.get(k as usize));
                    let text = digits(&clock(now.wrapping_sub(from.copied().unwrap_or(0))));
                    d.text(&text, 263, at(18, 18, 124), narrow(7));
                }
            }
            flashing = self.lap_flash(&mut d, player, now, at(35, 35, 141), at(32, 32, 138));
        }
        if show & show::SCORE != 0 {
            self.time_left(&mut d, car, self.limit.unwrap_or(0), now, at(21, 21, 127), at(18, 18, 124));
            d.colour = WHITE;
            d.glyph(1, 3, 259, at(35, 35, 141));
            d.colour = half([255, 0, 0]);
            // Right-aligned from x 342.
            let text = digits(&car.stunt_points.to_string());
            let mut x = 342;
            for &c in text.iter().rev() {
                d.over(1, c, x, at(32, 32, 138));
                x -= if is_digit(c) { 13 } else { 7 };
            }
        }
        if show & show::PLACE != 0 && car.flags & 0x4000 == 0 {
            d.colour = WHITE;
            d.glyph(1, 2, 315, at(215, 107, 215));
            d.colour = half([if car.laps.place != 0 { 0 } else { 255 }, 255, 0]);
            let text = digits(&format!("{}/{}", car.laps.place + 1, self.cars));
            d.text(&text, 319, at(212, 104, 212), narrow(9));
        }
        if show & show::LAP != 0 && !flashing {
            d.colour = WHITE;
            d.glyph(1, 0, 276, at(35, 35, 141));
            d.colour = half([255, 0, 0]);
            d.over(1, 27, 284, at(32, 32, 138));
            let text = digits(&format!("{}/{}", car.laps.done.wrapping_add(1), self.laps));
            d.text(&text, 320, at(32, 32, 138), narrow(9));
        }
        if show & show::TURBOS != 0 {
            self.turbo_meter(&mut d, player, car, now, at(159, 67, 175));
        }
        out
    }

    /// 0x800627a8: the time left of a race against the clock.
    fn time_left(&self, d: &mut Draw, car: &Car, limit: u32, now: u32, panel: i16, text: i16) {
        d.colour = WHITE;
        d.glyph(1, 3, 259, panel);
        d.colour = half([0, 255, 0]);
        let left = (limit as i32).wrapping_sub(now.wrapping_sub(car.laps.start) as i32).max(0) as u32;
        d.text(&digits(&clock(left)), 263, text, narrow(7));
    }

    /// 0x80062c58: a finished lap's time for five seconds, blinking each
    /// half second, yellow for the car's best; true while it lasts.
    fn lap_flash(&mut self, d: &mut Draw, player: usize, now: u32, panel: i16, text: i16) -> bool {
        let Some(flash) = self.flashes[player] else { return false };
        let since = now.wrapping_sub(flash.from);
        if since > 5000 {
            self.flashes[player] = None;
            return false;
        }
        d.colour = WHITE;
        d.glyph(1, 3, 259, panel);
        if (since / 500) & 1 == 0 {
            d.colour = half(if flash.best { [255, 255, 0] } else { [255, 0, 0] });
            d.text(&digits(&clock(flash.time)), 263, text, narrow(7));
        }
        true
    }

    /// 0x800632d8: the turbo meter: a bar for each turbo (those just won
    /// arriving one by one), the frame, and the label, which blinks when
    /// the meter is full or has just been added to.
    fn turbo_meter(&mut self, d: &mut Draw, player: usize, car: &Car, now: u32, base: i16) {
        let rise = self.rise_row();
        let count = car.turbos;
        let m = &mut self.meters[player];
        let (mut shown, mut step, mut added) = (count, 0i8, 0u8);
        if let Some(arriving) = m.arriving
            && m.before as u32 + m.added as u32 >= count as u32
        {
            let gained = count as i32 - m.before as i32;
            let mut ended = m.added == 0 || gained <= 0;
            if !ended {
                if (count as u32) < m.before as u32 + m.added as u32 {
                    m.added = count.wrapping_sub(m.before);
                }
                let s = arriving.wrapping_sub(((now.wrapping_sub(m.from) / 100) % 10) as i8);
                if (m.before as i32) < s as i32 {
                    (shown, step, added) = (m.before, s, m.added);
                } else {
                    ended = true;
                }
            }
            if ended {
                m.arriving = None;
            }
        }
        let mut y = base;
        for k in 0..shown.min(10) {
            y = rise.get(k as usize).map_or(0, |&r| r as i16) + base;
            d.colour = half(self.meter.colours[k as usize]);
            d.glyph(2, k + 3, 34, y);
        }
        for k in 0..added {
            let at = k as i32 + step as i32;
            if at >= 10 {
                break;
            }
            y = rise.get(at as usize).map_or(0, |&r| r as i16) + base;
            d.colour = half(self.meter.colours[at as usize]);
            d.glyph(2, (at + 3) as u8, 34, y);
        }
        d.colour = WHITE;
        d.glyph(2, 0, 34, y);
        let since = now.wrapping_sub(m.from);
        let blinking = count == 10 || (m.flashing && since < 1000);
        if blinking && (since / 250) & 1 == 0 {
            if count == 10 && !m.sounded {
                self.sounds.push(SOUND_FULL);
            }
            d.over(2, 2, 26, base);
            m.sounded = true;
        } else {
            d.over(2, 1, 26, base);
            m.sounded = false;
        }
    }

    fn rise_row(&self) -> [u8; 10] {
        self.meter.rise.get(self.players as usize).copied().unwrap_or([0; 10])
    }
}

/// The HUD's colour for white: the game halves what it is given (0x8001dc6c).
const WHITE: [u8; 3] = [127, 127, 127];

fn half(c: [u8; 3]) -> [u8; 3] {
    c.map(|v| v >> 1)
}

struct Draw<'a> {
    out: &'a mut Vec<Sprite>,
    colour: [u8; 3],
}

impl Draw<'_> {
    /// A panel's glyph (layer 2).
    fn glyph(&mut self, font: u8, glyph: u8, x: i16, y: i16) {
        self.out.push(Sprite { font, glyph, x, y, colour: self.colour, layer: 2 });
    }

    /// A glyph over the panels (layer 1).
    fn over(&mut self, font: u8, glyph: u8, x: i16, y: i16) {
        self.out.push(Sprite { font, glyph, x, y, colour: self.colour, layer: 1 });
    }

    /// Glyphs left to right from `x`, each advancing by `step` of it.
    fn text(&mut self, glyphs: &[u8], mut x: i16, y: i16, step: impl Fn(u8) -> i16) {
        for &g in glyphs {
            self.over(1, g, x, y);
            x += step(g);
        }
    }
}

/// The advance of the HUD's digits (13) and of its other glyphs.
fn narrow(other: i16) -> impl Fn(u8) -> i16 {
    move |g| if is_digit(g) { 13 } else { other }
}

fn is_digit(g: u8) -> bool {
    (14..24).contains(&g)
}

/// 0x80062100: text into the HUD font's glyphs: the digits are 14 to 23,
/// then ':', '.', '/' and the space.
pub fn digits(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|c| match c {
            b'0'..=b'9' => c - b'0' + 14,
            b':' => 24,
            b'.' => 25,
            b'/' => 26,
            b' ' => 28,
            c => c,
        })
        .collect()
}

/// A time as the HUD writes it: minutes, seconds and hundredths.
pub fn clock(ms: u32) -> String {
    let rest = ms % 60_000;
    let hundredths = rest % 1000;
    format!("{:02}:{:02}.{}{}", ms / 60_000, rest / 1000, hundredths / 100, (hundredths % 100) / 10)
}

/// 0x80062208: the speedometer's reading of `speed` (inches a second, 4.12):
/// miles an hour, a hundred at 80 and slower to rise past it, at most 180.
pub fn speed(speed: i32) -> i32 {
    let v = fx(speed, 232);
    let r = if v <= 0x5_0000 {
        div_fx(fx(v, 0x6_4000), 0x5_0000)
    } else {
        div_fx(fx(v - 0x5_0000, 0x5_0000), 0x3_2000).wrapping_add(0x6_4000)
    };
    (r >> 12).min(180)
}

/// The results' words: the column heads (strings 289 to 291: TIME, BEST,
/// POINTS), what a car with no time shows (215, DNF), and the cars' names
/// by car number (0x800c5d8c).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultsText {
    pub time: String,
    pub best: String,
    pub points: String,
    pub no_time: String,
    pub cars: Vec<String>,
    /// "DEMO MODE" (string 214), over the attract race, and "WRONG WAY"
    /// (216).
    pub demo: String,
    pub wrong_way: String,
}

/// 0x80063bc4: `s` in the race's text font from (`x`, `y`), left to
/// right, each letter's glyph upper case and its width plus the kerning to
/// the next, in layer 1.
pub fn race_text(
    style: &impl crate::front::screen::Style,
    s: &str,
    x: i16,
    y: i16,
    colour: [u8; 3],
    out: &mut Vec<Sprite>,
) {
    let b = s.as_bytes();
    let mut at = x as i32;
    for (i, &c) in b.iter().enumerate() {
        out.push(Sprite { font: 0, glyph: c.to_ascii_uppercase(), x: at as i16, y, colour, layer: 1 });
        at += style.advance(c, b.get(i + 1).copied().unwrap_or(0));
    }
}

/// 0x800644c0: with no players, "DEMO MODE" in red at (130, 20), shown
/// and hidden in turn every 750 ms of the race clock `time`.
pub fn demo_mode(style: &impl crate::front::screen::Style, text: &str, time: u32) -> Vec<Sprite> {
    let mut out = Vec::new();
    if ((time as u64 * 0x0576_19f1) >> 36) & 1 == 0 {
        race_text(style, text, 130, 20, half([255, 0, 0]), &mut out);
    }
    out
}

/// One line of the results: a car's name, whether a player drives it (its
/// name green), and its time or points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultLine {
    pub name: String,
    pub player: bool,
    pub value: String,
}

/// 0x80063ab4: a time for the results; none is `no_time` where the race
/// counts (DNF), else blank dashes.
pub fn result_time(ms: u32, no_time: Option<&str>) -> String {
    match (ms, no_time) {
        (0, Some(t)) => t.to_string(),
        (0, None) => "--:--.--".to_string(),
        _ => clock(ms),
    }
}

/// 0x80064040 and 0x80064294: the results' table over the whole screen,
/// "CAR" and `head` in cyan at the top, then a line every 20 pixels (an
/// empty place, `None`, skipped), names at the left and times or points at
/// 248, in the race's text font (0x80063bc4: left to right, each letter by
/// its width and the kerning to the next). A player's line is green when
/// `green` (the times' table), else every line is white.
pub fn results_table(
    style: &impl crate::front::screen::Style,
    head: &str,
    lines: &[Option<ResultLine>],
    green: bool,
) -> Vec<Sprite> {
    let mut out = Vec::new();
    let mut text = |s: &str, x: i16, y: i16, colour: [u8; 3]| race_text(style, s, x, y, colour, &mut out);
    let cyan = half([0, 255, 255]);
    text("CAR", 26, 60, cyan);
    text(head, 248, 60, cyan);
    for (k, line) in lines.iter().enumerate() {
        let Some(line) = line else { continue };
        let y = 80 + 20 * k as i16;
        let colour = if green && line.player { half([0, 255, 0]) } else { WHITE };
        text(&line.name, 26, y, colour);
        text(&line.value, 248, y, colour);
    }
    out
}
