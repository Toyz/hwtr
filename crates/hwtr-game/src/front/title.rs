//! The title (states 44 to 47), the way back from it or a race (states 49 to 61), and the save prompt.

use super::*;

pub(super) fn register(r: &mut Registry<Front>) {
    // 0x8008a668: the music starts (0x80018118), once.
    r.add(0x8008_a668, |f: &mut Front, _: &mut Poster| f.start_music());
    r.add(0x8008_af68, |f: &mut Front, _: &mut Poster| {
        f.set_background("psxspls");
        f.fade_in();
        f.screens[TITLE].enter(&f.font);
        f.title_sound = false;
        f.set_text(TITLE, 0, "", 50, 30, 0, 0, false, 0, [255; 3]);
    });
    r.add(0x8008_aff8, |f: &mut Front, _: &mut Poster| f.screens[TITLE].lay_out_texts(&f.font));
    r.add(0x8008_b0d4, |f: &mut Front, _: &mut Poster| f.screens[TITLE].update(f.clock, &f.font));
    r.add(0x8008_b020, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        if !f.title_sound {
            f.title_sound = true;
            f.play(55);
        }
        f.draw_screen(TITLE, None, None, f.fade.hidden);
        f.end_frame();
    });
    r.add(0x8008_b0b4, |f: &mut Front, p: &mut Poster| {
        f.pad_events(0, 64, p);
    });
    r.add(0x8008_b0fc, |f: &mut Front, _: &mut Poster| f.fade_out());
    r.add(0x8008_b11c, |f: &mut Front, p: &mut Poster| f.check_passwords(p));
    // 0x8008b2d4: a player's password to show (event 39, from now), or
    // none (40).
    r.add(0x8008_b2d4, |f: &mut Front, p: &mut Poster| {
        if f.saving.iter().any(|&s| s) {
            f.password_from = f.clock;
            p.post(39);
        } else {
            p.post(40);
        }
    });
    // The password shown (state 52, screen 2): "Valid Memory Card Not
    // Found. Password for <name>:" and its twenty letters over two lines;
    // after five seconds, that player's Cross or Start.
    r.add(0x8008_b33c, |f: &mut Front, _: &mut Poster| f.password_texts());
    r.add(0x8008_b5a4, |f: &mut Front, _: &mut Poster| f.screens[PASSWORD].reenter_texts(&f.font));
    r.add(0x8008_b5cc, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        f.draw_screen(PASSWORD, None, None, f.fade.hidden);
        f.end_frame();
    });
    r.add(0x8008_b62c, |f: &mut Front, p: &mut Poster| {
        if f.password_from.wrapping_add(5000) < f.clock {
            f.pad_events(f.password_of as usize, 80, p);
        }
    });
    r.add(0x8008_b6a4, |f: &mut Front, _: &mut Poster| f.save_prompt_texts());
    r.add(0x8008_b868, |f: &mut Front, _: &mut Poster| f.screens[SAVE_PROMPT].reenter_texts(&f.font));
    r.add(0x8008_b890, |f: &mut Front, _: &mut Poster| {
        f.begin_frame();
        let choice = Some(f.save_choice as usize);
        f.draw_screen(SAVE_PROMPT, choice, None, false);
        f.end_frame();
    });
    r.add(0x8008_b8e4, |f: &mut Front, p: &mut Poster| {
        let mask = if f.save_choice == 4 { 82 } else { 81 };
        let _ = f.pad_events(0, mask, p) || f.pad_events(1, mask, p);
    });
    r.add(0x8008_b918, |f: &mut Front, _: &mut Poster| f.save_choice = 4);
    r.add(0x8008_b928, |f: &mut Front, _: &mut Poster| f.save_choice = 5);
    // 0x8008b938: No on to the menus (event 42), Yes to Load/Save (43).
    r.add(0x8008_b938, |f: &mut Front, p: &mut Poster| p.post(if f.save_choice == 4 { 42 } else { 43 }));
    r.add(0x8008_df30, |f: &mut Front, p: &mut Poster| p.post(if f.title_to_cup { 44 } else { 45 }));
}

impl Front {
    /// 0x8008b33c: the next player whose password is to be shown (player
    /// one first), as the screen's six lines; no picture behind it.
    fn password_texts(&mut self) {
        self.screens[PASSWORD].enter(&self.font);
        self.background = None;
        self.fade_in();
        let k = if self.saving[0] { 0 } else { 1 };
        self.saving[k] = false;
        self.password_of = k as u8;
        let who = format!("{}:", self.players[k].name);
        let word = self.passwords.make(&self.players[k]);
        let (first, second): (String, String) = (word.chars().take(10).collect(), word.chars().skip(10).take(10).collect());
        let lines = ["Valid Memory Card", self.strings.get(86), self.strings.get(232), &who, &first, &second].map(str::to_string);
        for (i, line) in lines.iter().enumerate() {
            self.set_text(PASSWORD, i, line, 320, 155 + 26 * i as i16, 0, 0, true, 1, [255; 3]);
        }
    }
}
