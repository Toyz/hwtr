//! Signing in (states 104, 106, 222 to 227, 303 to 315): the main menu's
//! sign-in line does one of three things (0x8008db1c, by 0x800d116a).
//! "Sign In" asks whether to reset the player's data, then has a new name
//! typed; "Password" has a password typed, which adds what it carries to
//! the player; "Load/Save" goes to the memory card (states 218 on).
//!
//! The typing is on a board of letters (screen 26, `psxreg` or `psxpwd`):
//! the letters of `ENGNAME.CHM` or `ENGPWD.CHM` seven to a row, then back,
//! a space (names only) and DONE. The d-pad moves a slider over them,
//! Cross types, Start is DONE, Triangle leaves.

use super::*;

const RESET: usize = 24;
const WRONG: usize = 25;
const BOARD: usize = 26;
const BOARD_HELP: usize = 8;

/// The keys' places on the 640 by 240 screen (0x800c257c, 0x800c2640):
/// one a key, the last DONE's.
const KEYS_AT: (u32, u32) = (0x800c_257c, 0x800c_2640);
const KEY_PLACES: usize = 49;

/// What the board is typing (0x800d27c4): a name for the player signing
/// in, a new name for a card's player, or a password. (Mode 1, a name for
/// a new player on the card, is never set.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Typing {
    #[default]
    Name,
    CardRename,
    Password,
}

/// The board (0x800d274e to 0x800d27c8): its letters, which key is back,
/// space and DONE, how long the text may be, where the slider is, and the
/// text (0x80138e94) and its length so far.
#[derive(Clone, Debug, Default)]
pub(super) struct Board {
    letters: Vec<u8>,
    back: usize,
    space: Option<usize>,
    done: usize,
    longest: usize,
    typing: Typing,
    at: usize,
    /// Where the slider was before it went down to DONE (0x800d27c8).
    above: usize,
    pub(super) text: Vec<u8>,
    typed: usize,
    /// The reset dialog's choice (0x800d27c2): 4 no, 5 yes.
    pub(super) choice: u8,
}

impl Board {
    fn keys(&self) -> usize {
        self.done + 1
    }
}

