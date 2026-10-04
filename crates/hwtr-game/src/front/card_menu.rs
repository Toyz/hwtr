//! Load/Save (states 54, 59, 105, 218 to 271): the memory cards' players.
//! Each player playing picks a card slot (left and right on its first
//! line) and one of the card's four places (up and down). Cross on a place
//! with a player opens Load / Delete / Save; on an empty one, a new place
//! for the player. Each change is written to the cards at once (0x8006917c),
//! and its outcome shown.
//!
//! The cards are `slots`: each a save as the card holds it (0x8012ff44 and
//! 0x80131054 on, a block of settings and tables and up to four players).
//! The settings and tables in play (`card`, 0x80138f14) go onto a card only
//! when a player is saved over a place.

use super::*;

const SLOT_LISTS: [usize; 2] = [19, 20];
const NEW_MENU: usize = 21;
const ENTRY_MENU: usize = 22;
const DELETE_MENU: usize = 23;
const DIALOG: usize = 24;
const CARD_HELP: usize = 5;

/// The screen's state (0x800d2741 to 0x800d2748, 0x800d1172, 0x800d27c1,
/// 0x800d135b): each player's line (0 the slot, 1 to 4 the places), the
/// menus open and their choices, whether a rename is under way, how the
/// last operation went, and which of the three backgrounds comes next.
#[derive(Clone, Copy, Debug)]
pub(super) struct CardMenu {
    line: [u8; 2],
    new_menu: bool,
    new_choice: u8,
    entry_menu: bool,
    entry_choice: u8,
    delete_menu: bool,
    delete_choice: u8,
    renaming: bool,
    succeeded: bool,
    background: u8,
}

impl Default for CardMenu {
    fn default() -> CardMenu {
        CardMenu {
            line: [0; 2],
            new_menu: false,
            new_choice: 0,
            entry_menu: false,
            entry_choice: 0,
            delete_menu: false,
            delete_choice: 0,
            renaming: false,
            succeeded: false,
            background: 1,
        }
    }
}

