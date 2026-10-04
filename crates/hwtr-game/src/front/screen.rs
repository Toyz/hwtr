//! The front end's screens (0x800c24e8, 30 of them): lines of text the
//! menus write, labels and boxes whose text comes from the string table,
//! each letter placed by the font's spacing and sliding to its place.
//!
//! The executable holds each screen as a record of counts and pointers to
//! 44-byte text widgets, 24-byte boxes and labels and the 3D pieces; this
//! keeps what they say and where.

use super::font::ScreenFont;
use super::strings::Strings;
use crate::math::fx;

pub const SCREENS: u32 = 0x800c_24e8;
pub const SCREEN_COUNT: usize = 30;

/// A letter of a widget: where it is now and where it is going, in the
/// front end's 640 by 480 coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Letter {
    pub ch: u8,
    pub at: (i16, i16),
    pub to: (i16, i16),
}

/// How a widget comes on: from the left, from the right, or in place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Entry {
    #[default]
    InPlace,
    FromLeft,
    FromRight,
    /// Any other value: the letters are left where they were.
    Other(u8),
}

impl Entry {
    pub fn from_byte(b: u8) -> Entry {
        match b {
            0 => Entry::InPlace,
            1 => Entry::FromLeft,
            2 => Entry::FromRight,
            n => Entry::Other(n),
        }
    }
}

/// How a family of screens sets its letters. The original has two: the
/// front end's (0x8008173c and on) and the race's pause menu's (0x8009cc18
/// and on), the same widgets with their own spacing, alignment, entry and
/// slide, each a table of routines the shared code calls.
pub trait Style {
    /// How far the pen moves from `a` to `b`, and the last letter's width.
    fn advance(&self, a: u8, b: u8) -> i32;
    fn last(&self, c: u8) -> i32;
    /// Whether a label that comes on as `entry` starts at its strip's left
    /// edge (else it is centred on the strip).
    fn starts_left(&self, entry: Entry) -> bool;
    /// The screen's width, to which strips are cut, if they are.
    fn cut_at(&self) -> Option<i16>;
    /// Where a letter whose place is `to` starts as it comes on; `None`
    /// leaves it where it is.
    fn comes_from(&self, entry: Entry, to: (i16, i16)) -> Option<(i16, i16)>;
    /// A letter `f` (4.12) of the way to its place.
    fn slide(&self, l: &mut Letter, f: i32);
}

/// The front end's: its font's spacing, labels coming from the left start
/// at their left edge, letters come on from 640 pixels either side and
/// slide only rightward and downward.
impl Style for ScreenFont {
    fn advance(&self, a: u8, b: u8) -> i32 {
        ScreenFont::advance(self, a, b)
    }

    fn last(&self, c: u8) -> i32 {
        ScreenFont::last(self, c)
    }

    fn starts_left(&self, entry: Entry) -> bool {
        entry == Entry::FromLeft
    }

    fn cut_at(&self) -> Option<i16> {
        None
    }

    /// 0x80081bec and its twins.
    fn comes_from(&self, entry: Entry, to: (i16, i16)) -> Option<(i16, i16)> {
        match entry {
            Entry::FromRight => Some((to.0.wrapping_add(640), to.1)),
            Entry::FromLeft => Some((to.0.wrapping_sub(640), to.1)),
            Entry::InPlace => Some(to),
            Entry::Other(_) => None,
        }
    }

    /// 0x800828f8 and its twins: a letter with less than a pixel to go, or
    /// going leftward or upward, jumps there.
    fn slide(&self, l: &mut Letter, f: i32) {
        let dx = fx((l.to.0 as i32 - l.at.0 as i32) << 12, f) >> 12;
        let dy = fx((l.to.1 as i32 - l.at.1 as i32) << 12, f) >> 12;
        l.at.0 = if dx > 0 { l.at.0.wrapping_add(dx as i16) } else { l.to.0 };
        l.at.1 = if dy > 0 { l.at.1.wrapping_add(dy as i16) } else { l.to.1 };
    }
}

/// A line of text a menu writes (the 44-byte widget).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text {
    /// At most 24 characters.
    pub text: Vec<u8>,
    pub letters: Vec<Letter>,
    pub x: i16,
    pub y: i16,
    /// +0x24, which nothing on the screens ported so far reads.
    pub extra: i16,
    pub entry: Entry,
    pub centred: bool,
    /// Only spacing depends on it: font 2 is fixed-pitch, 35 a letter.
    pub font: u8,
    pub colour: [u8; 3],
}