/// The letters of a `.CHM` file (0x8007fd44): every byte but the line ends.
pub fn chm_letters(b: &[u8]) -> Vec<u8> {
    b.iter().copied().filter(|&c| c != b'\r' && c != b'\n').collect()
}

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8008_db1c, |f: &mut Front, p: &mut Poster| {
        p.post(match f.sign_in {
            0 => 70,
            1 => 71,
            _ => 72,
        })
    });
    // 0x8008c050: Start on the main menu: a second player joins (event
    // 52); on the sign-in line it is the second player signing in (53, once
    // two play).
    r.add(0x8008_c050, |f: &mut Front, p: &mut Poster| {
        if f.people == 1 {
            p.post(52);
        }
        if f.choice == 3 {
            f.sign_in_back = true;
            p.post(53);
        } else {
            p.post(52);
        }
    });
    // 0x8008c0bc: the second player in, their car's picture into slot 1.
    r.add(0x8008_c0bc, |f: &mut Front, _: &mut Poster| {
        if f.people == 1 {
            f.decal_car[1] = Some(f.players[1].car);
            f.car_shown[1] = true;
            f.car_loaded[1] = false;
            f.people = 2;
        }
    });
    // The reset dialog (states 104, 222 to 225).
    r.add(0x8009_7e34, |f: &mut Front, _: &mut Poster| f.reset_texts());
    r.add(0x8009_5c80, |f: &mut Front, _: &mut Poster| f.screens[RESET].reenter_texts(&f.font));
    r.add(0x8009_5ca8, |f: &mut Front, _: &mut Poster| {
        f.begin_frame();
        f.draw_screen(RESET, Some(f.board.choice as usize), None, false);
        f.end_frame();
    });
    r.add(0x8009_5c14, |f: &mut Front, p: &mut Poster| {
        let mask = if f.board.choice == 4 { 82 } else { 81 };
        f.pad_events(f.sign_in_back as usize, mask, p);
    });
    r.add(0x8009_5b74, |f: &mut Front, _: &mut Poster| f.board.choice = 4);
    r.add(0x8009_5b84, |f: &mut Front, _: &mut Poster| f.board.choice = 5);
    r.add(0x8009_5b94, |f: &mut Front, p: &mut Poster| p.post(if f.board.choice == 4 { 109 } else { 110 }));
    // The board (states 106, 226, 227, 303 to 315).
    r.add(0x8009_7af8, |f: &mut Front, p: &mut Poster| {
        let name = f.players[f.sign_in_back as usize].name.clone();
        f.board_for(Typing::Name, &name, p);
    });
    r.add(0x8009_7c10, |f: &mut Front, p: &mut Poster| f.board_for(Typing::Password, "", p));
    r.add(0x8009_7ffc, |f: &mut Front, _: &mut Poster| f.board_enter());
    r.add(0x8009_7e0c, |f: &mut Front, _: &mut Poster| f.screens[BOARD].lay_out_texts(&f.font));
    r.add(0x8009_84c8, |f: &mut Front, _: &mut Poster| f.screens[BOARD].update(f.clock, &f.font));
    r.add(0x8009_84f0, |f: &mut Front, p: &mut Poster| f.board_draw(p));
    r.add(0x8009_8400, |f: &mut Front, p: &mut Poster| f.board_pads(p));
    r.add(0x8009_8390, |f: &mut Front, p: &mut Poster| {
        // Left: to the main menu, or for the card's names, the card.
        p.post(match f.board.typing {
            Typing::CardRename => 127,
            _ => 126,
        })
    });
    r.add(0x8009_88b0, |f: &mut Front, p: &mut Poster| f.board_type(p));
    r.add(0x8009_86cc, |f: &mut Front, _: &mut Poster| {
        if f.board.at > 0 {
            f.board.at -= 1;
            f.slider_to_key();
        }
    });
    r.add(0x8009_8724, |f: &mut Front, _: &mut Poster| {
        let b = &mut f.board;
        if b.at + 2 == b.keys() {
            b.at = b.done;
        } else if b.at + 1 < b.keys() {
            b.at += 1;
        } else {
            return;
        }
        f.slider_to_key();
    });
    r.add(0x8009_87b0, |f: &mut Front, _: &mut Poster| {
        let b = &mut f.board;
        if b.at == b.done {
            b.at = b.above;
        } else if b.at >= 7 {
            b.at -= 7;
        }
        f.slider_to_key();
    });
    r.add(0x8009_8824, |f: &mut Front, _: &mut Poster| {
        let b = &mut f.board;
        if b.at + 7 >= b.done {
            b.above = b.at;
            b.at = b.done;
        } else {
            b.at += 7;
        }
        f.slider_to_key();
    });
    r.add(0x8009_8204, |f: &mut Front, p: &mut Poster| f.board_accept(p));
    r.add(0x8009_8c8c, |f: &mut Front, _: &mut Poster| f.fade_out());
    r.add(0x8009_8cac, |f: &mut Front, p: &mut Poster| f.password_check(p));
    // A password that is not one (state 315).
    r.add(0x8009_8dac, |f: &mut Front, _: &mut Poster| {
        f.fade_half();
        f.screens[WRONG].enter(&f.font);
    });
    r.add(0x8009_8ddc, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        f.draw_screen(BOARD, None, None, true);
        f.draw_screen(WRONG, None, None, false);
        f.end_frame();
    });
    r.add(0x8008_ad7c, |f: &mut Front, _: &mut Poster| f.fade_in());
    r.add(0x8009_8d74, |f: &mut Front, p: &mut Poster| {
        f.pad_events(f.sign_in_back as usize, 48, p);
    });
}

impl Front {
    /// 0x80097e34: "Player data will be reset! Are you sure?", No chosen.
    fn reset_texts(&mut self) {
        let (no, yes) = (self.strings.get(93).to_string(), self.strings.get(90).to_string());
        self.set_text(RESET, 0, "Player data", 320, 155, 0, 0, true, 0, [255; 3]);
        self.set_text(RESET, 1, "will be reset!", 320, 181, 0, 0, true, 0, [255; 3]);
        self.set_text(RESET, 2, "Are you sure?", 320, 207, 0, 0, true, 0, [255; 3]);
        self.set_text(RESET, 3, "", 320, 233, 0, 0, true, 0, [255; 3]);
        self.set_text(RESET, 4, &no, 320, 259, 1, 0, true, 1, [127; 3]);
        self.set_text(RESET, 5, &yes, 320, 286, 1, 0, true, 1, [127; 3]);
        self.screens[RESET].enter(&self.font);
        self.board.choice = 4;
    }

