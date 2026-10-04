//! The cups (states 62, 97, 113 to 155, 339 to 343; screens 5 to 9): the
//! Hot Wheels Cup (six tracks), the Secret Car Cup (two) and the TwinMill
//! Cup (one, against the two TwinMills). The cup screen shows the round,
//! the rank, the next track and its difficulty and strategy; between
//! rounds the standings; at the end victory, or "Loser!".
//!
//! The player keeps the cup (profile +0x22), the round (+0x23) and the six
//! drivers' points (+0x24, `Profile::records`); the times are the race's
//! own (0x80139924, not kept).

use super::*;

const CUP: usize = 5;
const CONFIRM: usize = 6;
const STANDINGS: usize = 7;
const WON: usize = 8;
const LOST: usize = 9;
const CUP_HELP: usize = 3;
/// The cups' tracks in order, -1 for none (0x800c2560, nine each).
pub(super) const CUP_TRACKS: u32 = 0x800c_2560;
/// The points a car is given when the race was left (0x800d1204).
const LEFT_POINTS: [u8; 5] = [5, 6, 7, 8, 10];

/// The cup screens' state: the line chosen (0x800d1174), whether the cup
/// screen is the menu (0x800d1169), the "cup will be lost" choice
/// (0x800d27a8), when a result came up (0x800d27a4), the drivers' total
/// times (0x80139924).
#[derive(Clone, Debug, Default)]
pub(super) struct CupScreens {
    line: u8,
    choice: u8,
    shown: u32,
    times: [u32; 6],
}

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8008_e0d0, |f: &mut Front, _: &mut Poster| {
        // A cup or round out of range starts the Hot Wheels Cup afresh.
        let p = &mut f.players[0];
        if p.progress[0] >= 3 || p.progress[1] >= 9 {
            p.progress[0] = 0;
            f.cup_reset();
        }
    });
    r.add(0x8008_df74, |f: &mut Front, _: &mut Poster| f.cup_enter());
    r.add(0x8008_e230, |f: &mut Front, _: &mut Poster| {
        let t = f.cup_track();
        f.track_model = t.and_then(|t| f.tables.track_files.get(t as usize).cloned().flatten());
    });
    r.add(0x8008_e8e0, |f: &mut Front, _: &mut Poster| f.cup_texts());
    r.add(0x8008_edac, |f: &mut Front, _: &mut Poster| f.screens[CUP].reenter_texts(&f.font));
    r.add(0x8008_edd4, |f: &mut Front, _: &mut Poster| f.screens[CUP].lay_out_texts(&f.font));
    r.add(0x8008_e2e8, |f: &mut Front, _: &mut Poster| f.screens[CUP].update(f.clock, &f.font));
    r.add(0x8008_e310, |f: &mut Front, p: &mut Poster| f.cup_draw(p));
    r.add(0x8008_e534, |f: &mut Front, p: &mut Poster| f.cup_pads(p));
    r.add(0x8008_e11c, |f: &mut Front, p: &mut Poster| {
        // On the car line, a car not shown is asked for.
        if f.cup.line == 2 && !f.car_asked[0] && !f.car_loaded[0] {
            f.car_changed[0] = f.clock;
            p.post(50);
        }
    });
    r.add(0x8008_e1e8, |f: &mut Front, _: &mut Poster| {
        f.preview_place(0, 435, 450, 0);
        f.preview_aim(0, 435, 450, 0);
        f.previews[0].size_to(0x1800);
    });
    r.add(0x8008_e1b4, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[0] {
            f.preview_aim(0, 435, 450, 0);
        }
    });
    r.add(0x8008_ee64, |f: &mut Front, _: &mut Poster| {
        f.cup.line = 0;
        f.in_cup = false;
    });
    r.add(0x8008_ee74, |f: &mut Front, _: &mut Poster| {
        f.screens[CUP].exit();
        f.previews[0].size_to(0);
        f.track_map.scale_to = 0;
        f.track_map.stopped = false;
        f.fade_out();
    });
    r.add(0x8008_eeb4, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_eebc, |f: &mut Front, _: &mut Poster| f.cup_line_step(false));
    r.add(0x8008_efa8, |f: &mut Front, _: &mut Poster| f.cup_line_step(true));
    r.add(0x8008_f13c, |f: &mut Front, p: &mut Poster| {
        // Left or right: the car (56), the sign-in choice (59) or, before
        // the first round, the cup (79).
        p.post(match f.cup.line {
            2 => 56,
            3 => 59,
            5 => 79,
            _ => 60,
        });
    });
    r.add(0x8008_f090, |f: &mut Front, p: &mut Poster| {
        // Cross: race (76), leave the cup (75), the garage (67), sign in
        // (66), options (68).
        p.post(match f.cup.line {
            0 => 76,
            1 => 75,
            2 => 67,
            3 => 66,
            4 => 68,
            _ => 60,
        });
    });
    r.add(0x8008_edfc, |f: &mut Front, p: &mut Poster| p.post(70 + f.sign_in_choice.min(2) as i16));
    r.add(0x8008_f2f4, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.car_step(0, false);
    });
    r.add(0x8008_f330, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.car_step(0, true);
    });
    r.add(0x8008_f36c, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.sign_in_choice = if f.sign_in_choice == 0 { 2 } else { f.sign_in_choice - 1 };
    });
    r.add(0x8008_f3b8, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.sign_in_choice = if f.sign_in_choice >= 2 { 0 } else { f.sign_in_choice + 1 };
    });
    r.add(0x8008_f1c4, |f: &mut Front, _: &mut Poster| f.cup_step(false));
    r.add(0x8008_f260, |f: &mut Front, _: &mut Poster| f.cup_step(true));
    // Leaving a cup under way: "Cup will be lost! Are you sure?"
    r.add(0x8009_0024, |f: &mut Front, _: &mut Poster| {
        f.cup.choice = 3;
        f.set_text(CONFIRM, 0, "Cup will be lost!", 320, 160, 0, 0, true, 1, [255; 3]);
        f.set_text(CONFIRM, 1, "Are you sure?", 320, 190, 0, 0, true, 1, [255; 3]);
        let (yes, no) = (f.strings.get(90).to_string(), f.strings.get(93).to_string());
        f.set_text(CONFIRM, 2, &yes, 320, 235, 0, 0, true, 1, [127; 3]);
        f.set_text(CONFIRM, 3, &no, 320, 265, 0, 0, true, 1, [127; 3]);
        f.screens[CONFIRM].enter(&f.font);
    });
    r.add(0x8008_fffc, |f: &mut Front, _: &mut Poster| f.screens[CONFIRM].reenter_texts(&f.font));
    r.add(0x8009_0174, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        f.draw_screen(CONFIRM, Some(f.cup.choice as usize), None, false);
        f.end_frame();
    });
    r.add(0x8009_0240, |f: &mut Front, p: &mut Poster| {
        let mask = if f.cup.choice == 3 { 17 } else { 18 };
        f.pad_events(0, mask, p);
    });
    r.add(0x8009_0220, |f: &mut Front, _: &mut Poster| f.cup.choice = 2);
    r.add(0x8009_0230, |f: &mut Front, _: &mut Poster| f.cup.choice = 3);
    r.add(0x8009_01d0, |f: &mut Front, p: &mut Poster| {
        if f.cup.choice == 3 {
            f.fade_in();
            p.post(10);
        } else {
            p.post(78);
        }
    });
    r.add(0x8008_e088, |f: &mut Front, _: &mut Poster| f.cup_reset());
    // The round run.
    r.add(0x8009_c44c, |f: &mut Front, _: &mut Poster| f.set_up_cup_race());
    r.add(0x8009_c9b8, |f: &mut Front, p: &mut Poster| {
        if f.race.is_none() && f.race_end.is_some() {
            p.post(10);
        }
    });
    r.add(0x8009_c8d4, |f: &mut Front, p: &mut Poster| {
        f.cd.push(crate::cd::CdAsk::Stop);
        let result = f.race_end.take().unwrap_or_else(|| RaceResult::ended(RaceEnd::Other(0)));
        p.post(match result.end.code() {
            1 => 136,
            2 => 137,
            3 => 138,
            _ => 134,
        });
        for pl in &mut f.players {
            pl.cheats = 0;
        }
        f.take_result(&result);
        f.cup_result = Some(result);
    });
    r.add(0x8008_e6cc, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_e6b0, |f: &mut Front, _: &mut Poster| f.players[0].progress[1] += 1);
    r.add(0x8008_e6d4, |f: &mut Front, _: &mut Poster| f.cup_left());
    // The standings.
    r.add(0x8008_f4bc, |f: &mut Front, _: &mut Poster| {
        f.set_background("psxcprs");
        f.screens[STANDINGS].enter(&f.font);
        f.fade_in();
    });
    r.add(0x8008_f4f8, |f: &mut Front, _: &mut Poster| f.cup_standings());
    r.add(0x8008_f9a0, |f: &mut Front, _: &mut Poster| f.screens[STANDINGS].reenter_texts(&f.font));
    r.add(0x8008_f9c8, |f: &mut Front, p: &mut Poster| {
        f.pad_events(0, 80, p);
    });
    r.add(0x8008_f9e8, |f: &mut Front, _: &mut Poster| f.screens[STANDINGS].update(f.clock, &f.font));
    r.add(0x8008_fa10, |f: &mut Front, p: &mut Poster| f.cup_screen_draw(STANDINGS, p));
    r.add(0x8008_fa70, |f: &mut Front, _: &mut Poster| f.fade_out());
    r.add(0x8008_fa90, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_f410, |f: &mut Front, p: &mut Poster| {
        // 0x8008f410: the cup over? First on points wins (event 81),
        // otherwise lost (82); more rounds, event 83.
        f.cup_won = false;
        let over = f.cup_track().is_none();
        if over {
            if f.cup_rank(0) == 0 {
                f.cup_won = true;
                p.post(81);
            } else {
                p.post(82);
            }
        } else {
            p.post(83);
        }
    });
    // Victory and defeat.
    r.add(0x8008_fae0, |f: &mut Front, _: &mut Poster| {
        f.set_background("psxwin");
        f.screens[WON].enter(&f.font);
        f.fade_in();
        f.cup.shown = f.clock;
    });
    r.add(0x8008_fb34, |f: &mut Front, _: &mut Poster| {
        let name = f.players[0].name.clone();
        let (a, b, c) =
            (f.strings.get(265).to_string(), f.strings.get(266).to_string(), f.strings.get(267).to_string());
        f.set_text(WON, 0, &a, 160, 210, 0, 2, true, 1, [255; 3]);
        f.set_text(WON, 1, &format!("{name} {b}"), 160, 240, 0, 2, true, 1, [255; 3]);
        f.set_text(WON, 2, &c, 160, 270, 0, 2, true, 1, [255; 3]);
    });
    r.add(0x8008_fc84, |f: &mut Front, _: &mut Poster| f.screens[WON].reenter_texts(&f.font));
    r.add(0x8008_fcac, |f: &mut Front, _: &mut Poster| f.screens[WON].update(f.clock, &f.font));
    r.add(0x8008_fcd4, |f: &mut Front, p: &mut Poster| f.cup_screen_draw(WON, p));
    r.add(0x8008_fa98, |f: &mut Front, p: &mut Poster| {
        if f.cup.shown.wrapping_add(5000) < f.clock {
            f.pad_events(0, 80, p);
        }
    });
    r.add(0x8008_fd34, |f: &mut Front, _: &mut Poster| f.fade_out());
    r.add(0x8008_fd54, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_fda4, |f: &mut Front, _: &mut Poster| {
        f.set_background("psxlose");
        f.screens[LOST].enter(&f.font);
        f.fade_in();
        f.cup.shown = f.clock;
    });
    r.add(0x8008_fdf8, |f: &mut Front, _: &mut Poster| {
        let name = f.players[0].name.clone();
        f.set_text(LOST, 0, "Loser!", 167, 195, 0, 2, true, 1, [255; 3]);
        f.set_text(LOST, 1, &format!("{name} has"), 167, 230, 0, 2, true, 1, [255; 3]);
        f.set_text(LOST, 2, "no skills whatsoever!", 167, 260, 0, 2, true, 1, [255; 3]);
    });
    r.add(0x8008_ff24, |f: &mut Front, _: &mut Poster| f.screens[LOST].reenter_texts(&f.font));
    r.add(0x8008_ff4c, |f: &mut Front, _: &mut Poster| f.screens[LOST].update(f.clock, &f.font));
    r.add(0x8008_ff74, |f: &mut Front, p: &mut Poster| f.cup_screen_draw(LOST, p));
    r.add(0x8008_fd5c, |f: &mut Front, p: &mut Poster| {
        if f.cup.shown.wrapping_add(5000) < f.clock {
            f.pad_events(0, 80, p);
        }
    });
    r.add(0x8008_ffd4, |f: &mut Front, _: &mut Poster| f.fade_out());
    r.add(0x8008_fff4, |_: &mut Front, _: &mut Poster| {});
}

