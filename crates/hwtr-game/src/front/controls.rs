//! The controls screen (states 196 to 217; screens 13 to 17, `psxctrl`):
//! each player's list, Vibration and then the thirteen actions with the
//! button each is on (as its icon), six rows at a time. Cross on Vibration
//! turns the motors on or off (with a buzz); on an action it waits for the
//! button to put it on. Leaving with two actions on one button (other than
//! the pairs meant to share) shows an error; otherwise the buttons go into
//! the players' records.

use super::*;

const SAME_ERROR: usize = 13;
const LISTS: [usize; 2] = [14, 15];
const CHOOSING: [usize; 2] = [16, 17];
const CONTROLS_HELP: usize = 4;
/// The rows shown at once.
const ROWS: u8 = 6;
/// Vibration and the actions.
const ACTIONS: usize = 13;
/// The mapping's word for the motors (control_mapping +0x70).
const VIBRATION: usize = 28;

/// The screen's state (0x800d27b9 to 0x800d27c0): each player waiting for
/// a button, the first row shown, one past the last, and the row chosen
/// among those shown; the two actions an error names; when the buzz began.
#[derive(Clone, Debug, Default)]
pub(super) struct ControlsMenu {
    choosing: [bool; 2],
    top: [u8; 2],
    end: [u8; 2],
    row: [u8; 2],
    clash: (String, String),
    buzz_from: u32,
}

/// The executable's names for the buttons (0x800bdc7c, by bit: L2, R2, L1,
/// R1, TRI, O, X, SQ, select, start, then the d-pad), the icon for each
/// (0x800c282c), and which action may share a button with which
/// (0x800c2704).
#[derive(Clone, Debug, Default)]
pub struct ControlNames {
    buttons: Vec<String>,
    icons: Vec<(String, String)>,
    shares: [u8; 14],
}

impl ControlNames {
    pub fn read(byte: &dyn Fn(u32) -> u8) -> ControlNames {
        let word = |a: u32| u32::from_le_bytes(std::array::from_fn(|k| byte(a + k as u32)));
        let text = |mut a: u32| {
            let mut s = String::new();
            while a != 0 && byte(a) != 0 && s.len() < 16 {
                s.push(byte(a) as char);
                a += 1;
            }
            s
        };
        ControlNames {
            buttons: (0..16).map(|k| text(word(0x800b_dc7c + 4 * k))).collect(),
            icons: (0..14).map(|k| (text(word(0x800c_282c + 8 * k)), text(word(0x800c_2830 + 8 * k)))).collect(),
            shares: std::array::from_fn(|k| byte(0x800c_2704 + k as u32)),
        }
    }
}

/// The actions on the low byte of the buttons (the d-pad): steering and
/// the spins and flips.
fn on_dpad(action: usize) -> bool {
    matches!(action, 0 | 1 | 4..=7)
}