/// What 0x8008649c sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    pub text: &'a str,
    pub x: i16,
    pub y: i16,
    pub extra: i16,
    pub entry: u8,
    pub centred: bool,
    pub font: u8,
    pub colour: [u8; 3],
}

impl Text {
    /// 0x8008649c: the widget's text and look. The letters follow at the
    /// next [`Text::copy_letters`].
    pub fn set(&mut self, line: &Line) {
        let mut text = line.text.as_bytes().to_vec();
        text.truncate(24);
        self.text = text;
        self.x = line.x;
        self.y = line.y;
        self.extra = line.extra;
        self.entry = Entry::from_byte(line.entry);
        self.centred = line.centred;
        self.font = line.font;
        self.colour = line.colour;
    }

    /// 0x80087830: the text into the letters, positions kept.
    pub fn copy_letters(&mut self) {
        let n = self.text.len().min(24);
        self.letters.resize(n, Letter::default());
        for (l, &c) in self.letters.iter_mut().zip(&self.text) {
            l.ch = c;
        }
    }

    /// 0x80081540: each letter's place, the line centred on `x` if asked;
    /// the letters jump there.
    pub fn lay_out(&mut self, font: &ScreenFont) {
        let n = self.letters.len();
        if n == 0 {
            return;
        }
        let step = |a: u8, b: u8| if self.font == 2 { 35 } else { font.advance(a, b) };
        let mut width = 0;
        for k in 0..n - 1 {
            width += step(self.letters[k].ch, self.letters[k + 1].ch);
        }
        width += if self.font == 2 { 35 } else { font.last(self.letters[n - 1].ch) };
        let half = width / 2;
        let mut x = if self.centred { self.x as i32 - half } else { self.x as i32 };
        let y = self.y;
        for k in 0..n {
            if k > 0 {
                x += step(self.letters[k - 1].ch, self.letters[k].ch);
            }
            let l = &mut self.letters[k];
            l.to = (x as i16, y);
            l.at = l.to;
        }
    }

    /// 0x80081bec: the letters to where they come on from.
    pub fn start_entry(&mut self, style: &impl Style) {
        for l in &mut self.letters {
            if let Some(at) = style.comes_from(self.entry, l.to) {
                l.at = at;
            }
        }
    }

    /// 0x80081d28: the letters' way off: to the right, for good, or
    /// sliding off whichever side they came from.
    pub fn start_exit(&mut self) {
        for l in &mut self.letters {
            match self.entry {
                Entry::InPlace => {
                    l.to = (l.at.0.wrapping_add(640), l.at.1);
                    l.at = l.to;
                }
                Entry::FromRight => l.to = (l.at.0.wrapping_add(640), l.at.1),
                Entry::FromLeft => l.to = (l.at.0.wrapping_sub(640), l.at.1),
                Entry::Other(_) => {}
            }
        }
    }

    /// 0x800828f8.
    pub fn slide(&mut self, f: i32, style: &impl Style) {
        for l in &mut self.letters {
            style.slide(l, f);
        }
    }
}

/// A box or label: a line from the string table on a strip from `left`
/// to `right` (the 24-byte widget).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Label {
    pub string: u16,
    pub letters: Vec<Letter>,
    pub left: i16,
    pub right: i16,
    pub y: i16,
    pub colour: [u8; 3],
    /// For a label, also its alignment: one that comes from the left
    /// starts at `left`; any other is centred on the strip.
    pub entry: Entry,
}

impl Label {
    /// The 24-byte record at `at`.
    pub(crate) fn read(word: &dyn Fn(u32) -> u32, byte: &dyn Fn(u32) -> u8, at: u32) -> Label {
        let half = |a: u32| (word(a) & 0xffff) as u16 as i16;
        Label {
            string: half(at) as u16,
            letters: Vec::new(),
            left: half(at + 8),
            right: half(at + 10),
            y: half(at + 12),
            colour: [byte(at + 16), byte(at + 17), byte(at + 18)],
            entry: Entry::from_byte(byte(at + 19)),
        }
    }

