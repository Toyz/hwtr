//! The unlocks announced (states 156, 344 to 348; screen 29, `psxunlk`):
//! each new car (shown turning), track (its map) or cup in turn, under
//! "<player> has unlocked"; five seconds, then Start for the next. And the
//! cup's prize (0x80099ab8): the winner's record, and what each cup opens.

use super::*;

const UNLOCKS: usize = 29;

/// What is being announced (0x800d27e9 to 0x800d27ed): a car or track of
/// which player, or a cup, and when it came up (0x800d27cc).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Announce {
    car: Option<(usize, u8)>,
    track: Option<(usize, u8)>,
    cup: Option<u8>,
    from: u32,
}

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8009_9d88, |f: &mut Front, _: &mut Poster| {
        f.screens[UNLOCKS].enter(&f.font);
        f.set_background("psxunlk");
        f.fade_in();
    });
    r.add(0x8009_a170, |f: &mut Front, p: &mut Poster| f.announce_next(p));
    r.add(0x8009_a824, |f: &mut Front, _: &mut Poster| f.screens[UNLOCKS].reenter_texts(&f.font));
    r.add(0x8009_a84c, |f: &mut Front, _: &mut Poster| f.screens[UNLOCKS].reenter_texts(&f.font));
    r.add(0x8009_a874, |f: &mut Front, _: &mut Poster| f.screens[UNLOCKS].update(f.clock, &f.font));
    r.add(0x8009_a89c, |f: &mut Front, p: &mut Poster| f.announce_draw(p));
    r.add(0x8009_9e10, |f: &mut Front, p: &mut Poster| {
        // A car to show is asked for (after two seconds in the original,
        // for the CD; at once here).
        if f.announce.car.is_some() && !f.car_loaded[0] && !f.car_asked[0] {
            f.car_changed[0] = f.clock;
            p.post(50);
        }
    });
    r.add(0x8009_9dc4, |f: &mut Front, p: &mut Poster| {
        if f.announce.from.wrapping_add(5000) < f.clock {
            p.post(140);
        }
    });
    r.add(0x8009_9eb8, |f: &mut Front, _: &mut Poster| {
        // 0x80099eb8: the car's model into preview 0.
        if let Some((_, car)) = f.announce.car {
            f.preview_car[0] = Some(car);
        }
        f.car_loaded[0] = false;
        f.car_asked[0] = true;
    });
    r.add(0x8009_a7c4, |f: &mut Front, _: &mut Poster| {
        let start = f.strings.get(96).to_string();
        f.set_text(UNLOCKS, 3, &start, 320, 434, 0, 0, true, 1, [255; 3]);
    });
    r.add(0x8009_aa28, |f: &mut Front, p: &mut Poster| {
        // Start from the player whose unlock it is.
        let second = matches!(f.announce.car.or(f.announce.track), Some((1, _)));
        f.pad_events(if second { 1 } else { 0 }, 64, p);
    });
    r.add(0x8009_9f84, |f: &mut Front, _: &mut Poster| {
        // 0x80099f84: the car or map put away.
        if f.announce.car.is_some() {
            f.car_loaded[0] = false;
            f.car_asked[0] = false;
            f.preview_car[0] = None;
        } else if f.announce.track.is_some() {
            f.track_model = None;
        }
    });
    r.add(0x8009_aaa4, |f: &mut Front, _: &mut Poster| {
        f.screens[UNLOCKS].exit();
        f.fade_out();
    });
    r.add(0x8009_aad4, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8009_9ab8, |f: &mut Front, _: &mut Poster| f.cup_prize());
}