pub(super) fn register(r: &mut Registry<Front>) {
    // 0x80095e58: the cards read again (they are in memory), then on.
    r.add(0x8009_5e58, |_: &mut Front, p: &mut Poster| p.post(10));
    // 0x80095dfc: no card in either slot, event 107.
    r.add(0x8009_5dfc, |f: &mut Front, p: &mut Poster| {
        p.post(if f.slot_bad(0) && f.slot_bad(1) { 107 } else { 10 });
    });
    r.add(0x8009_5ea0, |f: &mut Front, p: &mut Poster| f.card_menu_enter(p));
    r.add(0x8009_620c, |f: &mut Front, _: &mut Poster| f.card_lists());
    r.add(0x8009_6918, |f: &mut Front, _: &mut Poster| {
        for k in SLOT_LISTS {
            f.screens[k].lay_out_texts(&f.font);
        }
    });
    r.add(0x8009_6958, |f: &mut Front, _: &mut Poster| {
        f.screens[SLOT_LISTS[0]].update(f.clock, &f.font);
        if f.people == 2 {
            f.screens[SLOT_LISTS[1]].update(f.clock, &f.font);
        }
        if f.cards_ui.new_menu {
            f.screens[NEW_MENU].update(f.clock, &f.font);
        }
    });
    r.add(0x8009_69c4, |f: &mut Front, p: &mut Poster| f.card_menu_draw(p));
    r.add(0x8009_6ca8, |f: &mut Front, p: &mut Poster| f.card_menu_pads(p));
    // 0x80097138: off to the main menu, the next background chosen.
    r.add(0x8009_7138, |f: &mut Front, _: &mut Poster| {
        for k in SLOT_LISTS {
            f.screens[k].exit();
        }
        f.fade_out();
        f.cards_ui.background = f.rand.below(3) as u8 + 1;
    });
    r.add(0x8009_719c, |_: &mut Front, _: &mut Poster| {});
    // Player one's presses.
    r.add(0x8009_6d2c, |f: &mut Front, p: &mut Poster| f.card_choose(0, p));
    r.add(0x8009_6b30, |f: &mut Front, _: &mut Poster| f.cards_ui.line[0] = f.cards_ui.line[0].saturating_sub(1));
    r.add(0x8009_6b4c, |f: &mut Front, _: &mut Poster| {
        if f.cards_ui.line[0] < 4 {
            f.cards_ui.line[0] += 1;
        }
    });
    r.add(0x8009_6738, |f: &mut Front, p: &mut Poster| f.slot_turn(0, false, p));
    r.add(0x8009_67a8, |f: &mut Front, p: &mut Poster| f.slot_turn(0, true, p));
    // Player two's: Start or Cross joins first (event 116, the screen
    // again with two lists).
    r.add(0x8009_6dec, |f: &mut Front, p: &mut Poster| {
        if f.people == 1 {
            f.people = 2;
            p.post(116);
        } else {
            f.card_choose(1, p);
        }
    });
    r.add(0x8009_6b6c, |f: &mut Front, _: &mut Poster| f.cards_ui.line[1] = f.cards_ui.line[1].saturating_sub(1));
    r.add(0x8009_6b88, |f: &mut Front, _: &mut Poster| {
        if f.cards_ui.line[1] < 4 {
            f.cards_ui.line[1] += 1;
        }
    });
    r.add(0x8009_6818, |f: &mut Front, p: &mut Poster| f.slot_turn(1, false, p));
    r.add(0x8009_6898, |f: &mut Front, p: &mut Poster| f.slot_turn(1, true, p));
    // The error popup (state 240): "ERROR ON MEMORY CARD IN SLOT n: UNABLE
    // TO COMPLETE THIS OPERATION.", Continue.
    r.add(0x8008_98d8, |f: &mut Front, _: &mut Poster| f.card_error_texts());
    r.add(0x8008_9c78, |f: &mut Front, p: &mut Poster| {
        let _ = f.pad_events(0, 80, p) || f.pad_events(1, 80, p);
    });
    // A place with a player: Load, Delete, Save (state 241).
    r.add(0x8009_6ec0, |f: &mut Front, _: &mut Poster| {
        f.fade_half();
        f.cards_ui.entry_menu = true;
    });
    r.add(0x8009_71e0, |f: &mut Front, p: &mut Poster| {
        let c = f.cards_ui.entry_choice;
        let mut mask = if c == 0 { 48 } else { 49 };
        if c < 2 {
            mask |= 2;
        }
        f.pad_events(f.sign_in_back as usize, mask, p);
    });
    r.add(0x8009_71a4, |f: &mut Front, _: &mut Poster| {
        f.cards_ui.entry_choice = f.cards_ui.entry_choice.saturating_sub(1)
    });
    r.add(0x8009_71c0, |f: &mut Front, _: &mut Poster| {
        if f.cards_ui.entry_choice < 2 {
            f.cards_ui.entry_choice += 1;
        }
    });
    r.add(0x8009_7240, |f: &mut Front, p: &mut Poster| {
        // The background back to full (0x800287b8 with 255).
        f.fade.level = 255;
        if f.chosen_slot_bad(p) {
            return;
        }
        p.post(match f.cards_ui.entry_choice {
            0 => 121,
            1 => 123,
            2 => 122,
            _ => 112,
        });
    });
    // An empty place: a new player there, or not (state 242).
    r.add(0x8009_6f10, |f: &mut Front, _: &mut Poster| {
        f.fade_half();
        f.cards_ui.new_menu = true;
    });
    r.add(0x8009_6bc4, |f: &mut Front, p: &mut Poster| {
        let mask = if f.cards_ui.new_choice != 0 { 49 } else { 50 };
        f.pad_events(f.sign_in_back as usize, mask, p);
    });
    r.add(0x8009_6ba8, |f: &mut Front, _: &mut Poster| f.cards_ui.new_choice = 0);
    r.add(0x8009_6bb4, |f: &mut Front, _: &mut Poster| f.cards_ui.new_choice = 1);
    r.add(0x8009_6c14, |f: &mut Front, p: &mut Poster| {
        f.chosen_slot_bad(p);
        if f.cards_ui.new_choice == 0 {
            f.board.text.clear();
            p.post(117);
        } else {
            p.post(118);
        }
    });
    // Delete: sure? (state 253).
    r.add(0x8009_6ee8, |f: &mut Front, _: &mut Poster| {
        f.fade_half();
        f.cards_ui.delete_menu = true;
    });
    r.add(0x8009_79a4, |f: &mut Front, p: &mut Poster| {
        let mask = if f.cards_ui.delete_choice != 0 { 49 } else { 50 };
        f.pad_events(f.sign_in_back as usize, mask, p);
    });
    r.add(0x8009_7988, |f: &mut Front, _: &mut Poster| f.cards_ui.delete_choice = 0);
    r.add(0x8009_7994, |f: &mut Front, _: &mut Poster| f.cards_ui.delete_choice = 1);
    r.add(0x8009_79f4, |f: &mut Front, p: &mut Poster| {
        f.chosen_slot_bad(p);
        p.post(if f.cards_ui.delete_choice == 0 { 124 } else { 125 });
    });
    // The operations.
    r.add(0x8009_7320, |f: &mut Front, p: &mut Poster| f.card_save_new(p));
    r.add(0x8009_6f38, |f: &mut Front, p: &mut Poster| {
        f.card_load();
        p.post(10);
    });
    r.add(0x8009_756c, |f: &mut Front, p: &mut Poster| f.card_save_over(p));
    r.add(0x8009_7a7c, |f: &mut Front, _: &mut Poster| f.card_delete());
    // "Do you wish to overwrite the existing game?" (state 252).
    r.add(0x8009_59ac, |f: &mut Front, _: &mut Poster| {
        let (no, yes) = (f.strings.get(93).to_string(), f.strings.get(90).to_string());
        f.dialog_texts(["Do you wish to", "overwrite the", "existing game?", ""], [&no, &yes]);
    });
    // The outcomes (states 247, 262, 269) and on from them: back to the
    // card (event 119) or, failed, to the card check (120).
    r.add(0x8009_57c4, |f: &mut Front, _: &mut Poster| f.outcome_texts("Save"));
    r.add(0x8009_55dc, |f: &mut Front, _: &mut Poster| f.outcome_texts("Load"));
    r.add(0x8009_53f4, |f: &mut Front, _: &mut Poster| f.outcome_texts("Delete"));
    r.add(0x8009_5bdc, |f: &mut Front, p: &mut Poster| {
        f.pad_events(f.sign_in_back as usize, 80, p);
    });
    for at in [0x8009_5cfc, 0x8009_5d40, 0x8009_5d84] {
        r.add(at, |f: &mut Front, p: &mut Poster| p.post(if f.cards_ui.succeeded { 119 } else { 120 }));
    }
    // Renaming a place's player (states 220, 221, 260, 261, 264; nothing
    // leads there in the game).
    r.add(0x8009_7b98, |f: &mut Front, p: &mut Poster| f.board_for(sign_in::Typing::CardRename, "", p));
    r.add(0x8009_785c, |f: &mut Front, _: &mut Poster| {
        let k = f.sign_in_back as usize;
        if let Some(record) = f.chosen_record(k).cloned() {
            f.board.text = record.name.bytes().take(20).collect();
        }
        f.cards_ui.renaming = true;
    });
    r.add(0x8009_78d0, |f: &mut Front, p: &mut Poster| p.post(if f.cards_ui.renaming { 108 } else { 10 }));
    r.add(0x8009_7914, |f: &mut Front, _: &mut Poster| {
        let k = f.sign_in_back as usize;
        let name = f.board_text();
        let (slot, at) = f.chosen_place(k);
        if let Some(record) = f.slots[slot].as_mut().and_then(|s| s.players.get_mut(at)) {
            record.name = name;
        }
        f.write_cards();
    });
}