    /// 0x80097af8, 0x80097c10: the board's letters (none: event 11), its
    /// extra keys after them, how long the text may be (10 a name, 20 a
    /// password), and the text to start from.
    pub(super) fn board_for(&mut self, typing: Typing, text: &str, p: &mut Poster) {
        let letters = match typing {
            Typing::Password => self.password_keys.clone(),
            _ => self.name_keys.clone(),
        };
        if letters.is_empty() {
            p.post(11);
        }
        let n = letters.len();
        let (space, done, longest) = match typing {
            Typing::Password => (None, n + 1, password::LENGTH),
            _ => (Some(n + 1), n + 2, 10),
        };
        self.board = Board {
            letters,
            back: n,
            space,
            done,
            longest,
            typing,
            text: text.bytes().take(20).collect(),
            ..Board::default()
        };
    }

    /// The slider to the key it is on (0x80097c88: the board's slider's
    /// place, its x by 640 and y by 480 in the screen's thousandths).
    fn slider_to_key(&mut self) {
        let k = if self.board.at == self.board.done { KEY_PLACES - 1 } else { self.board.at.min(KEY_PLACES - 1) };
        let (x, y) = self.key_place(k);
        if let Some(piece) = self.screens[BOARD].pieces.iter_mut().find(|p| p.model == "signinselc") {
            piece.to[0] = screen::world_x((((x << 12) / 640) * 1000) >> 12);
            piece.to[1] = screen::world_y((((y << 12) / 480) * 1000) >> 12);
        }
    }

    fn key_place(&self, k: usize) -> (i32, i32) {
        self.key_places.get(k).copied().unwrap_or_default()
    }

    /// 0x80097ffc: the board comes on with the text (nothing typed yet),
    /// the slider on the first key; the player (or "PASSWORD") on the left, DONE in its place.
    fn board_enter(&mut self) {
        self.fade_in();
        self.screens[BOARD].enter(&self.font);
        self.board.typed = 0;
        self.board.at = 0;
        self.slider_to_key();
        let label = if self.board.typing == Typing::Password {
            self.set_background("psxpwd");
            238
        } else {
            self.set_background("psxreg");
            if self.sign_in_back { 103 } else { 102 }
        };
        let label = self.strings.get(label).to_string();
        let text = self.board_text();
        let done = self.strings.get(101).to_string();
        let (dx, dy) = self.key_place(KEY_PLACES - 1);
        self.set_text(BOARD, 0, &label, 167, 194, 0, 0, true, 1, [255; 3]);
        self.set_text(BOARD, 1, &text, 435, 380, 0, 0, true, 0, [255; 3]);
        self.set_text(BOARD, 2, "", 435, 380, 0, 0, true, 0, [255; 3]);
        self.set_text(BOARD, 3, &done, dx as i16, (dy * 2 - 16) as i16, 0, 0, true, 1, [247, 220, 7]);
        self.helps[BOARD_HELP].x = self.helps[BOARD_HELP].start << 12;
    }

    pub(super) fn board_text(&self) -> String {
        self.board.text.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
    }

