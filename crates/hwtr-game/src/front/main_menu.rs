//! The main menu (states 63 to 100).

use super::*;

pub(super) fn register(r: &mut Registry<Front>) {
    r.add(0x8008_c544, |f: &mut Front, _: &mut Poster| f.show_car(0));
    r.add(0x8008_c668, |f: &mut Front, _: &mut Poster| f.show_car(1));
    r.add(0x8008_bb38, |f: &mut Front, _: &mut Poster| f.car_changed[0] = f.clock);
    r.add(0x8008_bb64, |f: &mut Front, _: &mut Poster| f.car_changed[1] = f.clock);
    r.add(0x8008_bf18, |f: &mut Front, _: &mut Poster| f.main_menu_enter());
    r.add(0x8008_c7fc, |f: &mut Front, _: &mut Poster| {
        let t = f.players[0].track as usize;
        f.track_model = f.tables.track_files.get(t).cloned().flatten();
    });
    r.add(0x8008_c930, |f: &mut Front, _: &mut Poster| f.main_menu_texts());
    r.add(0x8008_83fc, |f: &mut Front, _: &mut Poster| f.menu_idle_from = f.clock);
    r.add(0x8008_c8e0, |f: &mut Front, _: &mut Poster| f.screens[MAIN_MENU].reenter_texts(&f.font));
    r.add(0x8008_cfcc, |f: &mut Front, _: &mut Poster| f.screens[MAIN_MENU].update(f.clock, &f.font));
    r.add(0x8008_cccc, |f: &mut Front, p: &mut Poster| f.main_menu_draw(p));
    r.add(0x8008_d264, |f: &mut Front, p: &mut Poster| f.main_menu_pads(p));
    r.add(0x8008_bb90, |f: &mut Front, p: &mut Poster| f.car_preview_timers(p));
    // 0x80088428: thirty seconds untouched, the attract race (event 37).
    r.add(0x8008_8428, |f: &mut Front, p: &mut Poster| {
        if f.menu_idle_from.wrapping_add(30_000) < f.clock {
            p.post(37);
        }
    });
    r.add(0x8008_d948, |f: &mut Front, p: &mut Poster| f.main_menu_choose(0x8008_d948, p));
    r.add(0x8008_d9f8, |f: &mut Front, p: &mut Poster| f.main_menu_choose(0x8008_d9f8, p));
    r.add(0x8008_d35c, |f: &mut Front, _: &mut Poster| f.menu_step(false));
    r.add(0x8008_d470, |f: &mut Front, _: &mut Poster| f.menu_step(true));
    r.add(0x8008_d700, |f: &mut Front, p: &mut Poster| {
        p.post(match f.choice {
            1 => 56,
            2 => 57,
            3 => 59,
            5 => 58,
            _ => 60,
        })
    });
    // Player two's left and right on the car line (states 71, 72, 80 to
    // 83): with two playing, the car turns away (as player one's does)
    // and the next car is chosen; else nothing.
    r.add(0x8008_b980, |_: &mut Front, _: &mut Poster| {});
    r.add(0x8008_d590, |f: &mut Front, p: &mut Poster| p.post(if f.people == 2 && f.choice == 1 { 54 } else { 55 }));
    r.add(0x8008_c460, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[1] {
            f.preview_aim(1, 775, 450, 0);
        }
    });
    r.add(0x8008_d5e4, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.car_step(1, false);
    });
    r.add(0x8008_d620, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.car_step(1, true);
    });
    r.add(0x8008_c3f4, |f: &mut Front, p: &mut Poster| {
        if !f.car_loaded[0] {
            p.post(10);
        } else {
            f.preview_full(0);
        }
    });
    r.add(0x8008_c42c, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[0] {
            f.preview_aim(0, 435, 450, 0);
        }
    });
    r.add(0x8008_c4dc, |f: &mut Front, p: &mut Poster| {
        f.preview_rest(0);
        p.post(10);
    });
    r.add(0x8008_c34c, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[0] {
            f.car_loaded[0] = false;
        }
    });
    r.add(0x8008_c3a0, |f: &mut Front, _: &mut Poster| {
        if f.car_loaded[1] {
            f.car_loaded[1] = false;
        }
    });
    r.add(0x8008_c78c, |f: &mut Front, _: &mut Poster| f.car_shown[0] = false);
    r.add(0x8008_c7c4, |f: &mut Front, _: &mut Poster| f.car_shown[1] = false);
    r.add(0x8008_c894, |f: &mut Front, _: &mut Poster| f.track_model = None);
    r.add(0x8008_d7a0, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.car_step(0, false);
    });
    r.add(0x8008_d7dc, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.car_step(0, true);
    });
    r.add(0x8008_d8d0, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.track_step(false);
    });
    r.add(0x8008_d90c, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.track_step(true);
    });
    r.add(0x8008_d818, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.settings.mode = if f.settings.mode == 0 { 4 } else { f.settings.mode - 1 };
    });
    r.add(0x8008_d868, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.settings.mode = if f.settings.mode >= 4 { 0 } else { f.settings.mode + 1 };
    });
    r.add(0x8008_d65c, |f: &mut Front, _: &mut Poster| {
        f.play(48);
        f.sign_in = if f.sign_in == 0 { 2 } else { f.sign_in - 1 };
    });
    r.add(0x8008_d6a8, |f: &mut Front, _: &mut Poster| {
        f.play(49);
        f.sign_in = if f.sign_in >= 2 { 0 } else { f.sign_in + 1 };
    });
    r.add(0x8008_c908, |f: &mut Front, _: &mut Poster| f.screens[MAIN_MENU].lay_out_texts(&f.font));
    r.add(0x8008_c1f0, |f: &mut Front, _: &mut Poster| {
        f.preview_place(0, 435, 450, 0);
        f.preview_aim(0, 435, 450, 0);
        f.previews[0].size_to(0x1800);
    });
    r.add(0x8008_c238, |f: &mut Front, _: &mut Poster| {
        f.preview_place(1, 775, 450, 0);
        f.preview_aim(1, 775, 450, 0);
        f.previews[1].size_to(0x1800);
    });
    r.add(0x8008_c124, |f: &mut Front, _: &mut Poster| f.ask_car(0));
    r.add(0x8008_c280, |f: &mut Front, _: &mut Poster| f.ask_car(1));
}