impl Front {
    /// The track of the cup's round now, if the cup has one (0x800c2560).
    fn cup_track(&self) -> Option<u8> {
        let p = &self.players[0];
        if p.progress[1] >= 9 {
            return None;
        }
        let t = self.cup_tracks.get(p.progress[0] as usize * 9 + p.progress[1] as usize).copied().unwrap_or(-1);
        (t >= 0).then_some(t as u8)
    }

    /// How many rounds the cup has.
    fn cup_rounds(&self) -> u8 {
        let c = self.players[0].progress[0] as usize;
        (0..9).take_while(|&k| self.cup_tracks.get(c * 9 + k).is_some_and(|&t| t >= 0)).count() as u8
    }

    /// 0x8008de60: driver `k`'s place on points (ties to the lower index).
    fn cup_rank(&self, k: usize) -> u8 {
        let pts = &self.players[0].records;
        (0..6).filter(|&j| j != k && (pts[k] < pts[j] || (pts[j] == pts[k] && j < k))).count() as u8
    }

    /// 0x8008ded0: how many drivers have more points than driver `k`.
    fn cup_ahead(&self, k: usize) -> u8 {
        let pts = &self.players[0].records;
        (0..6).filter(|&j| j != k && pts[k] < pts[j]).count() as u8
    }