    /// 0x800984f0: the board, DONE lit when the slider is on it, and every
    /// other key's letter (back and space as arrows) in yellow, the one
    /// under the slider in white.
    fn board_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        let lit = (self.board.at == self.board.done).then_some(3);
        self.draw_screen(BOARD, lit, None, self.fade.hidden);
        if !self.fade.hidden {
            for k in 0..self.board.done {
                let colour: [u8; 3] = if k == self.board.at { [240, 240, 240] } else { [247, 222, 7] };
                let ch = if k == self.board.back {
                    154
                } else if Some(k) == self.board.space {
                    155
                } else {
                    self.board.letters.get(k).copied().unwrap_or(b' ')
                };
                let (x, y) = self.key_place(k.min(KEY_PLACES - 1));
                self.drawing.push(screen::Glyph {
                    ch: ch.to_ascii_uppercase(),
                    x: (x - 5) as i16,
                    y: (y - 4) as i16,
                    colour: colour.map(|c| c >> 1),
                });
            }
            self.draw_help(BOARD_HELP);
        }
        self.end_frame();
    }

    /// 0x80098400: Start and Triangle always; Cross while there is room,
    /// or on back or DONE; the arrows where there is a key that way.
    fn board_pads(&mut self, p: &mut Poster) {
        let b = &self.board;
        let mut mask = 64 | 32;
        if b.typed < b.longest || b.at == b.back || b.at == b.done {
            mask |= 16;
        }
        if b.at != 0 {
            mask |= 4;
        }
        if b.at < b.done {
            mask |= 8;
        }
        if b.at != b.done {
            mask |= 2;
        }
        if b.at >= 7 {
            mask |= 1;
        }
        self.pad_events(self.sign_in_back as usize, mask, p);
    }

    /// 0x800988b0: Cross on a key: back takes the last letter off, the
    /// space and the letters go on the end (a "PLAYER n" left from before
    /// is cleared first), DONE is Start (event 129); the text shown again
    /// (a password over two lines of ten), event 130.
    fn board_type(&mut self, p: &mut Poster) {
        let player_names = [self.strings.get(102).to_string(), self.strings.get(103).to_string()];
        let b = &mut self.board;
        let length = |t: &[u8]| t.iter().take_while(|&&c| c != 0).count();
        let clear_default = |b: &mut Board| {
            let text: String = b.text.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
            if player_names.contains(&text) {
                if b.typed == 0 {
                    b.text.clear();
                }
            } else {
                b.typed = length(&b.text);
            }
        };
        if b.at == b.back {
            if b.typed == 0 {
                b.typed = length(&b.text);
            }
            if b.typed > 0 {
                b.typed -= 1;
                b.text.truncate(b.typed);
            }
        } else if Some(b.at) == b.space {
            clear_default(b);
            if b.typed < b.longest {
                b.text.truncate(b.typed);
                b.text.push(b' ');
                b.typed += 1;
            }
        } else if b.at == b.done {
            p.post(129);
            return;
        } else {
            clear_default(b);
            if b.typed < b.longest {
                let c = b.letters.get(b.at).copied().unwrap_or(b' ');
                b.text.truncate(b.typed);
                b.text.push(c);
                b.typed += 1;
            }
        }
        let shown: Vec<u8> = b.text.iter().copied().take(b.typed).collect();
        if b.longest >= 11 {
            let first: String = shown.iter().take(10).map(|&c| c as char).collect();
            let second: String = shown.iter().skip(10).take(10).map(|&c| c as char).collect();
            self.set_text(BOARD, 1, &first, 435, 365, 0, 0, true, 0, [255; 3]);
            self.set_text(BOARD, 2, &second, 435, 395, 0, 0, true, 0, [255; 3]);
        } else {
            let text = self.board_text();
            self.set_text(BOARD, 1, &text, 435, 380, 0, 0, true, 0, [255; 3]);
            self.set_text(BOARD, 2, "", 435, 380, 0, 0, true, 0, [255; 3]);
        }
        p.post(130);
    }

    /// 0x80098204: DONE. A name starts the player afresh under it (its
    /// password noted as saved), back to the main menu (event 126); the
    /// card's names go back to the card (127); a password is checked (128).
    fn board_accept(&mut self, p: &mut Poster) {
        match self.board.typing {
            Typing::Name => {
                let k = self.sign_in_back as usize;
                let name = self.board_text();
                let (tag, id) = (self.players[k].tag, self.players[k].id);
                self.players[k] = Profile { tag, id, name, ..Profile::new(&self.strings, k as u8) };
                self.saved[k] = Some(self.players[k].password_key());
                p.post(126);
            }
            Typing::CardRename => p.post(127),
            Typing::Password => p.post(128),
        }
    }

    /// 0x80098cac: the password read into the player (event 131, its
    /// password noted as saved), or not one (a buzz, event 132).
    fn password_check(&mut self, p: &mut Poster) {
        let k = self.sign_in_back as usize;
        let text = self.board_text();
        let mut player = self.players[k].clone();
        if self.passwords.read(&text, &mut player) {
            self.players[k] = player;
            self.saved[k] = Some(self.players[k].password_key());
            p.post(131);
        } else {
            self.play(53);
            p.post(132);
        }
    }
}

/// The keys' places, from the executable.
pub(super) fn key_places(byte: &dyn Fn(u32) -> u8) -> Vec<(i32, i32)> {
    let word = |a: u32| i32::from_le_bytes(std::array::from_fn(|k| byte(a + k as u32)));
    (0..KEY_PLACES as u32).map(|k| (word(KEYS_AT.0 + 4 * k), word(KEYS_AT.1 + 4 * k))).collect()
}
