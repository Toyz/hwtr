//! Off to a race and back (states 94 to 112, 329 to 340): the exhibition
//! against the computer cars, practice and the airtime challenge with the
//! players alone.

use super::*;

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8008_db84, |f: &mut Front, _: &mut Poster| {
        f.fade_out();
        f.screens[MAIN_MENU].exit();
        f.track_map.scale_to = 0;
        f.track_map.stopped = false;
        f.track_map.pos_to = world_at(500, 500, 0);
    });
    r.add(0x8008_ba94, |f: &mut Front, _: &mut Poster| {
        f.play(54);
        f.wait_from = f.clock;
    });
    r.add(0x8008_baec, |f: &mut Front, p: &mut Poster| {
        if f.wait_from.wrapping_add(1800) < f.clock {
            p.post(73);
        }
    });
    r.add(0x8008_dc04, |_: &mut Front, _: &mut Poster| {});
    // settings kept, the front end let go
    // 0x8008ab00: settings kept, the front end let go, and the music
    // stopped (0x8008a6a0, 0x800181ac).
    r.add(0x8008_ab00, |f: &mut Front, _: &mut Poster| {
        f.music_started = false;
        f.music = Some(false);
    });
    r.add(0x8009_ba28, |f: &mut Front, _: &mut Poster| f.set_up_race());
    r.add(0x8009_b254, |f: &mut Front, _: &mut Poster| f.set_up_attract());
    // 0x8009b1e0: any button on either pad during the attract race (event
    // 135, to the main menu).
    r.add(0x8009_b1e0, |f: &mut Front, p: &mut Poster| {
        if (0..2).any(|k| f.pads[k].holds(&f.mappings[k], 27)) {
            // The port loads the menus at once where the original reads
            // the CD for a while: the buttons held now count as already
            // pressed, so they do not choose on the main menu too.
            for k in 0..2 {
                for (i, held) in f.held[k].iter_mut().enumerate() {
                    *held |= f.pads[k].holds(&f.mappings[k], 14 + i as u8);
                }
            }
            p.post(135);
        }
    });
    // 0x8009b578: the attract race's frame (iface_game+0x18, which the
    // host runs: the frame of this blank is the race's); over, event 10.
    r.add(0x8009_b578, |f: &mut Front, p: &mut Poster| {
        f.frame_done = true;
        if f.race.is_none() && f.race_end.is_some() {
            p.post(10);
        }
    });
    // 0x8009b510: the attract race over or cut short: its song stopped,
    // its result taken and dropped, the race let go if it still runs.
    r.add(0x8009_b510, |f: &mut Front, _: &mut Poster| {
        f.cd.push(crate::cd::CdAsk::Stop);
        f.race_end = None;
        f.race_dropped = true;
    });
    // 0x8009b614: practice, half an hour on the clock; practice airtime
    // counts the stunts.
    r.add(0x8009_b614, |f: &mut Front, _: &mut Poster| {
        let flags = if f.settings.mode == 0 { 12 } else { 6 };
        f.set_up_alone(flags, 1_800_000);
    });
    // 0x8009c058: the airtime challenge, three minutes of stunts.
    r.add(0x8009_c058, |f: &mut Front, _: &mut Poster| f.set_up_alone(6, 180_000));
    // 0x8009c018, 0x8009b9e8, 0x8009c3d8: the race over, event 10.
    for at in [0x8009_c018, 0x8009_b9e8, 0x8009_c3d8] {
        r.add(at, |f: &mut Front, p: &mut Poster| {
            if f.race.is_none() && f.race_end.is_some() {
                p.post(10);
            }
        });
    }
    // 0x8009bf34, 0x8009b904, 0x8009c2f4: back from the race, by how it
    // ended (event 136 finished, 137 restart, 138, else 134), the cheats
    // dropped, and its result taken.
    for at in [0x8009_bf34, 0x8009_b904, 0x8009_c2f4] {
        r.add(at, race_back);
    }
    r.add(0x8008_abac, |f: &mut Front, _: &mut Poster| {
        // The font is loaded again, and its kerning scaled again; the
        // music comes back (0x8008a668).
        f.font.prepare();
        f.copy_all_letters();
        f.start_music();
    });
    // 0x80099c7c: anything new unlocked is announced (event 84).
    r.add(0x8009_9c7c, |f: &mut Front, p: &mut Poster| p.post(if f.unlocked.any() { 84 } else { 85 }));
}

