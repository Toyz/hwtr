//! The garage (states 107, 159 to 183; screens 10 and 11, a panel each):
//! each player steps through the cars with the car shown big; the panel
//! names the car before, this one and the one after, then turns to the
//! car's facts, two lines at a time, every three seconds; a locked car is a
//! spinning question mark. Cross picks the car; Triangle puts it back, or
//! leaves; when every player has picked, back to the main menu.

use super::*;

const PANELS: [usize; 2] = [10, 11];
/// Each panel's text column.
const PANEL_X: [i16; 2] = [194, 444];

/// The garage's state: the cars as it was entered (0x800d274a), who has
/// picked (0x800d274c), whose facts show their second pair (0x800d27b4),
/// whose names want writing (0x800d27b6), when each turned its facts
/// (0x800d27ac), and the question marks' turns (0x800d1260).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Garage {
    cars_before: [u8; 2],
    picked: [bool; 2],
    second_pair: [bool; 2],
    names: [bool; 2],
    turned: [u32; 2],
    mystery: [i32; 2],
}

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8009_0ea4, |f: &mut Front, p: &mut Poster| p.post(if f.people == 1 { 86 } else { 87 }));
    r.add(0x8009_0eec, |f: &mut Front, _: &mut Poster| {
        f.background = Some("psxgar1".to_string());
        f.fade_in();
        f.garage.cars_before = [f.players[0].car, f.players[1].car];
        f.previews[0].size_to(0x1800);
        f.preview_place(0, 375, 425, 0);
        f.preview_aim(0, 375, 425, 0);
        f.garage.picked[0] = false;
        f.garage.second_pair[0] = true;
        f.garage.names[0] = true;
        f.screens[PANELS[0]].enter(&f.font);
    });
    r.add(0x8009_0f88, |f: &mut Front, _: &mut Poster| {
        f.background = Some("psxgar".to_string());
        f.fade_in();
        f.garage.cars_before = [f.players[0].car, f.players[1].car];
        for (k, x) in [(0, 375), (1, 625)] {
            f.previews[k].size_to(0x1800);
            f.preview_place(k, x, 610, 0);
            f.preview_aim(k, x, 610, 0);
        }
        f.garage.picked = [false; 2];
        f.garage.second_pair = [true; 2];
        f.garage.names = [true; 2];
        f.screens[PANELS[0]].enter(&f.font);
        f.screens[PANELS[1]].enter(&f.font);
    });
    r.add(0x8009_0274, |f: &mut Front, _: &mut Poster| f.garage.turned[0] = f.clock);
    r.add(0x8009_02a0, |f: &mut Front, _: &mut Poster| f.garage.turned[1] = f.clock);
    r.add(0x8009_02cc, |f: &mut Front, p: &mut Poster| {
        // Every three seconds, each player's other pair of facts.
        if f.garage.turned[0].wrapping_add(3000) < f.clock {
            p.post(89);
            f.garage.second_pair[0] = !f.garage.second_pair[0];
        }
        if f.people == 2 && f.garage.turned[1].wrapping_add(3000) < f.clock {
            p.post(90);
            f.garage.second_pair[1] = !f.garage.second_pair[1];
        }
    });
    r.add(0x8009_038c, |f: &mut Front, _: &mut Poster| f.garage_facts(0));
    r.add(0x8009_0680, |f: &mut Front, _: &mut Poster| f.garage_facts(1));
    r.add(0x8009_0e64, |f: &mut Front, _: &mut Poster| {
        f.screens[PANELS[0]].lay_out_texts(&f.font);
        f.screens[PANELS[1]].lay_out_texts(&f.font);
    });
    r.add(0x8009_1558, |f: &mut Front, _: &mut Poster| {
        f.garage_names(0);
        f.garage_names(1);
    });
    r.add(0x8009_1518, |f: &mut Front, _: &mut Poster| {
        f.screens[PANELS[0]].update(f.clock, &f.font);
        f.screens[PANELS[1]].update(f.clock, &f.font);
    });
    r.add(0x8009_1cf0, |f: &mut Front, p: &mut Poster| f.garage_draw(p));
    r.add(0x8009_1384, |f: &mut Front, p: &mut Poster| f.garage_pads(p));
    r.add(0x8009_1bb8, |f: &mut Front, p: &mut Poster| {
        // A car not shown yet is asked for (the original waits 1.5 s, for
        // the CD; every model is in memory here).
        for k in 0..f.people.min(2) as usize {
            if !f.garage.picked[k] && !f.car_loaded[k] && f.car_open(f.players[k].car, k) {
                f.car_changed[k] = f.clock;
                p.post(if k == 0 { 50 } else { 51 });
            }
        }
    });
    r.add(0x8009_0df8, |f: &mut Front, p: &mut Poster| {
        // Everyone has picked: event 88.
        let done = if f.people == 1 { f.garage.picked[0] } else { f.garage.picked[0] && f.garage.picked[1] };
        if done {
            p.post(88);
        }
    });
    r.add(0x8009_22ac, |f: &mut Front, p: &mut Poster| {
        // Triangle: the pick put back, or out of the garage (event 91).
        if f.garage.picked[0] {
            f.garage.picked[0] = false;
            f.garage.names[0] = true;
            f.previews[0].size_to(0x1c00);
            f.preview_aim(0, 390, 610, 0);
            p.post(10);
        } else {
            p.post(91);
        }
    });
    r.add(0x8009_2314, |f: &mut Front, p: &mut Poster| {
        if f.garage.picked[1] {
            f.garage.picked[1] = false;
            f.garage.names[1] = true;
            f.previews[1].size_to(0x1c00);
            f.preview_aim(1, 610, 610, 0);
        }
        p.post(10);
    });
    r.add(0x8009_2368, |f: &mut Front, _: &mut Poster| f.garage_pick(0));
    r.add(0x8009_23ec, |f: &mut Front, _: &mut Poster| {
        if f.people == 2 {
            f.garage_pick(1);
        }
    });
    r.add(0x8009_1480, |f: &mut Front, p: &mut Poster| p.post(if f.garage.picked[0] { 93 } else { 92 }));
    r.add(0x8009_14c4, |f: &mut Front, p: &mut Poster| {
        p.post(if f.people == 2 && !f.garage.picked[1] { 54 } else { 55 })
    });
    r.add(0x8009_114c, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[0] {
            let y = if f.people == 1 { 525 } else { 610 };
            f.preview_aim(0, 325, y, 0);
        }
    });
    r.add(0x8009_1198, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[1] {
            f.preview_aim(1, 675, 610, 0);
        }
    });
    r.add(0x8009_2528, |f: &mut Front, _: &mut Poster| f.garage_step(0, false));
    r.add(0x8009_2480, |f: &mut Front, _: &mut Poster| f.garage_step(0, true));
    r.add(0x8009_2568, |f: &mut Front, _: &mut Poster| f.garage_step(1, false));
    r.add(0x8009_24d4, |f: &mut Front, _: &mut Poster| f.garage_step(1, true));
    r.add(0x8009_25a8, |f: &mut Front, _: &mut Poster| {
        // Out without picking: the cars as they were.
        f.players[0].car = f.garage.cars_before[0];
        f.players[1].car = f.garage.cars_before[1];
    });
    r.add(0x8009_25c8, |f: &mut Front, _: &mut Poster| {
        f.preview_aim(0, 500, 500, 0);
        f.previews[0].size_to(0);
        if f.people == 2 {
            f.previews[1].size_to(0);
            f.preview_aim(1, 500, 500, 0);
        }
        f.screens[PANELS[0]].exit();
        f.screens[PANELS[1]].exit();
        f.fade_out();
    });
    r.add(0x8009_2658, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8009_1074, |f: &mut Front, _: &mut Poster| {
        // A car shown: big in the middle for one player, beside the other
        // for two.
        f.preview_init(0);
        if f.people == 1 {
            f.preview_aim(0, 410, 525, 0);
            f.previews[0].size_to(0x2000);
        } else {
            f.preview_place(0, 325, 610, 0);
            f.preview_aim(0, 325, 610, 0);
            f.previews[0].size_to(0x1c00);
        }
    });
    r.add(0x8009_10fc, |f: &mut Front, _: &mut Poster| {
        f.preview_init(1);
        f.preview_place(1, 675, 610, 0);
        f.preview_aim(1, 675, 610, 0);
        f.previews[1].size_to(0x1c00);
    });
    r.add(0x8008_c494, |f: &mut Front, p: &mut Poster| {
        if !f.car_loaded[1] || f.people == 1 {
            p.post(10);
        } else {
            f.preview_full(1);
        }
    });
    r.add(0x8008_c510, |f: &mut Front, p: &mut Poster| {
        f.preview_rest(1);
        p.post(10);
    });
}