pub(super) fn register(r: &mut Registry<Front>) {
    for at in [0x8009_3d00, 0x8009_3d08, 0x8009_48a4] {
        r.add(at, |_: &mut Front, _: &mut Poster| {});
    }
    r.add(0x8009_3fbc, |f: &mut Front, _: &mut Poster| f.controls_enter());
    r.add(0x8009_4164, |f: &mut Front, _: &mut Poster| f.controls_texts());
    r.add(0x8009_4904, |f: &mut Front, _: &mut Poster| {
        for k in LISTS.into_iter().chain(CHOOSING) {
            f.screens[k].reenter_texts(&f.font);
        }
    });
    r.add(0x8009_48ac, |f: &mut Front, _: &mut Poster| {
        for k in LISTS.into_iter().chain(CHOOSING) {
            f.screens[k].lay_out_texts(&f.font);
        }
    });
    r.add(0x8009_449c, |f: &mut Front, _: &mut Poster| {
        for k in [LISTS[0], CHOOSING[0], LISTS[1], CHOOSING[1]] {
            f.screens[k].update(f.clock, &f.font);
        }
    });
    r.add(0x8009_44f4, |f: &mut Front, p: &mut Poster| f.controls_draw(p));
    r.add(0x8009_4f9c, |f: &mut Front, p: &mut Poster| f.controls_pads(p));
    // 0x80094db8: a pad plugged in or out (event 103, which nothing takes).
    r.add(0x8009_4db8, |_: &mut Front, _: &mut Poster| {});
    // Leaving: two actions on one button, an error (event 105); else on
    // (106).
    r.add(0x8009_4b2c, |f: &mut Front, p: &mut Poster| {
        if f.controls_clash() {
            f.play(53);
            f.fade_half();
            p.post(105);
        } else {
            p.post(106);
        }
    });
    r.add(0x8009_4b9c, |f: &mut Front, _: &mut Poster| f.controls.choosing[0] = true);
    r.add(0x8009_4bc8, |f: &mut Front, _: &mut Poster| {
        // Player two joins.
        if f.people == 1 {
            f.people = 2;
            let name = f.players[1].name.clone();
            f.set_text(LISTS[1], 6, &name, 470, 141, 0, 0, true, 1, [127; 3]);
            f.controls_arrows(1);
        }
    });
    r.add(0x8009_4bac, |f: &mut Front, _: &mut Poster| {
        if f.people == 2 {
            f.controls.choosing[1] = true;
        }
    });
    // 0x80087498: every button let go, event 10.
    r.add(0x8008_7498, |f: &mut Front, p: &mut Poster| {
        let held = (0..2).any(|k| (14..27).any(|a| f.pads[k].holds(&f.mappings[k], a)));
        if !held {
            p.post(10);
        }
    });
    r.add(0x8009_46a4, |f: &mut Front, _: &mut Poster| f.controls_step(0, false));
    r.add(0x8009_46fc, |f: &mut Front, _: &mut Poster| f.controls_step(0, true));
    r.add(0x8009_475c, |f: &mut Front, _: &mut Poster| f.controls_step(1, false));
    r.add(0x8009_47b4, |f: &mut Front, _: &mut Poster| f.controls_step(1, true));
    // 0x8001bd18: a pad's buttons back as they came.
    r.add(0x8009_4e5c, |f: &mut Front, _: &mut Poster| f.mappings[0] = Mapping::default());
    r.add(0x8009_4e84, |f: &mut Front, _: &mut Poster| f.mappings[1] = Mapping::default());
    // The motors on or off; turned on, a half-second buzz (0x8001d5a8).
    r.add(0x8009_4d18, |f: &mut Front, _: &mut Poster| f.vibration_toggle(0));
    r.add(0x8009_4d68, |f: &mut Front, _: &mut Poster| f.vibration_toggle(1));
    r.add(0x8009_4c40, |f: &mut Front, p: &mut Poster| f.buzz_over(0, p));
    r.add(0x8009_4cac, |f: &mut Front, p: &mut Poster| f.buzz_over(1, p));
    // "Error! <a> and <b> are same control." (state 216).
    r.add(0x8009_51a0, |f: &mut Front, _: &mut Poster| {
        let (a, b) = f.controls.clash.clone();
        let lines = ["Error!".to_string(), format!("{a} and"), format!("{b} are"), "same control.".to_string()];
        for (i, line) in lines.iter().enumerate() {
            f.set_text(SAME_ERROR, i, line, 320, 165 + 30 * i as i16, 0, 0, true, 1, [255; 3]);
        }
        f.screens[SAME_ERROR].enter(&f.font);
    });
    r.add(0x8009_5318, |f: &mut Front, _: &mut Poster| f.screens[SAME_ERROR].reenter_texts(&f.font));
    r.add(0x8009_5340, |f: &mut Front, p: &mut Poster| {
        f.begin_frame();
        f.fade_step(p);
        f.draw_screen(SAME_ERROR, None, None, false);
        f.end_frame();
    });
    r.add(0x8009_539c, |f: &mut Front, p: &mut Poster| {
        f.pad_events(0, 80, p);
        if f.people == 2 {
            f.pad_events(1, 80, p);
        }
    });
    r.add(0x8009_53d4, |f: &mut Front, _: &mut Poster| f.fade_in());
    // 0x80094814: off to the options, each pad's buttons into its player.
    r.add(0x8009_4814, |f: &mut Front, _: &mut Poster| {
        for k in LISTS.into_iter().chain(CHOOSING) {
            f.screens[k].exit();
        }
        f.fade_out();
        for k in 0..2 {
            f.players[k].mapping = f.mappings[k];
        }
    });
}

impl Front {
    /// 0x8001b528 with 0x8001abb0: the name of the button action `action`
    /// of pad `k` is on (its lowest).
    fn button_name(&self, k: usize, action: usize) -> String {
        let v = self.mappings[k].0.get(action).copied().unwrap_or(0);
        let v = if on_dpad(action) { v << 8 } else { v };
        if v == 0 {
            return String::new();
        }
        self.control_names.buttons.get(v.trailing_zeros() as usize).cloned().unwrap_or_default()
    }