    /// 0x8008e088: the cup from its first round, no points, no times.
    fn cup_reset(&mut self) {
        self.players[0].progress[1] = 0;
        self.players[0].records = [0; 6];
        self.cup.times = [0; 6];
    }

    /// 0x8008df74: the cup screen comes up: one player; a locked car is
    /// swapped for the default; the car and the track map come in.
    fn cup_enter(&mut self) {
        self.in_cup = true;
        self.set_background("psxhwc");
        self.fade_in();
        self.people = 1;
        for k in 0..2 {
            if !self.car_open(self.players[k].car, k) {
                self.players[k].car = 29;
            }
        }
        // 0x800848a0: the preview from nothing.
        self.previews[0].scale = 0;
        self.previews[0].size_to(4096);
        self.previews[0].angle_to = CAR_REST;
        self.preview_place(0, 500, 500, 0);
        self.preview_aim(0, 435, 450, 0);
        let m = &mut self.track_map;
        m.scale = 0;
        m.stopped = false;
        m.scale_to = 2048;
        m.spin_to = TRACK_REST;
        m.pos = world_at(500, 500, 0);
        m.pos_to = world_at(1070, 950, 0);
        self.screens[CUP].enter(&self.font);
        self.car_shown[0] = false;
        self.helps[CUP_HELP].x = self.helps[CUP_HELP].start << 12;
        self.cup_arrows();
    }