impl Front {
    /// 0x80084780: a preview at rest, full size.
    pub(super) fn preview_init(&mut self, k: usize) {
        let p = &mut self.previews[k];
        p.angle = CAR_REST;
        p.angle_to = CAR_REST;
        p.scale = 4096;
        p.size_to(4096);
    }

    /// 0x80092368, 0x800923ec: Cross: a car the player has is picked (the
    /// preview to the side, full size); a locked one buzzes and says so.
    fn garage_pick(&mut self, k: usize) {
        if self.car_open(self.players[k].car, k) {
            self.garage.picked[k] = true;
            self.preview_aim(k, if k == 0 { 325 } else { 675 }, 650, 0);
            self.preview_full(k);
        } else {
            self.play(53);
            self.garage_facts(k);
        }
    }

    /// 0x80092528 and its twins: the car before or after (round the 41),
    /// unless picked.
    fn garage_step(&mut self, k: usize, next: bool) {
        if self.garage.picked[k] {
            return;
        }
        let c = self.players[k].car;
        self.players[k].car = match (next, c) {
            (true, c) if c + 1 < 41 => c + 1,
            (true, _) => 0,
            (false, 0) => 40,
            (false, c) => c - 1,
        };
        self.garage.names[k] = true;
    }

    /// 0x8009038c, 0x80090680: the car's facts (its name and a pair), or
    /// that it is locked.
    fn garage_facts(&mut self, k: usize) {
        if self.garage.picked[k] {
            return;
        }
        let car = self.players[k].car;
        let (screen, x) = (PANELS[k], PANEL_X[k]);
        let lines: [String; 3] = if self.car_open(car, k) {
            let f = self.facts.0.get(car as usize).cloned().unwrap_or_default();
            if self.garage.second_pair[k] {
                [f[0].clone(), f[3].clone(), f[4].clone()]
            } else {
                [f[0].clone(), f[1].clone(), f[2].clone()]
            }
        } else {
            [self.strings.get(97).to_string(), self.strings.get(236).to_string(), self.strings.get(237).to_string()]
        };
        for (i, line) in lines.iter().enumerate() {
            self.set_text(screen, i, line, x, 370 + 25 * i as i16, 0, 0, true, 1, [255; 3]);
        }
    }

