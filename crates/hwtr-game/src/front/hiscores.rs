//! The Hi-Scores screen (states 109, 316 to 324; screen 27): the card's
//! best scores and best times for each track, and the winners of each cup,
//! five to a table. Up and down choose between the table's kind and its
//! track (or cup), left and right change it. Left alone for 30 seconds it
//! shows tables at random, a line a second.

use super::*;

const HISCORES: usize = 27;
const HISCORES_HELP: usize = 2;

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8008_6cac, |f: &mut Front, _: &mut Poster| f.second_from = f.clock);
    r.add(0x8008_6c80, |f: &mut Front, _: &mut Poster| f.idle_from = f.clock);
    r.add(0x8008_6bc0, |f: &mut Front, p: &mut Poster| {
        // 0x80086bc0: event 5 each second.
        if f.clock.wrapping_sub(f.second_from) >= 1000 {
            f.second_from = f.clock;
            p.post(5);
        }
    });
    r.add(0x8008_6c34, |f: &mut Front, p: &mut Poster| {
        // 0x80086c34: event 37 after 30 seconds without a press (the
        // title's attract mode, the hi-scores' random tables).
        if f.clock.wrapping_sub(f.idle_from) >= 30_000 {
            p.post(37);
        }
    });
    r.add(0x8009_907c, |f: &mut Front, _: &mut Poster| {
        f.set_background("psxhigh");
        f.fade_in();
        f.screens[HISCORES].enter(&f.font);
        f.screens[HISCORES].clear_texts();
        f.hs = HiScores::default();
        f.hiscore_arrows();
        f.helps[HISCORES_HELP].x = f.helps[HISCORES_HELP].start << 12;
    });
    r.add(0x8009_90f8, |f: &mut Front, _: &mut Poster| f.screens[HISCORES].reenter_texts(&f.font));
    r.add(0x8009_9120, |f: &mut Front, p: &mut Poster| {
        let mask = if f.hs.row != 0 { 109 } else { 110 };
        f.pad_events(0, mask, p);
    });
    r.add(0x8009_9150, |f: &mut Front, _: &mut Poster| f.screens[HISCORES].update(f.clock, &f.font));
    r.add(0x8009_9234, |f: &mut Front, _: &mut Poster| {
        if !f.hs.cycling {
            f.random_table();
            f.hs.cycling = true;
            f.hs.shown = 0;
        }
    });
    r.add(0x8009_9268, |f: &mut Front, _: &mut Poster| f.hs.cycling = false);
    r.add(0x8009_9274, |f: &mut Front, _: &mut Poster| {
        // A line more each second; after forty, another table.
        if f.hs.cycling {
            f.hs.shown += 1;
            if f.hs.shown >= 40 {
                f.random_table();
                f.hs.shown = 0;
            }
        }
    });
    r.add(0x8009_92c0, |f: &mut Front, _: &mut Poster| {
        f.hs.row = f.hs.row.saturating_sub(1);
        f.hiscore_arrows();
    });
    r.add(0x8009_92f0, |f: &mut Front, _: &mut Poster| {
        if f.hs.row == 0 {
            f.hs.row = 1;
        }
        f.hiscore_arrows();
    });
    r.add(0x8009_9320, |f: &mut Front, _: &mut Poster| f.table_step(false));
    r.add(0x8009_946c, |f: &mut Front, _: &mut Poster| f.table_step(true));
    r.add(0x8009_95c4, |f: &mut Front, p: &mut Poster| p.post(if f.hs.row == 2 { 133 } else { 10 }));
    r.add(0x8009_960c, |f: &mut Front, _: &mut Poster| f.hiscore_texts());
    r.add(0x8009_9a08, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        f.draw_screen(HISCORES, Some(f.hs.row as usize), None, f.fade.hidden);
        if !f.fade.hidden {
            f.draw_help(HISCORES_HELP);
        }
        f.end_frame();
    });
    r.add(0x8009_9a80, |f: &mut Front, _: &mut Poster| {
        f.screens[HISCORES].exit();
        f.fade_out();
    });
    r.add(0x8009_9ab0, |_: &mut Front, _: &mut Poster| {});
}

/// The screen's state: the line chosen (0x800d1177), the kind of table
/// (0x800d275d: scores, times, cups), its track (0x800d275e) or cup
/// (0x800d275f), and the random show (0x800d2760, lines shown 0x800d2761).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct HiScores {
    row: u8,
    kind: u8,
    track: u8,
    cup: u8,
    cycling: bool,
    shown: u8,
}

impl Front {
    /// 0x8008a2f0: either player has track `t`.
    pub(super) fn track_open_either(&self, t: u8) -> bool {
        (self.players[0].tracks >> t & 1) != 0 || (self.players[1].tracks >> t & 1) != 0
    }

