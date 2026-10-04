//! The options screen (states 108, 184 to 195; screen 12): Difficulty,
//! Music and Sound FX as sliders, Audio Mode (mono or stereo), Controls,
//! Hi-Scores, Credits, and the Boom Box (the race's song: random, the
//! track's own, or one chosen, which then plays here).

use super::*;

/// The options screen.
pub(super) const OPTIONS: usize = 12;
/// Its help line.
const OPTIONS_HELP: usize = 1;

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8009_2b98, |f: &mut Front, _: &mut Poster| f.options_enter());
    r.add(0x8009_3418, |f: &mut Front, _: &mut Poster| f.options_sliders());
    r.add(0x8009_2d28, |f: &mut Front, _: &mut Poster| f.screens[OPTIONS].lay_out_texts(&f.font));
    r.add(0x8009_2c70, |f: &mut Front, _: &mut Poster| f.screens[OPTIONS].update(f.clock, &f.font));
    r.add(0x8009_2c98, |f: &mut Front, p: &mut Poster| f.options_draw(p));
    r.add(0x8009_2d50, |f: &mut Front, p: &mut Poster| f.options_held(p));
    r.add(0x8009_2be8, |f: &mut Front, p: &mut Poster| {
        // 0x80092be8: the presses each line takes: up, down, Triangle and
        // Start always; left or right to turn the audio mode the other
        // way; Cross on Controls, Hi-Scores and Credits; left and right on
        // the Boom Box's lines.
        let extra = match f.option_line {
            3 if f.settings.other[1] == 0 => 8,
            3 => 4,
            4..=6 => 16,
            7 | 8 => 12,
            _ => 0,
        };
        f.pad_events(0, 99 | extra, p);
    });
    r.add(0x8009_32d8, |f: &mut Front, _: &mut Poster| f.option_step(false));
    r.add(0x8009_3368, |f: &mut Front, _: &mut Poster| f.option_step(true));
    r.add(0x8009_2fd0, |f: &mut Front, _: &mut Poster| f.option_slide(false));
    r.add(0x8009_311c, |f: &mut Front, _: &mut Poster| f.option_slide(true));
    r.add(0x8009_2f50, |f: &mut Front, _: &mut Poster| {
        // 0x80092f50: letting go of the effects slider plays one at its
        // new volume.
        if f.option_line == 2 {
            // 0x80015908: 4096ths of full, to 127ths.
            let v = ((f.settings.volume as i32) << 12) / 255;
            f.sounds.push((56, ((v * 127) >> 12) as u8));
        }
    });
    r.add(0x8009_2de4, |f: &mut Front, _: &mut Poster| f.option_turn(false));
    r.add(0x8009_2e94, |f: &mut Front, _: &mut Poster| f.option_turn(true));
    r.add(0x8009_33e8, |f: &mut Front, _: &mut Poster| {
        // 0x800933e8: leaving from the song line stops the song.
        if f.option_line == 8 {
            f.song_preview_off();
        }
        f.option_line = 0;
    });
    r.add(0x8009_3624, |f: &mut Front, _: &mut Poster| {
        // 0x80093624: the screen off and the picture out.
        f.screens[OPTIONS].exit();
        f.fade_out();
    });
    r.add(0x8009_3654, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8009_35b0, |f: &mut Front, p: &mut Poster| {
        // 0x800935b0: Cross on a line that opens a screen.
        p.post(match f.option_line {
            4 => 96,
            5 => 98,
            6 => 97,
            _ => 10,
        });
    });
}

impl Front {
    /// 0x80092b98.
    fn options_enter(&mut self) {
        self.set_background("psxopt");
        self.fade_in();
        self.screens[OPTIONS].enter(&self.font);
        self.options_texts();
        self.helps[OPTIONS_HELP].x = self.helps[OPTIONS_HELP].start << 12;
    }

    /// 0x8009292c: the Boom Box's lines: the song's artist and title, the
    /// mode, and the "Song" line when a song is chosen.
    fn options_texts(&mut self) {
        let (artist, title) = self.songs.get(self.song as usize).cloned().unwrap_or_default();
        self.set_text(OPTIONS, 1, &artist, 449, 356, 0, 0, true, 1, [255; 3]);
        self.set_text(OPTIONS, 2, &title, 449, 386, 0, 0, true, 0, [255; 3]);
        let (label, mode) = match self.music_mode {
            0 => (String::new(), self.strings.get(255).to_string()),
            1 => (String::new(), self.strings.get(256).to_string()),
            _ => (self.strings.get(98).to_string(), self.strings.get(257).to_string()),
        };
        self.set_text(OPTIONS, 0, &label, 40, 394, 0, 1, false, 1, [127; 3]);
        self.set_text(OPTIONS, 3, &mode, 449, 314, 0, 0, true, 0, [255; 3]);
        self.option_arrows();
    }