    /// 0x80091558: a picked car's name alone; otherwise, when the car has
    /// changed, the cars before and after it dim either side of its name
    /// (a locked car is "Mystery Car").
    fn garage_names(&mut self, k: usize) {
        let (screen, x) = (PANELS[k], PANEL_X[k]);
        if k == 1 && self.people == 1 {
            for i in 0..3 {
                self.set_text(screen, i, "", x, 370 + 25 * i as i16, 0, 0, true, 1, [100; 3]);
            }
            return;
        }
        if self.garage.picked[k] {
            let name = self.facts.0.get(self.players[k].car as usize).map(|f| f[0].clone()).unwrap_or_default();
            self.set_text(screen, 0, "", x, 370, 0, 0, true, 1, [100; 3]);
            self.set_text(screen, 1, &name, x, 395, 0, 0, true, 1, [255; 3]);
            self.set_text(screen, 2, "", x, 420, 0, 0, true, 1, [100; 3]);
            return;
        }
        if !self.garage.names[k] {
            return;
        }
        self.garage.names[k] = false;
        let c = self.players[k].car;
        let around = [if c == 0 { 40 } else { c - 1 }, c, if c + 1 < 41 { c + 1 } else { 0 }];
        for (i, car) in around.into_iter().enumerate() {
            let name = if self.car_open(car, k) {
                self.tables.car_names.get(car as usize).cloned().unwrap_or_default()
            } else {
                self.strings.get(97).to_string()
            };
            let shade = if i == 1 { 255 } else { 100 };
            self.set_text(screen, i, &name, x, 370 + 25 * i as i16, 0, 0, true, 1, [shade; 3]);
        }
    }