    /// 0x800878b4, 0x80087988: the string's letters.
    pub(crate) fn fill(&mut self, strings: &Strings) {
        self.letters = strings.get(self.string as usize).bytes().map(|ch| Letter { ch, ..Letter::default() }).collect();
    }

    /// 0x8008173c (a box: always centred) and 0x800818f4 (a label); in the
    /// pause menu 0x8009cc18 and 0x8009ce2c (both centred, cut to the
    /// screen).
    pub fn lay_out(&mut self, font: &impl Style, label: bool) {
        if let Some(w) = font.cut_at()
            && self.right > w
        {
            self.right = w;
        }
        let n = self.letters.len();
        let middle = (self.left as i32 + self.right as i32) / 2;
        let mut x = if label && font.starts_left(self.entry) {
            self.left as i32
        } else {
            let mut width = 0;
            for k in 0..n.saturating_sub(1) {
                width += font.advance(self.letters[k].ch, self.letters[k + 1].ch);
            }
            // The original reads the last letter even of an empty string.
            width += font.last(self.letters.last().map_or(0, |l| l.ch));
            middle - width / 2
        };
        for k in 0..n {
            if k > 0 {
                x += font.advance(self.letters[k - 1].ch, self.letters[k].ch);
            }
            let l = &mut self.letters[k];
            l.to = (x as i16, self.y);
            l.at = l.to;
        }
    }

    /// 0x80081e8c, 0x800820a8; 0x8009d040, 0x8009d17c.
    pub fn start_entry(&mut self, style: &impl Style) {
        for l in &mut self.letters {
            if let Some(at) = style.comes_from(self.entry, l.to) {
                l.at = at;
            }
        }
    }

    /// A frame's slide.
    pub fn slide(&mut self, f: i32, style: &impl Style) {
        for l in &mut self.letters {
            style.slide(l, f);
        }
    }

    /// 0x80081fc8, 0x800821e4: off the side they came from.
    fn start_exit(&mut self) {
        for l in &mut self.letters {
            match self.entry {
                Entry::FromRight => l.to = (l.at.0.wrapping_add(640), l.at.1),
                Entry::FromLeft => l.to = (l.at.0.wrapping_sub(640), l.at.1),
                _ => {}
            }
        }
    }
}

/// A 3D piece on a screen: a model by name, where it rests (the front end's
/// 1000-wide coordinates, turned into world units by 0x800813fc and its
/// twins), how it comes on, its size in percent; and where it is now, where
/// it is going, how it is turned (the 16-byte record and the 68-byte one
/// beside it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Piece {
    pub model: String,
    pub rest: [i16; 3],
    pub entry: Entry,
    pub size: u16,
    pub pos: [i32; 3],
    pub to: [i32; 3],
    pub turn: [i32; 3],
    pub turn_to: [i32; 3],
    /// Percent, where it is going and where it is.
    pub size_to: u16,
    pub size_now: u16,
}

/// 0x800813fc: a front-end x (0 to 1000) in world units, 500 the middle.
pub fn world_x(x: i32) -> i32 {
    fx((x << 12) / 1000, 0x3e_8000).wrapping_add(-0x1f_4000)
}

/// 0x80081468: a y (0 at the top to 1000) in world units, up positive.
pub fn world_y(y: i32) -> i32 {
    fx(-((y << 12) / 1000), 0x2e_e000).wrapping_add(0x17_7000)
}

/// 0x800814d0: a depth, -500 to 500, in world units.
pub fn world_z(z: i32) -> i32 {
    fx(((z + 500) << 12) / 1000, 0x3e_8000).wrapping_add(-0x1f_4000)
}

impl Piece {
    /// 0x80082590: to its resting place, coming on as its entry says.
    fn enter(&mut self) {
        let [x, y, z] = self.rest.map(i32::from);
        self.to = [world_x(x), world_y(y), world_z(z)];
        match self.entry {
            Entry::InPlace => {
                self.pos = self.to;
                self.turn = [0; 3];
            }
            Entry::FromRight => {
                self.pos = [self.to[0].wrapping_add(0x1f_4000 - world_x(x)), self.to[1], self.to[2]];
                self.turn = [0; 3];
            }
            _ => {}
        }
        self.turn_to = [0; 3];
        self.size_to = self.size;
        self.size_now = self.size;
    }