    /// 0x80093fbc: the screen comes on, every list at its top, each
    /// player's name over theirs.
    fn controls_enter(&mut self) {
        self.set_background("psxctrl");
        self.fade_in();
        self.controls = ControlsMenu { top: [0; 2], end: [ROWS; 2], ..ControlsMenu::default() };
        for k in LISTS.into_iter().chain(CHOOSING) {
            self.screens[k].enter(&self.font);
            self.screens[k].clear_texts();
        }
        self.controls_arrows(0);
        self.controls_arrows(1);
        self.helps[CONTROLS_HELP].x = self.helps[CONTROLS_HELP].start << 12;
        let one = self.players[0].name.clone();
        self.set_text(LISTS[0], 6, &one, 200, 141, 0, 0, true, 1, [127; 3]);
        let two = if self.people == 2 { self.players[1].name.clone() } else { String::new() };
        self.set_text(LISTS[1], 6, &two, 470, 141, 0, 0, true, 1, [127; 3]);
    }

    /// 0x80093e78, 0x80093ef0 (by 0x80093d10): a list's arrows, up while
    /// it is scrolled, down while more is below.
    fn controls_arrows(&mut self, k: usize) {
        let c = &self.controls;
        let (up, down) = if k == 1 && self.people != 2 {
            (0, 0)
        } else {
            (if c.top[k] != 0 { 120 } else { 0 }, if c.end[k] < 14 { 100 } else { 0 })
        };
        let at = if k == 0 { [(350, 350), (400, 900)] } else { [(775, 350), (825, 900)] };
        for (name, (x, y), size) in [("arrowu", at[0], up), ("arrowd", at[1], down)] {
            if let Some(p) = self.screens[LISTS[k]].pieces.iter_mut().find(|p| p.model == name) {
                p.size_to = size;
                p.size_now = size;
                p.pos[0] = screen::world_x(x);
                p.pos[1] = screen::world_y(y);
                p.to = p.pos;
            }
        }
    }

    /// 0x80094164: the rows shown, Vibration or the action's name, and
    /// "Choose" by the row waiting for a button.
    fn controls_texts(&mut self) {
        for k in 0..2 {
            if k == 1 && self.people != 2 {
                break;
            }
            let (x, choose_x) = if k == 0 { (240, 175) } else { (510, 600) };
            let (top, end) = (self.controls.top[k], self.controls.end[k]);
            for (i, row) in (top..end).enumerate() {
                let name =
                    if row == 0 { "Vibration".to_string() } else { self.strings.get(69 + row as usize).to_string() };
                let y = 196 + 39 * i as i16;
                self.set_text(LISTS[k], i, &name, x, y, 0, 0, true, 1, [127; 3]);
                self.set_text(CHOOSING[k], i, "", choose_x, y, 0, 0, true, 1, [127; 3]);
            }
        }
        let choose = self.strings.get(99).to_string();
        for (k, x) in [(0, 100), (1, 375)] {
            if self.controls.choosing[k] {
                let row = self.controls.row[k];
                self.set_text(CHOOSING[k], row as usize, &choose, x, 196 + 39 * row as i16, 0, 0, true, 1, [127; 3]);
            }
        }
    }