impl Front {
    /// 0x80068ca4: the slot has no card the game can use.
    fn slot_bad(&self, slot: usize) -> bool {
        slot >= 2 || self.card_code(slot) != 0
    }

    /// 0x800695a8: how many players the card in `slot` holds.
    fn slot_count(&self, slot: usize) -> usize {
        self.slots.get(slot).and_then(Option::as_ref).map_or(0, |s| s.players.len())
    }

    /// The slot and place player `k` is on.
    fn chosen_place(&self, k: usize) -> (usize, usize) {
        (self.card_place[k] as usize & 1, (self.cards_ui.line[k] as usize).saturating_sub(1))
    }

    fn chosen_record(&self, k: usize) -> Option<&Profile> {
        let (slot, at) = self.chosen_place(k);
        self.slots[slot].as_ref().and_then(|s| s.players.get(at))
    }

    /// The slot of the player choosing has no card: the error (event 111).
    fn chosen_slot_bad(&mut self, p: &mut Poster) -> bool {
        let slot = self.card_place[self.sign_in_back as usize] as usize;
        if self.slot_bad(slot) {
            self.trouble_slot = slot as u8;
            p.post(111);
            return true;
        }
        false
    }

    /// 0x8006917c: every card written (a card without an id given one).
    fn write_cards(&mut self) -> bool {
        for k in 0..2 {
            if let Some(save) = self.slots[k].as_mut() {
                if save.card_id == 0 {
                    save.card_id = card_id(&mut self.rand);
                }
                self.save_due[k] = true;
            }
        }
        true
    }