    /// 0x800827b8: off to the right if it came from there.
    fn exit(&mut self) {
        if self.entry == Entry::FromRight {
            self.to = [self.pos[0].wrapping_add(0x1f_41f4), self.pos[1], self.pos[2]];
            self.turn_to = [0; 3];
        }
        self.turn = [0; 3];
        self.size_to = self.size;
        self.size_now = self.size;
    }

    /// 0x80082c20: a fraction `f` (4.12) of the way, place, turn and size.
    fn slide(&mut self, f: i32) {
        for k in 0..3 {
            self.pos[k] = self.pos[k].wrapping_add(fx(self.to[k].wrapping_sub(self.pos[k]), f));
            self.turn[k] = self.turn[k].wrapping_add(fx(self.turn_to[k].wrapping_sub(self.turn[k]), f));
        }
        let d = fx((self.size_to as i16 as i32 - self.size_now as i16 as i32) << 12, f) >> 12;
        self.size_now = self.size_now.wrapping_add(d as u16);
    }

    /// What 0x8008429c draws: the model where the piece is, its size in
    /// 4.12 (percent over 100).
    fn draw(&self) -> PieceDraw {
        let s = (self.size_now as i16 as i32) << 12;
        let scale = ((s / 0x6_4000) << 12) + (((s % 0x6_4000) << 12) / 0x6_4000);
        PieceDraw { model: self.model.clone(), pos: self.pos, turn: self.turn, scale }
    }
}

/// A 3D piece to draw (0x80083ac0): a model, its place in world units, its
/// turn about x, y and z (4.12 radians), its scale (4.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceDraw {
    pub model: String,
    pub pos: [i32; 3],
    pub turn: [i32; 3],
    pub scale: i32,
}

/// One screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Screen {
    pub texts: Vec<Text>,
    pub boxes: Vec<Label>,
    pub labels: Vec<Label>,
    pub pieces: Vec<Piece>,
    /// When the screen last moved (0x80136b98), for its slides.
    pub moved: u32,
}

impl Screen {
    /// The record at `at`.
    pub(crate) fn read(word: &dyn Fn(u32) -> u32, byte: &dyn Fn(u32) -> u8, at: u32) -> Screen {
        let list = |count: u32, ptr: u32, size: u32| -> Vec<u32> {
            let (n, p) = (byte(at + count) as u32, word(at + ptr));
            (0..n).map(|k| p + size * k).collect()
        };
        let cstr = |mut a: u32| {
            let mut s = String::new();
            while byte(a) != 0 && s.len() < 32 {
                s.push(byte(a) as char);
                a += 1;
            }
            s
        };
        Screen {
            texts: list(0x00, 0x04, 44).iter().map(|_| Text::default()).collect(),
            boxes: list(0x08, 0x0c, 24).into_iter().map(|a| Label::read(word, byte, a)).collect(),
            labels: list(0x10, 0x14, 24).into_iter().map(|a| Label::read(word, byte, a)).collect(),
            pieces: list(0x20, 0x24, 16)
                .into_iter()
                .map(|a| {
                    let half = |at: u32| (word(at) & 0xffff) as u16;
                    Piece {
                        model: cstr(word(a)),
                        rest: [half(a + 4) as i16, half(a + 6) as i16, half(a + 8) as i16],
                        entry: Entry::from_byte(byte(a + 10)),
                        size: half(a + 12),
                        ..Piece::default()
                    }
                })
                .collect(),
            moved: 0,
        }
    }

    /// At boot (0x80081ab0, 0x800824a0): everything laid out, the boxes and
    /// labels set to come on.
    pub fn boot(&mut self, font: &ScreenFont) {
        for t in &mut self.texts {
            t.lay_out(font);
        }
        for b in &mut self.boxes {
            b.lay_out(font, false);
            b.start_entry(font);
        }
        for l in &mut self.labels {
            l.lay_out(font, true);
            l.start_entry(font);
        }
    }

    /// 0x80083278: the texts laid out again where they stand.
    pub fn lay_out_texts(&mut self, font: &ScreenFont) {
        for t in &mut self.texts {
            t.lay_out(font);
        }
    }

    /// 0x800822c4: everything laid out and sent to where it comes on from.
    pub fn enter(&mut self, font: &ScreenFont) {
        for t in &mut self.texts {
            t.lay_out(font);
            t.start_entry(font);
        }
        for b in &mut self.boxes {
            b.lay_out(font, false);
            b.start_entry(font);
        }
        for l in &mut self.labels {
            l.lay_out(font, true);
            l.start_entry(font);
        }
        for p in &mut self.pieces {
            p.enter();
        }
    }

