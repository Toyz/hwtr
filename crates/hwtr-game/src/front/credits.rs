//! The credits (states 197 to 201; screen 18): two pages over one of three
//! pictures, each page a list of lines scrolling up through a window, up
//! and down between them, Triangle or Start back to the options.

use super::*;

const CREDITS: usize = 18;
const CREDITS_HELP: usize = 7;
/// The two pages' scrollers (0x800bfd9c, 0x800bfdbc).
pub(super) const SCROLLERS_AT: [u32; 2] = [0x800b_fd9c, 0x800b_fdbc];

/// A scrolling list of lines (0x80080ffc's record): the lines (each bright
/// or not), centred between `left` and `right`, shown between `top` and
/// `bottom`, starting at `start`, now at `pos` (20.12).
#[derive(Clone, Debug, Default)]
pub(super) struct Scroller {
    lines: Vec<(bool, String)>,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    start: i32,
    pos: i32,
}

impl Scroller {
    pub(super) fn read(byte: &dyn Fn(u32) -> u8, at: u32) -> Scroller {
        let word = |a: u32| i32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let text = |a: u32| (0..80).map(|k| byte(a + k)).take_while(|&b| b != 0).map(char::from).collect::<String>();
        let (list, count) = (word(at) as u32, word(at + 0x1c));
        let lines =
            (0..count.max(0) as u32).map(|k| (byte(list + 8 * k) == 0, text(word(list + 8 * k + 4) as u32))).collect();
        Scroller {
            lines,
            left: word(at + 4),
            right: word(at + 8),
            top: word(at + 12),
            bottom: word(at + 16),
            start: word(at + 20),
            pos: word(at + 24),
        }
    }
}

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8009_365c, |f: &mut Front, _: &mut Poster| {
        // 0x8009365c: one of the three pictures, the first page.
        f.credits_picture = f.rand.below(3) as u8 + 1;
        f.background = Some(format!("psxcrde{}", f.credits_picture));
        f.fade_in();
        f.screens[CREDITS].enter(&f.font);
        f.credits_page = 0;
        for s in &mut f.scrollers {
            s.pos = s.start << 12;
        }
        f.helps[CREDITS_HELP].x = f.helps[CREDITS_HELP].start << 12;
    });
    r.add(0x8009_3718, |f: &mut Front, _: &mut Poster| f.screens[CREDITS].update(f.clock, &f.font));
    r.add(0x8009_3740, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        f.draw_screen(CREDITS, None, Some(f.credits_page as usize), f.fade.hidden);
        if !f.fade.hidden {
            if f.credits_page < 2 {
                f.scroll(f.credits_page as usize);
            }
            f.draw_help(CREDITS_HELP);
        }
        f.end_frame();
    });
    r.add(0x8009_36e8, |f: &mut Front, p: &mut Poster| {
        // Up from the second page, down from the first; Triangle, Start.
        let mask = if f.credits_page != 0 { 97 } else { 98 };
        f.pad_events(0, mask, p);
    });
    r.add(0x8009_3828, |f: &mut Front, _: &mut Poster| {
        if f.credits_page == 1 {
            f.credits_page = 0;
            f.background = Some(format!("psxcrde{}", f.credits_picture));
        }
    });
    r.add(0x8009_386c, |f: &mut Front, _: &mut Poster| {
        if f.credits_page == 0 {
            f.credits_page = 1;
            f.background = Some(format!("psxcrds{}", f.credits_picture));
        }
    });
    r.add(0x8009_38b0, |f: &mut Front, _: &mut Poster| {
        f.screens[CREDITS].exit();
        f.fade_out();
    });
    r.add(0x8009_38e0, |_: &mut Front, _: &mut Poster| {});
}

impl Front {
    /// 0x80080ffc: page `k`'s lines where they are (24 apart, those inside
    /// the window drawn by 0x80080e08: centred, bright 255 or 170), then 30
    /// pixels a second further up; from the start again once all are past
    /// the top.
    fn scroll(&mut self, k: usize) {
        let s = self.scrollers[k].clone();
        let mut drawn = 0;
        let mut y = 0;
        for (i, (bright, text)) in s.lines.iter().enumerate() {
            y = (s.pos >> 12) + 24 * i as i32;
            if s.top < y && y < s.bottom {
                let b = text.as_bytes();
                let width: i32 = (0..b.len())
                    .map(|i| if i + 1 < b.len() { self.font.advance(b[i], b[i + 1]) } else { self.font.last(b[i]) })
                    .sum();
                let mut x = (s.left + s.right) / 2 - width / 2;
                let shade = if *bright { 255 } else { 170 };
                for i in 0..b.len() {
                    self.drawing.push(screen::Glyph::new(b[i], x, y, [shade; 3]));
                    if i + 1 < b.len() {
                        x += self.font.advance(b[i], b[i + 1]);
                    }
                }
                drawn += 1;
            }
        }
        let sc = &mut self.scrollers[k];
        if drawn == 0 && y < 0 {
            sc.pos = sc.start << 12;
        } else {
            let secs = self.seconds(5);
            self.scrollers[k].pos -= fx(0x1_e000, secs);
        }
    }
}