impl Front {
    /// 0x8009a170: the next unlock (the first player's cars, then tracks,
    /// then the second player's, then a cup) with its lines, event 139; none
    /// left, event 10.
    fn announce_next(&mut self, p: &mut Poster) {
        self.announce = Announce { from: self.clock, ..Default::default() };
        let has = self.strings.get(239).to_string();
        let mut lines: Option<([String; 3], i16)> = None;
        for k in 0..2 {
            let name = self.players[k].name.clone();
            let car = results::take_lowest(&mut self.unlocked.cars[k][0])
                .or_else(|| results::take_lowest(&mut self.unlocked.cars[k][1]).map(|c| c + 32));
            if let Some(c) = car {
                // 0x8009a05c: its picture into slot 0, the model to come.
                self.announce.car = Some((k, c));
                self.decal_car[0] = Some(c);
                self.car_shown[0] = true;
                self.car_loaded[0] = false;
                self.car_asked[0] = false;
                let car_name = self.tables.car_names.get(c as usize).cloned().unwrap_or_default();
                lines = Some(([name, has.clone(), car_name], 300));
                break;
            }
            if let Some(t) = results::take_lowest(&mut self.unlocked.tracks[k]) {
                self.announce.track = Some((k, t));
                // 0x8009a0ac: the track's map in the middle.
                self.track_model = self.tables.track_files.get(t as usize).cloned().flatten();
                let m = &mut self.track_map;
                m.scale = 0;
                m.stopped = false;
                m.scale_to = 3072;
                m.pos = world_at(500, 500, 0);
                m.pos_to = world_at(500, 425, 0);
                let track_name = self.tables.track_names.get(t as usize).cloned().unwrap_or_default();
                lines = Some(([name, has.clone(), track_name], 300));
                break;
            }
        }
        if lines.is_none() && self.unlocked.cup != 0 {
            let cup = std::mem::take(&mut self.unlocked.cup);
            self.announce.cup = Some(cup);
            let cup_name = self.strings.get(59 + cup as usize).to_string();
            lines = Some(([self.players[0].name.clone(), has, cup_name], 210));
        }
        if let Some((text, y)) = &lines {
            for (i, line) in text.iter().enumerate() {
                self.set_text(UNLOCKS, i, line, 320, y + 30 * i as i16, 0, 2, true, 1, [255; 3]);
            }
            p.post(139);
        }
        self.set_text(UNLOCKS, 3, "", 320, 450, 0, 0, true, 1, [255; 3]);
        p.post(10);
    }

    /// 0x8009a89c: the screen, and the car turning (loaded as asked, then
    /// big in the middle) or the track's map.
    fn announce_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        self.draw_screen(UNLOCKS, None, None, self.fade.hidden);
        if self.announce.car.is_some() {
            if self.car_loaded[0] {
                self.draw_preview(0);
            } else if self.car_asked[0] {
                self.car_loaded[0] = true;
                self.car_shown[0] = false;
                self.car_asked[0] = false;
                self.preview_aim(0, 500, 520, 0);
                self.previews[0].size_to(0x2000);
            }
            self.draw_decal(0, 215, 77);
        } else if self.announce.track.is_some() {
            self.draw_track_map();
        }
        self.end_frame();
    }

    /// 0x80099ab8: a cup won: the winner's points into the cup's record,
    /// and its prize: the Hot Wheels Cup opens the Secret Car Cup (tracks 7
    /// to 9, cars 0, 7, 21 and 33); the Secret Car Cup the TwinMill Cup
    /// (cars 38 and 39, tracks 10 and 11); the TwinMill Cup every car.
    fn cup_prize(&mut self) {
        if !self.cup_won {
            return;
        }
        self.cup_won = false;
        let cup = self.players[0].progress[0];
        let (name, points) = (self.players[0].name.clone(), self.players[0].records[0]);
        let empty = self.strings.get(84).to_string();
        results::insert(&mut self.card.cup_winners, cup as usize, &name, points, |new, old| new > old, &empty);
        let p = self.players[0].clone();
        let (open1, open2) = (self.cup_open(1), self.cup_open(2));
        let u = &mut self.unlocked;
        match cup {
            0 => {
                if !open1 {
                    u.cup = 1;
                }
                u.cars[0][0] |= !p.cars[0] & 0x20_0081;
                u.cars[0][1] |= !p.cars[1] & 2;
                u.tracks[0] |= !p.tracks & 0x380;
                let pl = &mut self.players[0];
                pl.tracks |= 0x380;
                pl.cars[1] |= 2;
                pl.cars[0] |= 0x20_0081;
            }
            1 => {
                if !open2 {
                    u.cup = 2;
                }
                u.cars[0][1] |= !p.cars[1] & 0xc0;
                u.tracks[0] |= !p.tracks & 0xc00;
                let pl = &mut self.players[0];
                pl.cars[1] |= 0xc0;
                pl.tracks |= 0xc00;
            }
            2 => {
                u.cars[0][0] |= !p.cars[0];
                u.cars[0][1] |= !p.cars[1] & 0x1df;
                let pl = &mut self.players[0];
                pl.cars[0] = u32::MAX;
                pl.cars[1] |= 0x1df;
            }
            _ => {}
        }
    }
}