    /// Both cards' records of who last played as player `k`.
    fn note_player_on_cards(&mut self, k: usize, id: u32) {
        for save in self.slots.iter_mut().flatten() {
            save.player_ids[k] = id;
        }
    }

    /// 0x80095ea0: the screen comes on (`psxload1` to 3 in turn), each
    /// player's name over their list, everything closed and on the first
    /// line; each player on the card they came from, or the first card in
    /// (none: event 111).
    fn card_menu_enter(&mut self, p: &mut Poster) {
        self.background = Some(format!("psxload{}", self.cards_ui.background));
        self.fade_in();
        for k in [SLOT_LISTS[0], SLOT_LISTS[1], NEW_MENU, ENTRY_MENU, DELETE_MENU] {
            self.screens[k].enter(&self.font);
        }
        let one = self.players[0].name.clone();
        self.set_text(SLOT_LISTS[0], 5, &one, 190, 140, 0, 0, true, 0, [255; 3]);
        let two = if self.people == 2 { self.players[1].name.clone() } else { String::new() };
        self.set_text(SLOT_LISTS[1], 5, &two, 450, 140, 0, 0, true, 0, [255; 3]);
        self.set_text(ENTRY_MENU, 0, "Save", 320, 285, 0, 0, true, 0, [255; 3]);
        self.copy_all_letters();
        for k in [ENTRY_MENU, SLOT_LISTS[0], SLOT_LISTS[1]] {
            self.screens[k].reenter_texts(&self.font);
        }
        self.cards_ui = CardMenu { background: self.cards_ui.background, ..CardMenu::default() };
        let Some(first) = (0..2).find(|&k| !self.slot_bad(k)) else {
            p.post(111);
            return;
        };
        for k in 0..2 {
            self.saved[k] = Some(self.players[k].password_key());
            let tag = self.players[k].tag;
            let from = (0..2).find(|&s| tag != 0 && self.slots[s].as_ref().is_some_and(|c| c.card_id == tag));
            self.card_place[k] = from.unwrap_or(first) as u8;
        }
        self.sign_in_back = false;
        self.helps[CARD_HELP].x = self.helps[CARD_HELP].start << 12;
    }