    /// 0x800831ec: the texts only, again (after a menu rewrote them).
    pub fn reenter_texts(&mut self, font: &ScreenFont) {
        for t in &mut self.texts {
            t.lay_out(font);
            t.start_entry(font);
        }
    }

    /// 0x800823c4: everything on its way off.
    pub fn exit(&mut self) {
        for t in &mut self.texts {
            t.start_exit();
        }
        for b in &mut self.boxes {
            b.start_exit();
        }
        for l in &mut self.labels {
            l.start_exit();
        }
        for p in &mut self.pieces {
            p.exit();
        }
    }

    /// 0x800865cc: every text emptied.
    pub fn clear_texts(&mut self) {
        for t in &mut self.texts {
            t.text.clear();
            t.letters.clear();
        }
    }

    /// 0x80082e48 (0x8009d6ec in the pause menu): one frame of sliding at
    /// `now` (ms): a hundredth of the way per millisecond since the last, or
    /// a tenth after a pause.
    pub fn update(&mut self, now: u32, style: &impl Style) {
        let dt = now.wrapping_sub(self.moved);
        let f = if dt < 101 { ((dt as i32) << 12) / 100 } else { 4096 / 10 };
        self.moved = now;
        for t in &mut self.texts {
            t.slide(f, style);
        }
        for b in &mut self.boxes {
            b.slide(f, style);
        }
        for l in &mut self.labels {
            l.slide(f, style);
        }
        for p in &mut self.pieces {
            p.slide(f);
        }
    }

    /// 0x80086268: the glyphs to draw. `text` and `label` are the chosen
    /// ones (if any), drawn in the flashing `flash` grey; with `hidden`
    /// (while the screen fades) the texts are not drawn.
    pub fn draw(
        &self,
        text: Option<usize>,
        label: Option<usize>,
        flash: u8,
        hidden: bool,
        out: &mut Vec<Glyph>,
        pieces: &mut Vec<PieceDraw>,
    ) {
        if !hidden {
            pieces.extend(self.pieces.iter().map(Piece::draw));
            for (k, t) in self.texts.iter().enumerate() {
                let colour = if text == Some(k) { [flash; 3] } else { t.colour };
                out.extend(t.letters.iter().map(|l| Glyph::at(l, colour)));
            }
        }
        for b in &self.boxes {
            out.extend(b.letters.iter().map(|l| Glyph::at(l, b.colour)));
        }
        for (k, l) in self.labels.iter().enumerate() {
            let colour = if label == Some(k) { [flash; 3] } else { l.colour };
            out.extend(l.letters.iter().map(|g| Glyph::at(g, colour)));
        }
    }
}

/// A letter as the glyph routine draws it (0x80083a20): at x and half the
/// height less 8 on the 640 by 240 screen, upper case, in half the colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: u8,
    pub x: i16,
    pub y: i16,
    pub colour: [u8; 3],
}

impl Glyph {
    /// Letter `ch` drawn at (x, y) of the 640 by 480 screen in `colour`,
    /// as 0x80083a20 draws it.
    pub fn new(ch: u8, x: i32, y: i32, colour: [u8; 3]) -> Glyph {
        Glyph { ch: ch.to_ascii_uppercase(), x: x as i16, y: (y / 2 - 8) as i16, colour: colour.map(|c| c >> 1) }
    }

    fn at(l: &Letter, colour: [u8; 3]) -> Glyph {
        Glyph {
            ch: l.ch.to_ascii_uppercase(),
            x: l.at.0,
            y: (l.at.1 as i32 / 2 - 8) as i16,
            colour: colour.map(|c| c >> 1),
        }
    }
}

/// The 30 screens, their boxes' and labels' text filled from `strings`
/// (0x80087a50).
pub fn read_screens(byte: &dyn Fn(u32) -> u8, strings: &Strings) -> Vec<Screen> {
    let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
    (0..SCREEN_COUNT as u32)
        .map(|k| {
            let mut s = Screen::read(&word, byte, word(SCREENS + 4 * k));
            for b in s.boxes.iter_mut().chain(s.labels.iter_mut()) {
                b.fill(strings);
            }
            s
        })
        .collect()
}