    /// 0x8008a33c: whether player one has cup `c`: the first always, the
    /// second with two of the profile's flags, the third with car 38.
    pub(super) fn cup_open(&self, c: u8) -> bool {
        let t = self.players[0].tracks;
        match c {
            0 => true,
            1 => t >> 7 & 1 != 0 && t >> 8 & 1 != 0,
            2 => self.car_open(38, 0),
            _ => false,
        }
    }

    /// A track the table can show: open, and on the disc.
    fn table_track(&self, t: u8) -> bool {
        self.track_open_either(t) && self.tables.track_files.get(t as usize).is_some_and(Option::is_some)
    }

    /// 0x80099178: a table at random: a kind, then an open track or cup.
    fn random_table(&mut self) {
        self.hs.kind = self.rand.below(3) as u8;
        if self.hs.kind < 2 {
            loop {
                self.hs.track = self.rand.below(12) as u8;
                if self.track_open_either(self.hs.track) {
                    break;
                }
            }
        } else {
            loop {
                self.hs.cup = self.rand.below(3) as u8;
                if self.cup_open(self.hs.cup) {
                    break;
                }
            }
        }
    }

    /// 0x80099320, 0x8009946c: the kind round, or the next open track or
    /// cup.
    fn table_step(&mut self, right: bool) {
        let step = |v: u8, n: u8| {
            if right {
                if v + 1 < n { v + 1 } else { 0 }
            } else if v == 0 {
                n - 1
            } else {
                v - 1
            }
        };
        match self.hs.row {
            0 => self.hs.kind = step(self.hs.kind, 3),
            1 if self.hs.kind < 2 => {
                for _ in 0..12 {
                    self.hs.track = step(self.hs.track, 12);
                    if self.table_track(self.hs.track) {
                        break;
                    }
                }
            }
            1 => {
                for _ in 0..3 {
                    self.hs.cup = step(self.hs.cup, 3);
                    if self.cup_open(self.hs.cup) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// 0x80098fd8: the arrows on the chosen line.
    fn hiscore_arrows(&mut self) {
        match self.hs.row {
            0 => {
                self.place_piece(HISCORES, "arrowlt", 30, 390, 100);
                self.place_piece(HISCORES, "arrowrt", 430, 390, 100);
            }
            1 => {
                self.place_piece(HISCORES, "arrowlt", 30, 520, 100);
                self.place_piece(HISCORES, "arrowrt", 430, 520, 100);
            }
            _ => {
                self.place_piece(HISCORES, "arrowlt", -1, -1, 0);
                self.place_piece(HISCORES, "arrowrt", -1, -1, 0);
            }
        }
    }

    /// 0x8009960c: the table's kind and track (or cup), and its five lines
    /// (only those shown so far while showing at random).
    fn hiscore_texts(&mut self) {
        let kinds = [66, 67, 68];
        let kind = self.strings.get(kinds[self.hs.kind.min(2) as usize]).to_string();
        let which = if self.hs.kind < 2 {
            self.tables.track_names.get(self.hs.track as usize).cloned().unwrap_or_default()
        } else {
            self.strings.get(59 + self.hs.cup as usize).to_string()
        };
        self.set_text(HISCORES, 0, &kind, 33, 180, 0, 0, false, 0, [255; 3]);
        self.set_text(HISCORES, 1, &which, 33, 240, 0, 0, false, 0, [255; 3]);
        let base = if self.hs.kind < 2 { self.hs.track } else { self.hs.cup } as usize * 5;
        for i in 0..5 {
            let y = 155 + 53 * i as i16;
            let (name, value) = {
                let table = match self.hs.kind {
                    0 => &self.card.high_scores,
                    1 => &self.card.best_times,
                    _ => &self.card.cup_winners,
                };
                let s = table.get(base + i).cloned().unwrap_or(card::Score { name: String::new(), value: 0 });
                let value = if self.hs.kind == 1 {
                    let ms = s.value;
                    format!("{:02}:{:02}.{:02}", ms / 1000 / 60, ms / 1000 % 60, ms % 1000 / 10)
                } else {
                    s.value.to_string()
                };
                (format!("{}. {}", i + 1, s.name), value)
            };
            let (name, value) = if self.hs.cycling && i as u8 >= self.hs.shown {
                (String::new(), String::new())
            } else {
                (name, value)
            };
            self.set_text(HISCORES, 2 + i, &name, 290, y, 0, 0, false, 0, [255; 3]);
            self.set_text(HISCORES, 7 + i, &value, 508, y, 0, 0, false, 0, [255; 3]);
        }
    }
}