fn race_back(f: &mut Front, p: &mut Poster) {
    // The race's song stopped (0x8008a7e0).
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
}

impl Front {
    /// 0x8009b254: the attract race: a track at random that either player
    /// has (not a Volcano one), six computer cars at random from those
    /// player one has until the field is fair (at most 100 draws), one lap
    /// against a minute's clock (flags 1, the demo, and 4), at the
    /// game's starting difficulty. Its checkpoints are read by the track's
    /// number alone, as if every track were the Desert's (0x8009b380).
    fn set_up_attract(&mut self) {
        // 0x800836e8 with no name: no picture behind (0x80015648 clears
        // 0x800d2401).
        self.background = None;
        let (world, number) = loop {
            let world = self.rand.below(4) as u8;
            let number = self.rand.below(3) as u8 + 1;
            let t = world * 3 + number - 1;
            if self.track_open(t)
                && self.tables.track_files.get(t as usize).is_some_and(Option::is_some)
                && !(9..12).contains(&t)
            {
                break (world, number);
            }
        };
        let file = |c: u8| self.tables.car_files.get(c as usize).cloned().flatten().unwrap_or_default();
        let mut cars = Vec::new();
        for _ in 0..100 {
            let mut free = [true; 41];
            cars.clear();
            for k in 0..6u8 {
                let mut c = self.rand.below(41) + 1;
                loop {
                    if c >= 41 {
                        c = 0;
                    }
                    if self.car_open(c as u8, 0) && free[c as usize] {
                        break;
                    }
                    c += 1;
                }
                free[c as usize] = false;
                // The original leaves the car's number as the last race
                // had it; the port gives the car's own.
                cars.push(Entrant {
                    name: file(c as u8),
                    driver: Driver::Computer,
                    car_id: c as u8,
                    player: 0,
                    grid: k,
                });
            }
            if self.field_fair(&cars) {
                break;
            }
        }
        self.race = Some(RaceSetup {
            flags: 5,
            track: self.tables.worlds.get(world as usize).cloned().unwrap_or_default(),
            track_number: number,
            laps: 1,
            checkpoints: self.tables.checkpoints.get(number as usize - 1).copied().unwrap_or(0),
            options: 0,
            time_limit: 60_000,
            cars,
            difficulty: self.default_difficulty,
            best_line: None,
            names: self.player_names(),
        });
        self.race_end = None;
        self.race_dropped = false;
        self.race_music();
    }

    /// 0x8009b5b8: the players' names, for the race's results.
    pub(super) fn player_names(&self) -> [String; 2] {
        [self.players[0].name.clone(), self.players[1].name.clone()]
    }

    /// 0x8009b614, 0x8009c058: a race of two laps for the players alone,
    /// on player one's track, against `time_limit` ms, with the cheats'
    /// flags (0x8009b5f8, 0x8009b604) and options as for any race.
    fn set_up_alone(&mut self, flags: u32, time_limit: u32) {
        let p0 = self.players[0].clone();
        let p1 = self.players[1].clone();
        let world = (p0.track / 3) as usize;
        let number = p0.track % 3 + 1;
        let file = |c: u8| self.tables.car_files.get(c as usize).cloned().flatten().unwrap_or_default();
        let mut cars =
            vec![Entrant { name: file(p0.car), driver: Driver::PlayerOne, car_id: p0.car, player: 0, grid: 0 }];
        if self.people == 2 {
            cars.push(Entrant { name: file(p1.car), driver: Driver::PlayerOne, car_id: p1.car, player: 1, grid: 1 });
        }
        let cheat_flags = |c: u32| ((c >> 3) & 16) | (((c & 256 != 0) as u32) << 6);
        self.race = Some(RaceSetup {
            flags: flags | cheat_flags(p0.cheats) | cheat_flags(p1.cheats),
            track: self.tables.worlds.get(world).cloned().unwrap_or_default(),
            track_number: number,
            laps: 2,
            checkpoints: self.tables.checkpoints.get(world * 3 + number as usize - 1).copied().unwrap_or(0),
            options: if p0.cheats != 0 { p0.cheats } else { p1.cheats },
            time_limit,
            cars,
            difficulty: self.settings.difficulty,
            best_line: None,
            names: self.player_names(),
        });
        self.race_end = None;
        self.last_race = self.race.clone();
        self.race_music();
    }
}
