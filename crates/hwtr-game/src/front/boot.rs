//! Boot (state 1) and the memory card (states 2 to 43): the cards looked
//! at, the game's save read from the first card that has it, or, when none
//! can be used, the reason asked about: a card without the game's file
//! (create it?), one whose save is damaged, a full one, none at all
//! (continue without saving?).

use super::*;

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8008_a8e0, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_aa18, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_ac48, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_aa70, |_: &mut Front, _: &mut Poster| {});
    // strings and car facts: read in `new`
    r.add(0x8008_a920, |_: &mut Front, _: &mut Poster| {});
    // the screens' 3D pieces: the program reads SCREENS.SCR
    r.add(0x8008_a860, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_81a4, |f: &mut Front, _: &mut Poster| f.copy_all_letters());
    r.add(0x8008_32f0, |f: &mut Front, _: &mut Poster| f.boot_screens());
    r.add(0x8008_ad68, |f: &mut Front, _: &mut Poster| f.fade = Fade::default());
    r.add(0x8008_aab0, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_af48, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_bc84, |f: &mut Front, _: &mut Poster| f.people = 1);
    r.add(0x8008_6a04, |_: &mut Front, p: &mut Poster| p.post(10));
    r.add(0x8008_9eb4, |f: &mut Front, _: &mut Poster| {
        f.popup_choice = 4;
        f.screens[POPUP].enter(&f.font);
    });
    r.add(0x8008_8760, |f: &mut Front, p: &mut Poster| p.post(if f.card_present() { 12 } else { 13 }));
    r.add(0x8008_8828, |f: &mut Front, p: &mut Poster| {
        // Defaults, then what the first card with the save holds (none:
        // event 23).
        f.settings = Settings::new(f.default_difficulty);
        f.players = [Profile::new(&f.strings, 0), Profile::new(&f.strings, 1)];
        let Some(save) = f.slots.iter().flatten().next().cloned() else {
            p.post(23);
            return;
        };
        f.card = save;
        f.settings = f.card.settings;
        for k in 0..2 {
            if let Some(pl) = f.card.player(k) {
                f.players[k] = pl.clone();
            }
            // 0x8001bafc: the player's buttons into the pad's.
            f.mappings[k] = f.players[k].mapping;
        }
        p.post(22);
    });
    // 0x800887a8: an unformatted card (event 14); there is never one here.
    r.add(0x8008_87a8, |_: &mut Front, p: &mut Poster| p.post(13));
    r.add(0x8008_9d18, |f: &mut Front, p: &mut Poster| f.card_trouble(p));
    // The dialogs for each trouble.
    r.add(0x8008_91d0, |f: &mut Front, _: &mut Poster| {
        let slot = format!("{} {}", f.strings.get(85), f.trouble_slot + 1);
        let lines = [slot, f.text(252), f.text(253), f.text(254)];
        f.popup_texts(lines, [93, 90]);
    });
    r.add(0x8008_8e74, |f: &mut Front, _: &mut Poster| {
        let slot = format!("{} {}:", f.strings.get(85), f.trouble_slot + 1);
        let lines = [slot, f.text(91), f.text(86), f.text(92)];
        f.popup_texts(lines, [93, 90]);
    });
    r.add(0x8008_938c, |f: &mut Front, _: &mut Poster| {
        let slot = format!("{} {}:", f.strings.get(85), f.trouble_slot + 1);
        f.popup_texts([slot, "Other Error".into(), "Don't Use?".into(), String::new()], [89, 90]);
    });
    r.add(0x8008_9710, |f: &mut Front, _: &mut Poster| {
        let slot = format!("{} {}:", f.strings.get(85), f.trouble_slot + 1);
        f.popup_texts([slot, "Other Error.".into(), "Don't Use?".into(), String::new()], [89, 90]);
    });
    r.add(0x8008_9554, |f: &mut Front, _: &mut Poster| {
        let slot = format!("{} {}", f.strings.get(85), f.trouble_slot + 1);
        let lines = [slot, f.text(225), f.text(226), f.text(227)];
        f.popup_texts(lines, [90, 89]);
    });
    r.add(0x8008_9030, |f: &mut Front, _: &mut Poster| {
        let lines = [f.text(221), f.text(222), f.text(87), f.text(88)];
        f.popup_texts(lines, [93, 90]);
    });
    r.add(0x8008_9aac, |f: &mut Front, _: &mut Poster| {
        let slot = format!("IN SLOT {}:", f.trouble_slot + 1);
        let lines = ["MEMORY CARD NOT INSERTED", &slot, "UNABLE TO COMPLETE", "THIS OPERATION."].map(str::to_string);
        for (i, line) in lines.iter().enumerate() {
            f.set_text(POPUP, i, line, 320, 155 + 26 * i as i16, 0, 0, true, 0, [255; 3]);
        }
        f.set_text(POPUP, 4, "", 320, 259, 1, 0, true, 1, [255; 3]);
        f.set_text(POPUP, 5, "CONTINUE", 320, 286, 1, 0, true, 1, [127; 3]);
        f.popup_choice = 5;
    });
    // 0x80089fb4: the game's file made on the card (with nothing read yet,
    // its settings and the players start afresh), then on (event 10).
    r.add(0x8008_9fb4, |f: &mut Front, p: &mut Poster| {
        let k = f.trouble_slot as usize & 1;
        let save = Save::new(card_id(&mut f.rand), f.strings.get(84), Settings::new(f.default_difficulty));
        f.card_status[k] = CardStatus::Ready;
        f.slots[k] = Some(save.clone());
        f.save_due[k] = true;
        f.card = save;
        f.settings = f.card.settings;
        f.players = [Profile::new(&f.strings, 0), Profile::new(&f.strings, 1)];
        p.post(10);
    });
    // 0x80088714: the card set aside, as if not there.
    r.add(0x8008_8714, |f: &mut Front, _: &mut Poster| f.card_seen[f.trouble_slot as usize & 1] = false);
    // 0x80089c9c: the card formatted (event 32; it never fails here).
    r.add(0x8008_9c9c, |f: &mut Front, p: &mut Poster| {
        f.card_status[f.trouble_slot as usize & 1] = CardStatus::NoFile;
        p.post(32);
    });
    // 0x8008a0c8: after a failed save, the card again: gone (event 35),
    // full (21), or something else (36).
    r.add(0x8008_a0c8, |f: &mut Front, p: &mut Poster| {
        p.post(match f.card_code(f.trouble_slot as usize & 1) {
            1 => 35,
            7 => 21,
            _ => 36,
        })
    });
    // State 3, the front end's files not there: the original lets
    // everything go and stops; here the front end does not build at all.
    for at in [0x8008_a8c0, 0x8008_36e8, 0x8008_3350, 0x8008_81d4, 0x8008_aa50, 0x8008_ac88] {
        r.add(at, |_: &mut Front, _: &mut Poster| {});
    }
    r.add(0x8008_8cbc, |f: &mut Front, _: &mut Poster| f.card_missing_texts());
    r.add(0x8008_8100, |f: &mut Front, _: &mut Poster| f.copy_all_letters());
    r.add(0x8008_9ee4, |f: &mut Front, _: &mut Poster| f.screens[POPUP].lay_out_texts(&f.font));
    r.add(0x8008_9f40, |f: &mut Front, _: &mut Poster| {
        f.begin_frame();
        let choice = Some(f.popup_choice as usize);
        f.draw_screen(POPUP, choice, None, false);
        f.end_frame();
    });
    r.add(0x8008_9f0c, |f: &mut Front, p: &mut Poster| {
        let mask = if f.popup_choice == 4 { 82 } else { 81 };
        let _ = f.pad_events(0, mask, p) || f.pad_events(1, mask, p);
    });
    r.add(0x8008_9f94, |f: &mut Front, _: &mut Poster| f.popup_choice = 4);
    r.add(0x8008_9fa4, |f: &mut Front, _: &mut Poster| f.popup_choice = 5);
    r.add(0x8008_a228, |f: &mut Front, p: &mut Poster| p.post(if f.popup_choice == 4 { 31 } else { 30 }));
    // the cards looked at again
    r.add(0x8008_8738, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_a184, |f: &mut Front, _: &mut Poster| {
        f.settings = Settings::new(f.default_difficulty);
        f.players = [Profile::new(&f.strings, 0), Profile::new(&f.strings, 1)];
    });
    r.add(0x8008_a270, |_: &mut Front, p: &mut Poster| p.post(10));
}