    /// 0x8009620c: each player's list: "Memory Card In Slot n" and the
    /// card's four places, by name or EMPTY.
    fn card_lists(&mut self) {
        let empty = self.strings.get(84).to_string();
        let columns: &[(usize, i16)] = if self.people == 2 { &[(0, 190), (1, 445)] } else { &[(0, 190)] };
        if self.people != 2 {
            for k in 0..7 {
                self.set_text(SLOT_LISTS[1], k, "", 450, 210 + 40 * k as i16, 0, 0, true, 1, [127; 3]);
            }
        } else {
            let two = self.players[1].name.clone();
            self.set_text(SLOT_LISTS[1], 5, &two, 445, 140, 0, 0, true, 0, [255; 3]);
        }
        for &(k, x) in columns {
            let slot = self.card_place[k] as usize;
            if self.slot_bad(slot) {
                continue;
            }
            let names: Vec<String> = self.slots[slot]
                .as_ref()
                .map(|s| s.players.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default();
            self.set_text(SLOT_LISTS[k], 6, "Memory Card", x, 195, 0, 0, true, 1, [255; 3]);
            self.set_text(SLOT_LISTS[k], 0, &format!("In Slot {}", slot + 1), x, 220, 0, 0, true, 1, [127; 3]);
            for i in 0..4 {
                let name = names.get(i).unwrap_or(&empty);
                self.set_text(
                    SLOT_LISTS[k],
                    i + 1,
                    &format!("{}. {name}", i + 1),
                    x,
                    260 + 40 * i as i16,
                    0,
                    0,
                    true,
                    1,
                    [127; 3],
                );
            }
        }
    }

    /// 0x800969c4: the menu open, or the players' lists lit on their lines
    /// with the help line.
    fn card_menu_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        let ui = self.cards_ui;
        if ui.new_menu {
            self.draw_screen(NEW_MENU, Some(ui.new_choice as usize), None, false);
        } else if ui.entry_menu {
            if ui.delete_menu {
                self.draw_screen(DELETE_MENU, Some(ui.delete_choice as usize), None, false);
            } else if ui.entry_choice == 2 {
                self.draw_screen(ENTRY_MENU, Some(0), None, false);
            } else {
                self.draw_screen(ENTRY_MENU, None, Some(ui.entry_choice as usize), false);
            }
        } else {
            let hidden = self.fade.hidden;
            self.draw_screen(SLOT_LISTS[0], Some(ui.line[0] as usize), None, hidden);
            if self.people == 2 {
                self.draw_screen(SLOT_LISTS[1], Some(ui.line[1] as usize), None, hidden);
            }
            if !hidden {
                self.draw_help(CARD_HELP);
            }
        }
        self.end_frame();
    }

    /// 0x80096ca8: player one: Start, Triangle, and left and right on the
    /// slot line or Cross and up on the places, down above the last; player
    /// two the same once playing, else only Start.
    fn card_menu_pads(&mut self, p: &mut Poster) {
        let masks = |line: u8, slot_mask: u16, place_mask: u16| {
            let mut m = if line == 0 { slot_mask } else { place_mask };
            if line < 4 {
                m |= 2;
            }
            m
        };
        let one = masks(self.cards_ui.line[0], 108, 113);
        self.pad_events(0, one, p);
        let two = if self.people == 1 { 64 } else { masks(self.cards_ui.line[1], 12, 17) };
        self.pad_events(1, two, p);
    }

    /// 0x80096d2c, 0x80096dec: Cross on player `k`'s line: the slot line
    /// does nothing (event 112), a place with a player opens its menu
    /// (114), an empty one the new place (115).
    fn card_choose(&mut self, k: usize, p: &mut Poster) {
        let slot = self.card_place[k] as usize;
        if self.slot_bad(slot) {
            self.trouble_slot = slot as u8;
            p.post(111);
            return;
        }
        let line = self.cards_ui.line[k] as usize;
        if line == 0 {
            p.post(112);
        } else if line == 5 {
            p.post(113);
        } else {
            self.sign_in_back = k == 1;
            p.post(if self.slot_count(slot) < line { 115 } else { 114 });
        }
    }

    /// 0x80096738 and its neighbours: on the slot line, the other slot (a
    /// slot without a card: event 111).
    fn slot_turn(&mut self, k: usize, _right: bool, p: &mut Poster) {
        if self.cards_ui.line[k] != 0 || (k == 1 && self.people != 2) {
            return;
        }
        // Two slots: one step either way is the other one.
        self.card_place[k] = (self.card_place[k] + 1) & 1;
        let slot = self.card_place[k];
        if self.slot_bad(slot as usize) {
            self.trouble_slot = slot;
            p.post(111);
        }
    }

    /// 0x800898d8: the error popup for the slot that failed.
    fn card_error_texts(&mut self) {
        let slot = format!("{} {}:", self.strings.get(85), self.trouble_slot + 1);
        self.set_text(POPUP, 0, "ERROR ON", 320, 155, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 1, &slot, 320, 181, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 2, "UNABLE TO COMPLETE", 320, 207, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 3, "THIS OPERATION.", 320, 233, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 4, "", 320, 259, 1, 0, true, 1, [255; 3]);
        self.set_text(POPUP, 5, "CONTINUE", 320, 286, 1, 0, true, 1, [127; 3]);
        self.popup_choice = 5;
    }