    /// 0x800944f4: the lists lit on their rows, the help line and the
    /// buttons' icons.
    fn controls_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        let hidden = self.fade.hidden;
        for k in 0..2 {
            let row = (self.controls.row[k] % ROWS) as usize;
            self.draw_screen(LISTS[k], Some(row), None, hidden);
            self.draw_screen(CHOOSING[k], Some(row), None, hidden);
        }
        if !hidden {
            self.draw_help(CONTROLS_HELP);
            self.controls_icons();
        }
        self.end_frame();
    }

    /// 0x800938e8: each row's icon: the motors on or off, or the button the
    /// action is on (none on the row waiting for one).
    fn controls_icons(&mut self) {
        for k in 0..2 {
            if k == 1 && self.people != 2 {
                break;
            }
            let x = if k == 0 { 175 } else { 600 };
            let (top, end) = (self.controls.top[k], self.controls.end[k]);
            for (i, row) in (top..end).enumerate() {
                let y = 430 + 80 * i as i32;
                let model = if row == 0 {
                    Some(if self.mappings[k].0[VIBRATION] != 0 { "onbutt" } else { "offbutt" }.to_string())
                } else if self.controls.choosing[k] && row == self.controls.row[k] + top {
                    None
                } else {
                    let name = self.button_name(k, row as usize - 1);
                    self.control_names.icons.iter().find(|(n, _)| *n == name).map(|(_, m)| m.clone())
                };
                if let Some(model) = model {
                    self.drawing_pieces.push(PieceDraw {
                        model,
                        pos: [screen::world_x(x), screen::world_y(y), 0],
                        turn: [0; 3],
                        scale: 4096,
                    });
                }
            }
        }
    }

    /// 0x80094f9c: each player's presses: waiting for a button, the next
    /// one goes on the action (event 104); on Vibration, Cross turns it
    /// (events 101, 102); else up and down where the list goes on, Cross,
    /// Triangle, Start. Player two only joins (Start) until playing.
    fn controls_pads(&mut self, p: &mut Poster) {
        for k in 0..2 {
            let c = &self.controls;
            if c.choosing[k] {
                self.controls_choose(k, p);
                continue;
            }
            if k == 1 && self.people == 1 {
                self.pad_events(1, 64, p);
                continue;
            }
            let (top, end, row) = (c.top[k], c.end[k], c.row[k]);
            if top == 0 && row == 0 {
                if self.pressed(k, 18) {
                    p.post(if k == 0 { 101 } else { 102 });
                    self.play(50);
                }
                self.pad_events(k, if k == 0 { 98 } else { 2 }, p);
                continue;
            }
            let base = if k == 0 { 112 } else { 16 };
            let mut mask = if top != 0 || row != 0 { base | 1 } else { base };
            if end < 14 || row < 5 {
                mask |= 2;
            }
            self.pad_events(k, mask, p);
        }
    }

    /// 0x80094eac, 0x80094f24 (by 0x8001b640): the buttons pad `k` has
    /// just pressed onto the action its row is: the d-pad for steering and
    /// the stunts (not Select, Start or the stick buttons), any of the rest
    /// for the others.
    fn controls_choose(&mut self, k: usize, p: &mut Poster) {
        let c = &self.controls;
        let action = (c.top[k] + c.row[k]) as usize - 1;
        let buttons = self.pads[k].buttons;
        let (low, high) = ((buttons & 0xff) as u32, (buttons >> 8) as u32);
        let byte = if on_dpad(action) {
            if low == 0 || low & 15 != 0 {
                return;
            }
            low
        } else {
            if high == 0 {
                return;
            }
            high
        };
        if let Some(m) = self.mappings[k].0.get_mut(action) {
            *m = byte;
        }
        self.controls.choosing[k] = false;
        p.post(104);
    }

    /// 0x800946a4 and its neighbours: the row up or down, the list
    /// scrolling at its ends (player two's one row shorter, as in the
    /// original).
    fn controls_step(&mut self, k: usize, down: bool) {
        let c = &mut self.controls;
        if down {
            let last = if k == 0 { 15 } else { 14 };
            if c.row[k] < 5 {
                c.row[k] += 1;
            } else if c.end[k] < last {
                c.end[k] += 1;
                c.top[k] += 1;
            }
        } else if c.row[k] != 0 {
            c.row[k] -= 1;
        } else if c.top[k] != 0 {
            c.top[k] -= 1;
            c.end[k] -= 1;
        }
        self.controls_arrows(k);
    }

    /// 0x80094d18: pad `k`'s motors on or off.
    fn vibration_toggle(&mut self, k: usize) {
        let m = &mut self.mappings[k].0[VIBRATION];
        *m = (*m == 0) as u32;
        self.controls.buzz_from = self.clock;
        if *m != 0 {
            self.buzz = Some(k as u8);
        }
    }

    /// 0x80094c40: with the motors off, or the buzz over, event 10.
    fn buzz_over(&mut self, k: usize, p: &mut Poster) {
        if self.mappings[k].0[VIBRATION] == 0 || self.clock.wrapping_sub(self.controls.buzz_from) > 500 {
            p.post(10);
        }
    }

    /// 0x8009495c: two actions on one button that are not a pair meant to
    /// share it (steering and the spin that way, accelerate and back flip,
    /// brake and front flip), for either player; their names noted.
    fn controls_clash(&mut self) -> bool {
        for i in 0..ACTIONS - 1 {
            for j in i + 1..ACTIONS {
                for k in 0..self.people.min(2) as usize {
                    if self.button_name(k, i) == self.button_name(k, j) && self.control_names.shares[i] as usize != j {
                        self.controls.clash =
                            (self.strings.get(70 + i).to_string(), self.strings.get(70 + j).to_string());
                        return true;
                    }
                }
            }
        }
        false
    }
}