/// What a card slot holds, as far as the game is concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardStatus {
    /// No card.
    #[default]
    Missing,
    /// A card without the game's file.
    NoFile,
    /// A card whose save fails its checksum.
    Damaged,
    /// A card with the game's save.
    Ready,
}

impl Front {
    /// 0x80068ca4: slot `k`'s state as the game codes it: 0 ready, 1 no
    /// card (or one set aside), 3 no file, 8 a damaged save.
    pub(super) fn card_code(&self, k: usize) -> u8 {
        if !self.card_seen[k] {
            return 1;
        }
        match self.card_status[k] {
            CardStatus::Missing => 1,
            CardStatus::NoFile => 3,
            CardStatus::Damaged => 8,
            CardStatus::Ready => 0,
        }
    }

    /// 0x80068934: a card in either slot.
    pub(super) fn card_present(&self) -> bool {
        (0..2).any(|k| self.card_code(k) != 1)
    }

    /// 0x80089d18: why no card could be used, the first slot with a card
    /// deciding: none at all (event 16), unformatted (14), no file (18), a
    /// new card (17), full (21), an error (19), anything else (20).
    fn card_trouble(&mut self, p: &mut Poster) {
        let codes = [self.card_code(0), self.card_code(1)];
        if codes.iter().all(|&c| c == 1) {
            p.post(16);
            return;
        }
        for (k, &c) in codes.iter().enumerate() {
            let event = match c {
                6 => 14,
                3 => 18,
                2 => 17,
                7 => 21,
                4 => 19,
                _ => continue,
            };
            self.trouble_slot = k as u8;
            p.post(event);
            return;
        }
        p.post(20);
    }

    /// String `k`, owned.
    fn text(&self, k: usize) -> String {
        self.strings.get(k).to_string()
    }

    /// The card dialogs' lines: four of text, then two choices (strings).
    fn popup_texts(&mut self, lines: [String; 4], choices: [usize; 2]) {
        for (i, line) in lines.iter().enumerate() {
            self.set_text(POPUP, i, line, 320, 155 + 26 * i as i16, 0, 0, true, 0, [255; 3]);
        }
        let (a, b) = (self.strings.get(choices[0]).to_string(), self.strings.get(choices[1]).to_string());
        self.set_text(POPUP, 4, &a, 320, 259, 1, 0, true, 1, [127; 3]);
        self.set_text(POPUP, 5, &b, 320, 286, 1, 0, true, 1, [127; 3]);
    }
}