    /// 0x8008dd78: the arrows on the car, sign-in and cup lines.
    fn cup_arrows(&mut self) {
        let at = match self.cup.line {
            2 => Some((19, 240, 520)),
            3 => Some((19, 260, 590)),
            5 => Some((19, 390, 730)),
            _ => None,
        };
        match at {
            Some((l, r, y)) => {
                self.place_piece(CUP, "arrowlt", l, y, 100);
                self.place_piece(CUP, "arrowrt", r, y, 100);
            }
            None => {
                self.place_piece(CUP, "arrowlt", -1, -1, 0);
                self.place_piece(CUP, "arrowrt", -1, -1, 0);
            }
        }
    }

    /// 0x8008e8e0: the cup screen's lines: the player, the car, the next
    /// track, the cup, the sign-in choice, the round, the rank, the track's
    /// difficulty and strategy.
    fn cup_texts(&mut self) {
        let p = self.players[0].clone();
        let track = self.cup_track().unwrap_or(0) as usize;
        let car = self.tables.car_names.get(p.car as usize).cloned().unwrap_or_default();
        let track_name = self.tables.track_names.get(track).cloned().unwrap_or_default();
        let cup_name = self.strings.get(59 + p.progress[0].min(2) as usize).to_string();
        let sign_in = self.strings.get(self.tables.sign_in[self.sign_in_choice.min(2) as usize] as usize).to_string();
        self.set_text(CUP, 0, &p.name, 273, 122, 0, 0, true, 0, [255; 3]);
        self.set_text(CUP, 2, &car, 273, 238, 0, 0, true, 1, [255; 3]);
        self.set_text(CUP, 4, &track_name, 505, 280, 0, 0, true, 0, [255; 3]);
        self.set_text(CUP, 5, &cup_name, 23, 342, 0, 1, false, 1, [127; 3]);
        self.set_text(CUP, 6, &sign_in, 23, 274, 0, 1, false, 1, [127; 3]);
        let round =
            format!("{} {} {} {}", self.strings.get(258), p.progress[1] + 1, self.strings.get(259), self.cup_rounds());
        self.set_text(CUP, 1, &round, 410, 128, 0, 0, false, 1, [255; 3]);
        let rank = if p.progress[1] == 0 {
            self.strings.get(260).to_string()
        } else {
            format!("{} {}", self.strings.get(261), self.cup_rank(0) + 1)
        };
        self.set_text(CUP, 3, &rank, 410, 158, 0, 0, false, 1, [255; 3]);
        let difficulty = self.track_difficulty.get(track).cloned().unwrap_or_default();
        let strategy = self.track_strategy.get(track).cloned().unwrap_or_default();
        self.set_text(CUP, 7, &format!("{} {}", self.strings.get(262), difficulty), 410, 188, 0, 0, false, 1, [255; 3]);
        self.set_text(CUP, 8, &format!("{} {}", self.strings.get(263), strategy), 410, 218, 0, 0, false, 1, [255; 3]);
    }