    /// 0x80091384: the pads, both in turn with two players: Cross to pick
    /// (once), Triangle; with one, Start too; up and down held step the car.
    fn garage_pads(&mut self, p: &mut Poster) {
        if self.car_asked[0] && !self.car_loaded[0] {
            return;
        }
        if self.people == 2 {
            let first = if self.garage.picked[0] { 32 } else { 48 };
            let second = if self.garage.picked[1] { 32 } else { 16 };
            if !self.second_first {
                self.garage_pad(0, first, p);
                self.garage_pad(1, second, p);
                self.second_first = true;
            } else {
                self.garage_pad(1, second, p);
                self.garage_pad(0, first, p);
                self.second_first = false;
            }
        } else if !self.garage.picked[0] {
            self.garage_pad(0, 112, p);
        }
    }

    /// 0x800911cc, 0x800912a0.
    fn garage_pad(&mut self, pad: usize, mask: u16, p: &mut Poster) {
        self.pad_events(pad, mask, p);
        let events = if pad == 0 { (24, 25) } else { (27, 28) };
        self.held_repeat(pad, (14, events.0), (15, events.1), p);
    }

    /// 0x80091cf0: the panels, each player's car (or question mark), and
    /// the bars of the cars' qualities.
    fn garage_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        self.draw_screen(PANELS[0], None, None, self.fade.hidden);
        self.draw_screen(PANELS[1], None, None, self.fade.hidden);
        for k in 0..self.people.min(2) as usize {
            if !self.car_open(self.players[k].car, k) {
                if self.fade.hidden {
                    continue;
                }
                // 0x80091cf0: a question mark turning a turn every five
                // seconds, twice size, tilted back.
                let s = self.seconds(6 + k);
                let m = &mut self.garage.mystery[k];
                *m = m.wrapping_add(fx(s, TRACK_SPIN));
                while *m > TWO_PI {
                    *m -= TWO_PI;
                }
                let (x, y) = match (k, self.people) {
                    (0, 1) => (310, 450),
                    (0, _) => (310, 625),
                    _ => (700, 625),
                };
                let at = world_at(x, y, 0);
                self.drawing_pieces.push(PieceDraw {
                    model: "mystery".to_string(),
                    pos: [at[0], at[1], 0],
                    turn: [((-0x5_0000i64 * 71) >> 12) as i32, 0, self.garage.mystery[k]],
                    scale: 0x2000,
                });
            } else if self.car_loaded[k] {
                self.draw_preview(k);
            } else if self.car_asked[k] {
                self.car_loaded[k] = true;
                self.car_shown[k] = false;
                self.car_asked[k] = false;
                if self.people == 2 {
                    self.preview_aim(k, if k == 0 { 375 } else { 625 }, 610, 0);
                }
                self.car_arrived(k);
            }
            // The picture only where neither the mystery nor the model was
            // drawn.
            if self.car_open(self.players[k].car, k) && !self.car_loaded[k] {
                let at = match (k, self.people) {
                    (0, 1) => (95, 85),
                    (0, _) => (95, 120),
                    _ => (350, 120),
                };
                self.draw_decal(k, at.0, at.1);
            }
        }
        if !self.fade.hidden {
            self.garage_bars();
        }
        self.end_frame();
    }

    /// 0x80090c9c, 0x80090974: each car's three bars (top speed, stunts,
    /// durability: a block per five points, red, green, blue) and its
    /// class (green, yellow, red or pink, a dozen blocks a class).
    fn garage_bars(&mut self) {
        let columns: Vec<(usize, i32)> = if self.people == 1 { vec![(0, 350)] } else { vec![(0, 99), (1, 369)] };
        for (k, x) in columns {
            let [a, b, c, class] = self.car_stats[k];
            if class < 0 {
                continue;
            }
            for (v, y, colour) in [(a, 104, [240, 0, 20]), (b, 131, [0, 240, 20]), (c, 158, [20, 0, 240])] {
                for i in 0..(v as i32 / 5 + 1) {
                    self.drawing.push(screen::Glyph::new(153, x + 3 * i, y, colour));
                }
            }
            let colour = match class {
                0 => [78, 176, 83],
                1 => [246, 246, 36],
                2 => [240, 40, 40],
                3 => [240, 70, 180],
                _ => [127; 3],
            };
            for i in 0..(class as i32 + 1) * 12 {
                self.drawing.push(screen::Glyph::new(153, x + 3 * i, 185, colour));
            }
        }
    }
}