    /// 0x800927cc: the arrows either side of the chosen line (none on
    /// Controls, Hi-Scores and Credits).
    fn option_arrows(&mut self) {
        let at = match self.option_line {
            0 => Some((40, 310, 270)),
            1 => Some((40, 310, 340)),
            2 => Some((40, 310, 410)),
            3 => Some((40, 310, 480)),
            7 => Some((30, 320, 755)),
            8 => Some((40, 310, 840)),
            _ => None,
        };
        match at {
            Some((left, right, y)) => {
                self.place_piece(OPTIONS, "arrowlt", left, y, 100);
                self.place_piece(OPTIONS, "arrowrt", right, y, 100);
            }
            None => {
                self.place_piece(OPTIONS, "arrowlt", -1, -1, 0);
                self.place_piece(OPTIONS, "arrowrt", -1, -1, 0);
            }
        }
    }

    /// 0x80093418: the sliders' knobs to their settings (0 to 255 across
    /// 485 to 915), the audio mode's to mono or stereo.
    fn options_sliders(&mut self) {
        let knob = |v: u8| screen::world_x((fx(0x1afa, (v as i32) << 12) >> 12) + 485);
        let s = self.settings;
        let at = [
            (knob(s.difficulty), 285),
            (knob(s.music), 395),
            (knob(s.volume), 470),
            (screen::world_x(if s.other[1] != 0 { 915 } else { 485 }), 565),
        ];
        for (p, (x, y)) in self.screens[OPTIONS].pieces.iter_mut().zip(at) {
            p.to[0] = x;
            p.to[1] = screen::world_y(y);
        }
    }

    /// 0x80092c98.
    fn options_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        let text = (self.option_line == 8).then_some(0);
        self.draw_screen(OPTIONS, text, Some(self.option_line as usize), self.fade.hidden);
        if !self.fade.hidden {
            self.draw_help(OPTIONS_HELP);
        }
        self.end_frame();
    }

    /// 0x80092d50: on a slider, left or right held (events 94, 95), else
    /// event 10.
    fn options_held(&mut self, p: &mut Poster) {
        let holds = |f: &Front, action: u8| f.pads[0].holds(&f.mappings[0], action);
        if self.option_line < 3 && holds(self, 16) {
            p.post(94);
        } else if self.option_line < 3 && holds(self, 17) {
            p.post(95);
        } else {
            p.post(10);
        }
    }

    /// 0x800932d8, 0x80093368: the line above or below (the song line only
    /// with a song chosen); onto the song line its song plays, off it the
    /// menu's music comes back.
    fn option_step(&mut self, down: bool) {
        let lines = if self.music_mode == 2 { 9 } else { 8 };
        if self.option_line == 8 {
            self.song_preview_off();
        }
        self.option_line = match (down, self.option_line) {
            (false, 0) => lines - 1,
            (false, l) => l - 1,
            (true, l) if l + 1 < lines => l + 1,
            (true, _) => 0,
        };
        if self.option_line == 8 {
            self.song_preview_on();
        }
        self.option_arrows();
    }

    /// 0x80093274: the menu's music off, the chosen song on.
    fn song_preview_on(&mut self) {
        self.music_started = false;
        self.music = Some(false);
        self.cd.push(crate::cd::CdAsk::Play(self.song));
    }

    /// 0x800932a8: the song stopped, the menu's music back.
    fn song_preview_off(&mut self) {
        self.cd.push(crate::cd::CdAsk::Stop);
        self.start_music();
    }

    /// 0x80092fd0, 0x8009311c: a slider 10 down or up a frame (to 0 or
    /// 255), with a click; the music's volume set as it goes.
    fn option_slide(&mut self, up: bool) {
        let line = self.option_line;
        let s = &mut self.settings;
        let v = match line {
            0 => &mut s.difficulty,
            1 => &mut s.music,
            2 => &mut s.volume,
            _ => return,
        };
        let moved = if up {
            if *v < 245 {
                *v += 10;
                true
            } else {
                *v = 255;
                false
            }
        } else if *v >= 11 {
            *v -= 10;
            true
        } else {
            *v = 0;
            false
        };
        if moved {
            self.play(if up { 49 } else { 48 });
        }
        if line == 1 {
            self.cd.push(crate::cd::CdAsk::Volume(self.settings.music));
        }
    }

    /// 0x80092de4, 0x80092e94: left or right on the audio mode, the Boom
    /// Box's mode (it goes round), or the song (round, playing it).
    fn option_turn(&mut self, right: bool) {
        match self.option_line {
            3 => self.settings.other[1] = right as u8,
            7 => {
                self.music_mode = match (right, self.music_mode) {
                    (false, 0) => 2,
                    (false, m) => m - 1,
                    (true, m) if m < 2 => m + 1,
                    (true, _) => 0,
                };
                self.options_texts();
            }
            8 => {
                self.song = match (right, self.song) {
                    (false, 0) => crate::cd::SONGS - 1,
                    (false, s) => s - 1,
                    (true, s) if s < crate::cd::SONGS - 1 => s + 1,
                    (true, _) => 0,
                };
                self.cd.push(crate::cd::CdAsk::Play(self.song));
                self.options_texts();
            }
            _ => {}
        }
    }
}