    /// 0x8008e310: the cup screen, its car and track map, the help line.
    fn cup_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        let (text, label) = match self.cup.line {
            3 => (Some(6), None),
            4 => (None, Some(3)),
            5 => (Some(5), None),
            l => (None, Some(l as usize)),
        };
        self.draw_screen(CUP, text, label, self.fade.hidden);
        if self.car_loaded[0] {
            self.draw_preview(0);
        } else if self.car_asked[0] {
            self.car_loaded[0] = true;
            self.car_shown[0] = false;
            self.car_asked[0] = false;
            if self.cup.line == 2 {
                self.preview_aim(0, 445, 480, 0);
            } else {
                self.preview_aim(0, 435, 450, 0);
            }
            self.car_arrived(0);
        }
        self.draw_decal(0, 191, 61);
        self.draw_track_map();
        if !self.fade.hidden {
            self.draw_help(CUP_HELP);
        }
        self.end_frame();
    }

    /// 0x8008e534: the presses (Cross on all but the cup line), left and
    /// right held on the car and sign-in lines, and on the cup line before
    /// the first round when another cup is open.
    fn cup_pads(&mut self, p: &mut Poster) {
        if self.car_asked[0] && !self.car_loaded[0] {
            return;
        }
        let mask = if self.cup.line < 5 { 99 | 16 } else { 99 };
        self.pad_events(0, mask, p);
        let line = self.cup.line;
        let steps = matches!(line, 2 | 3)
            || (line == 5 && self.players[0].progress[1] == 0 && (self.cup_open(1) || self.cup_open(2)));
        if steps {
            self.held_repeat(0, (16, 46), (17, 47), p);
        }
    }

    /// 0x8008eebc, 0x8008efa8: the line above or below, the car and the
    /// map following as on the main menu.
    fn cup_line_step(&mut self, down: bool) {
        if self.cup.line == 2 {
            self.preview_full(0);
            self.preview_aim(0, 435, 450, 0);
        }
        if self.cup.line == 5 {
            self.track_map.stopped = false;
            self.track_map.scale_to = 2048;
            self.track_map.spin_to = TRACK_REST;
            self.track_map.pos_to = world_at(1070, 950, 0);
        }
        self.cup.line = match (down, self.cup.line) {
            (false, 0) => 5,
            (false, l) => l - 1,
            (true, l) if l < 5 => l + 1,
            (true, _) => 0,
        };
        if self.cup.line == 2 {
            self.previews[0].size_to(0x1800);
            self.preview_aim(0, 445, 480, 0);
        }
        if self.cup.line == 5 {
            self.track_map.stopped = false;
            self.track_map.scale_to = 3072;
            self.track_map.pos_to = world_at(875, 800, 0);
        }
        self.cup_arrows();
    }

    /// 0x8008f1c4, 0x8008f260: before the first round, the cup before or
    /// after that is open, from its start.
    fn cup_step(&mut self, next: bool) {
        if self.players[0].progress[1] != 0 {
            return;
        }
        self.play(if next { 49 } else { 48 });
        loop {
            let c = self.players[0].progress[0];
            self.players[0].progress[0] = match (next, c) {
                (true, c) if c < 2 => c + 1,
                (true, _) => 0,
                (false, 0) => 2,
                (false, c) => c - 1,
            };
            if self.cup_open(self.players[0].progress[0]) {
                break;
            }
        }
        self.cup_reset();
    }

    /// 0x8009c44c: the round's race: the player on the grid by points, the
    /// computer drivers' cars picked from the player's name (each an open
    /// car not yet taken, the field kept fair), or for the TwinMill Cup the
    /// two TwinMills; six laps; the settings' difficulty.
    fn set_up_cup_race(&mut self) {
        let p0 = self.players[0].clone();
        let file = |c: u8| self.tables.car_files.get(c as usize).cloned().flatten().unwrap_or_default();
        let cup = p0.progress[0];
        let grid = if cup == 2 { 2 } else { self.cup_rank(0) };
        let mut entrants =
            vec![Entrant { name: file(p0.car), driver: Driver::PlayerOne, car_id: p0.car, player: 0, grid }];
        let difficulty;
        if cup == 2 {
            entrants.push(Entrant {
                name: "twinmill".into(),
                driver: Driver::Computer,
                car_id: 38,
                player: 0,
                grid: 0,
            });
            entrants.push(Entrant {
                name: "twinmil2".into(),
                driver: Driver::Computer,
                car_id: 39,
                player: 0,
                grid: 1,
            });
            difficulty = 255;
        } else {
            difficulty = self.settings.difficulty;
            let name = p0.name.as_bytes();
            for attempt in 1..100u32 {
                entrants.truncate(1);
                let mut free = [true; 41];
                free[p0.car as usize % 41] = false;
                for k in 1..6 {
                    let seed = name.get(k - 1).copied().unwrap_or(0) as u32 * attempt;
                    let mut c = ((seed & 255) % 41) as u8 + 1;
                    loop {
                        if c >= 41 {
                            c = 0;
                        }
                        if self.car_open(c, 0) && free[c as usize] {
                            break;
                        }
                        c += 1;
                    }
                    free[c as usize] = false;
                    entrants.push(Entrant {
                        name: file(c),
                        driver: Driver::Computer,
                        car_id: c,
                        player: 0,
                        grid: self.cup_rank(k),
                    });
                }
                if self.field_fair(&entrants) {
                    break;
                }
            }
        }
        let track = self.cup_track().unwrap_or(0);
        let world = (track / 3) as usize;
        let number = track % 3 + 1;
        let cheats = if p0.cheats != 0 { p0.cheats } else { self.players[1].cheats };
        let mut flags = 32
            | ((p0.cheats >> 3) & 16)
            | ((self.players[1].cheats >> 3) & 16)
            | (((p0.cheats & 256 != 0) as u32) << 6)
            | (((self.players[1].cheats & 256 != 0) as u32) << 6);
        if cup == 2 {
            flags |= 256;
        }
        self.race = Some(RaceSetup {
            flags,
            track: self.tables.worlds.get(world).cloned().unwrap_or_default(),
            track_number: number,
            laps: 6,
            checkpoints: self.tables.checkpoints.get(track as usize).copied().unwrap_or(0),
            options: cheats,
            time_limit: 0,
            cars: entrants,
            difficulty,
            best_line: (cup == 2).then(|| "tcup".to_string()),
            names: self.player_names(),
        });
        self.race_end = None;
        self.last_race = self.race.clone();
        self.race_music();
    }

    /// 0x8008e6d4: a round left: the player gets 4 points (7 in the
    /// TwinMill Cup); if the race gave the others none, they share 5, 6, 7,
    /// 8 and 10 at random with no times (in the TwinMill Cup, 8 and 10).
    fn cup_left(&mut self) {
        let cup = self.players[0].progress[0];
        let mut r = self.cup_result.take().unwrap_or_else(|| RaceResult::ended(RaceEnd::Other(0)));
        r.cars.resize(6, CarResult::default());
        let others_scored = r.cars.get(1).is_some_and(|c| c.points != 0);
        r.cars[0].points = if cup == 2 { 7 } else { 4 };
        if !others_scored {
            if cup == 2 {
                let first = self.rand.below(5) != 0;
                r.cars[1] = CarResult { points: if first { 10 } else { 8 }, ..CarResult::default() };
                r.cars[2] = CarResult { points: if first { 8 } else { 10 }, ..CarResult::default() };
            } else {
                let mut taken = [false; 5];
                for k in 1..6 {
                    let i = loop {
                        let i = self.rand.below(5) as usize;
                        if !taken[i] {
                            break i;
                        }
                    };
                    taken[i] = true;
                    r.cars[k] = CarResult { points: LEFT_POINTS[i], ..CarResult::default() };
                }
            }
        }
        self.cup_result = Some(r);
    }

    /// 0x8008f4f8: each driver's points and time added, and the standings:
    /// "place. name" and points, in order (three drivers in the TwinMill
    /// Cup).
    fn cup_standings(&mut self) {
        let r = self.cup_result.clone().unwrap_or_else(|| RaceResult::ended(RaceEnd::Other(0)));
        for k in 0..6 {
            let c = r.cars.get(k).copied().unwrap_or_default();
            self.players[0].records[k] = self.players[0].records[k].wrapping_add(c.points as u32);
            self.cup.times[k] = self.cup.times[k].wrapping_add(c.time);
        }
        let drivers = if self.players[0].progress[0] == 2 { 3 } else { 6 };
        let top = if drivers == 3 { 240 } else { 215 };
        let setup = self.last_race.clone();
        for k in 0..drivers {
            let name = if k == 0 {
                self.players[0].name.clone()
            } else {
                let car = setup.as_ref().and_then(|s| s.cars.get(k)).map_or(0, |e| e.car_id);
                self.tables.car_names.get(car as usize).cloned().unwrap_or_default()
            };
            let place = self.cup_rank(k);
            let y = top + 25 * place as i16;
            self.set_text(
                STANDINGS,
                place as usize,
                &format!("{}. {}", self.cup_ahead(k) + 1, name),
                55,
                y,
                0,
                0,
                false,
                1,
                [255; 3],
            );
            self.set_text(
                STANDINGS,
                place as usize + 6,
                &self.players[0].records[k].to_string(),
                295,
                y,
                0,
                0,
                false,
                1,
                [255; 3],
            );
        }
        for k in drivers..6 {
            let y = top + 25 * k as i16;
            self.set_text(STANDINGS, k, "", 55, y, 0, 0, false, 1, [255; 3]);
            self.set_text(STANDINGS, k + 6, "", 295, y, 0, 0, false, 1, [255; 3]);
        }
        let title = self.strings.get(264).to_string();
        self.set_text(STANDINGS, 12, &title, 200, 175, 0, 0, true, 1, [255; 3]);
    }

    /// The standings, victory and defeat screens drawn.
    fn cup_screen_draw(&mut self, k: usize, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        self.draw_screen(k, None, None, self.fade.hidden);
        self.end_frame();
    }
}