    /// Screen 24's six lines: four of text and two choices, the first
    /// chosen.
    fn dialog_texts(&mut self, lines: [&str; 4], choices: [&str; 2]) {
        for (i, line) in lines.iter().enumerate() {
            self.set_text(DIALOG, i, line, 320, 155 + 26 * i as i16, 0, 0, true, 0, [255; 3]);
        }
        self.set_text(DIALOG, 4, choices[0], 320, 259, 1, 0, true, 1, [127; 3]);
        self.set_text(DIALOG, 5, choices[1], 320, 286, 1, 0, true, 1, [127; 3]);
        self.screens[DIALOG].enter(&self.font);
        self.board.choice = 4;
    }

    /// 0x800957c4, 0x800955dc, 0x800953f4: "Memory Card: <what>
    /// Succeeded." or "Failed.", OK.
    fn outcome_texts(&mut self, what: &str) {
        let outcome = format!("{what} {}", if self.cards_ui.succeeded { "Succeeded." } else { "Failed." });
        self.dialog_texts(["", "Memory Card", &outcome, ""], ["OK", ""]);
    }

    /// 0x80097320: the player onto the card as a new place (after the
    /// last), noted on both cards as who last played; written.
    fn card_save_new(&mut self, p: &mut Poster) {
        let k = self.sign_in_back as usize;
        let slot = self.card_place[k] as usize & 1;
        let line = self.cards_ui.line[k] as usize;
        let at = line.saturating_sub(1).min(self.slot_count(slot));
        let mut record = self.players[k].clone();
        record.id = card_id(&mut self.rand);
        let added = match self.slots[slot].as_mut() {
            Some(save) if save.players.len() < card::MOST_PLAYERS => {
                save.players.push(record);
                true
            }
            _ => false,
        };
        if !added {
            self.cards_ui.succeeded = false;
            p.post(120);
            return;
        }
        let id = self.slots[slot].as_ref().and_then(|s| s.players.get(at)).map_or(0, |r| r.id);
        self.note_player_on_cards(k, id);
        self.cards_ui.succeeded = self.write_cards();
        p.post(if self.cards_ui.succeeded { 119 } else { 120 });
    }

    /// 0x80096f38: the place's player becomes the player, noted on both
    /// cards and its password as saved; written.
    fn card_load(&mut self) {
        let k = self.sign_in_back as usize;
        let Some(record) = self.chosen_record(k).cloned() else {
            self.cards_ui.succeeded = false;
            return;
        };
        self.players[k] = record;
        let id = self.players[k].id;
        self.note_player_on_cards(k, id);
        self.saved[k] = Some(self.players[k].password_key());
        self.cards_ui.succeeded = self.write_cards();
    }

    /// 0x8009756c: the player (with the pad's buttons) over the place, the
    /// settings and tables in play onto that card, noted on both cards;
    /// written.
    fn card_save_over(&mut self, p: &mut Poster) {
        let k = self.sign_in_back as usize;
        self.players[k].mapping = self.mappings[k];
        let (slot, at) = self.chosen_place(k);
        let player = self.players[k].clone();
        let current = self.card.clone();
        if let Some(save) = self.slots[slot].as_mut() {
            if let Some(record) = save.players.get_mut(at) {
                *record = player;
            }
            save.high_scores = current.high_scores;
            save.best_times = current.best_times;
            save.cup_winners = current.cup_winners;
            save.settings = self.settings;
            save.player_ids = current.player_ids;
        }
        let id = self.chosen_record(k).map_or(0, |r| r.id);
        self.note_player_on_cards(k, id);
        self.cards_ui.succeeded = self.write_cards();
        p.post(if self.cards_ui.succeeded { 119 } else { 120 });
    }

    /// 0x80097a7c: the place's player off the card (those after it move
    /// up); written.
    fn card_delete(&mut self) {
        let k = self.sign_in_back as usize;
        let (slot, at) = self.chosen_place(k);
        if let Some(save) = self.slots[slot].as_mut()
            && at < save.players.len()
        {
            save.players.remove(at);
        }
        self.cards_ui.succeeded = self.write_cards();
    }
}
