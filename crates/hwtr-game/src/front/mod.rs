//! The front end: the boot checks, the memory card dialog, the title, the
//! main menu and the way into a race and back, run as the original runs
//! them, by its main state machine (`fsm_main`, 0x800c5bdc, 349 states)
//! read out of the executable.
//!
//! The states name their actions by the original's function addresses;
//! [`Front::act`] is where each one is ported. An action not ported yet is
//! reported once and does nothing. What the front end asks of the rest of
//! the game (a race, a sound) it leaves in [`Front`] for the program to
//! pick up, as the original calls through its interface table.
//!
//! The memory card is the game's own card file kept as a file
//! ([`card`]): always in, always holding a Hot Wheels file (a new one the
//! first time), so the game boots straight to its title as with a card
//! that has a save.
//!
//! Not drawn yet: the 3D pieces (the dialog box, the arrows, the button
//! icons, the car and track previews); their state is kept so the flow is
//! the original's.

pub mod card;
pub mod font;
pub mod password;
pub mod screen;
pub mod strings;

mod boot;
mod card_menu;
mod controls;
mod credits;
mod cup;
mod garage;
mod hiscores;
mod main_menu;
mod options;
mod race_start;
mod results;
mod sign_in;
mod title;
mod unlocks;

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::fsm::{self, Def, Fsm, Kit, Poster, Registry};
use crate::hud::Font;
use crate::math::fx;
use crate::pad::{Mapping, PadState};
use crate::race::{Driver, Entrant, RaceSetup};
use crate::rand::Rand;
use card::Save;
use font::ScreenFont;
pub use results::{CarResult, RaceResult};
use screen::{Glyph, Line, PieceDraw, Screen};
use strings::{CarFacts, Strings};

/// The main state machine's record.
pub const FSM_MAIN: u32 = 0x800c_5bdc;

/// Screens by their place in the table.
const POPUP: usize = 0;
const TITLE: usize = 1;
const PASSWORD: usize = 2;
const SAVE_PROMPT: usize = 3;
const MAIN_MENU: usize = 4;

/// The help line along the bottom of the main menu (0x800c2714).
const MAIN_HELP: u32 = 0x800c_2714;

/// The difficulty's byte in `TUNING.PRM`.
const DIFFICULTY: usize = 0x20;

/// What the original's clock adds each vertical blank.
pub const FRAME_MS: u32 = 17;

/// At most this many state machine steps a blank when none draws.
const STEPS_A_BLANK: usize = 64;

/// A player's record (0x801397a4, 0x80139864; 0xc0 bytes): the name, what
/// is unlocked, the car and track last chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The record's first word, and its id (+4), by which the settings
    /// name the players last playing.
    pub tag: u32,
    pub id: u32,
    pub name: String,
    /// Cars 0 to 31 (+0x14) and 32 on (+0x18), a bit each.
    pub cars: [u32; 2],
    /// Tracks, a bit each (+0x1c).
    pub tracks: u32,
    pub car: u8,
    pub track: u8,
    /// +0x22, +0x23 and the six words from +0x24: progress the password
    /// carries.
    pub progress: [u8; 2],
    pub records: [u32; 6],
    /// Cheats in effect (+0x3c).
    pub cheats: u32,
    /// The player's buttons (+0x40).
    pub mapping: Mapping,
}

impl Profile {
    /// 0x80088474: "PLAYER n" with the cars and tracks a new player has.
    pub fn new(strings: &Strings, k: u8) -> Profile {
        Profile {
            tag: 0,
            id: 0,
            mapping: Mapping::default(),
            name: format!("{} {}", strings.get(83), k + 1),
            cars: [0xb48e_e52e, 265],
            tracks: 63,
            car: 29,
            track: 0,
            progress: [0; 2],
            records: [0; 6],
            cheats: 0,
        }
    }

    /// What a password made from the record would say (0x8006a140): two
    /// records with the same say the same.
    fn password_key(&self) -> PasswordKey {
        (self.cars[0], self.cars[1], self.tracks & 0xffff, self.progress, self.records)
    }
}

/// What a player's password encodes: the cars, the tracks, progress and
/// records.
type PasswordKey = (u32, u32, u32, [u8; 2], [u32; 6]);

/// The game's settings (0x80138f14 from +0x874).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// +0x875: practice, practice airtime, exhibition, airtime challenge,
    /// the cup.
    pub mode: u8,
    /// +0x878: the computer cars' skill, which the race gets.
    pub difficulty: u8,
    /// +0x876, +0x877, +0x87a: the volumes.
    pub volume: u8,
    pub music: u8,
    pub effects: u8,
    /// +0x874, +0x879.
    pub other: [u8; 2],
    /// +0x87b.
    pub spare: u8,
}

impl Settings {
    /// 0x80088544 (with the 60 high-score names it also clears left out);
    /// the difficulty comes from `TUNING.PRM` (0x80136a38).
    pub fn new(difficulty: u8) -> Settings {
        Settings { mode: 2, difficulty, volume: 200, music: 255, effects: 255, other: [0, 1], spare: 0 }
    }
}

/// What the executable's tables say about cars and tracks.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// The cars' file names (0x800c5ce8) and display names (0x800c5d8c).
    pub car_files: Vec<Option<String>>,
    pub car_names: Vec<String>,
    /// Each car's rating (0x800c289c), which the computer field averages.
    pub car_ratings: Vec<u16>,
    /// By world and number: file names (0x800c5bf4) and titles (0x800c5c34).
    pub track_files: Vec<Option<String>>,
    pub track_names: Vec<String>,
    /// The worlds (0x800c5c24) and the checkpoints of each track (0x800c5c64).
    pub worlds: Vec<String>,
    pub checkpoints: Vec<u8>,
    /// The sign-in line's string by state (0x800d116c).
    pub sign_in: [u16; 3],
    /// Each track's own song (0x800be8c0).
    pub track_songs: Vec<u8>,
    /// The cheat codes, by bit (0x800bee6c), and the one car code
    /// (0x800d1004) with its car's file name (0x800d1008).
    pub cheat_codes: Vec<String>,
    pub car_code: (String, String),
}

impl Tables {
    pub fn read(byte: &dyn Fn(u32) -> u8) -> Tables {
        let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let cstr = |a: u32| -> Option<String> {
            if a == 0 {
                return None;
            }
            let mut s = String::new();
            let mut p = a;
            while byte(p) != 0 && s.len() < 40 {
                s.push(byte(p) as char);
                p += 1;
            }
            Some(s)
        };
        let strs = |at: u32, n: u32| (0..n).map(|k| cstr(word(at + 4 * k))).collect::<Vec<_>>();
        Tables {
            car_files: strs(0x800c_5ce8, 42),
            car_names: strs(0x800c_5d8c, 42).into_iter().map(Option::unwrap_or_default).collect(),
            car_ratings: (0..42)
                .map(|k| u16::from_le_bytes([byte(0x800c_289c + 2 * k), byte(0x800c_289d + 2 * k)]))
                .collect(),
            track_files: strs(0x800c_5bf4, 12),
            track_names: strs(0x800c_5c34, 12).into_iter().map(Option::unwrap_or_default).collect(),
            worlds: strs(0x800c_5c24, 4).into_iter().map(Option::unwrap_or_default).collect(),
            checkpoints: (0..12).map(|k| byte(0x800c_5c64 + k)).collect(),
            sign_in: std::array::from_fn(|k| {
                u16::from_le_bytes([byte(0x800d_116c + 2 * k as u32), byte(0x800d_116d + 2 * k as u32)])
            }),
            track_songs: (0..12).map(|k| byte(0x800b_e8c0 + k)).collect(),
            cheat_codes: strs(0x800b_ee6c, 10).into_iter().map(Option::unwrap_or_default).collect(),
            car_code: (cstr(word(0x800d_1004)).unwrap_or_default(), cstr(word(0x800d_1008)).unwrap_or_default()),
        }
    }

    /// 0x8007f87c: a pad's last eight code buttons (`code`, '1' to '6')
    /// against the cheat codes, then the car code. True if one matched.
    ///
    /// - 0x8007f710: the first cheat code that matches makes `p`'s cheats
    ///   its bit alone. Several slots hold the same unused code, so only
    ///   the first of them can be had.
    /// - 0x8007f79c: the car code gives `p` its car (by its file name,
    ///   0x80088370).
    pub fn apply_code(&self, code: &[u8], p: &mut Profile) -> bool {
        if let Some(bit) = self.cheat_codes.iter().position(|c| c.as_bytes() == code) {
            p.cheats = 1 << bit;
            return true;
        }
        if self.car_code.0.as_bytes() != code {
            return false;
        }
        // 0x80088370: the first 41 file names; -1 if none (a shift by 31).
        let id = self.car_files[..41]
            .iter()
            .position(|f| f.as_deref() == Some(self.car_code.1.as_str()))
            .map_or(-1, |k| k as i32);
        if id < 32 {
            p.cars[0] |= 1u32.wrapping_shl(id as u32);
        } else {
            p.cars[1] |= 1 << (id - 32);
        }
        true
    }
}

/// The fade (0x8008adc0): whether it goes in, out or to half, how far it
/// is (0 to 255), and whether the screen's texts are hidden meanwhile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Fade {
    way: u8,
    level: u8,
    hidden: bool,
}

/// The scrolling help line (0x800c2714 and its neighbours, 24 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Help {
    string: u16,
    left: i32,
    right: i32,
    y: i32,
    start: i32,
    /// 20.12.
    x: i32,
}

impl Help {
    fn read(byte: &dyn Fn(u32) -> u8, at: u32) -> Help {
        let word = |a: u32| i32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        Help {
            string: word(at) as u16,
            left: word(at + 4),
            right: word(at + 8),
            y: word(at + 12),
            start: word(at + 16),
            x: word(at + 20),
        }
    }
}

/// What the front end draws this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    /// The background picture's name ("psxmain2") and how bright it is (128
    /// is as it is).
    pub background: Option<(String, u8)>,
    /// The 3D pieces, in the order drawn, under the letters, and the track
    /// map.
    pub pieces: Vec<PieceDraw>,
    pub track: Option<TrackDraw>,
    /// The car previews, over the pieces.
    pub cars: Vec<CarDraw>,
    /// The cars' pictures (`DECALS.BMF`), shown until their models are in.
    pub decals: Vec<DecalDraw>,
    pub glyphs: Vec<Glyph>,
}

/// How a race the front end started ended (the byte the race leaves at
/// 0x80138d14): run to the end, restarted, or quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceEnd {
    Finished,
    Restart,
    Other(u8),
}

impl RaceEnd {
    fn code(self) -> u8 {
        match self {
            RaceEnd::Finished => 1,
            RaceEnd::Restart => 2,
            RaceEnd::Other(n) => n,
        }
    }
}

pub struct Front {
    def: Rc<Def<u32>>,
    /// The ported actions, by address.
    actions: Rc<Registry<Front>>,
    fsm: Fsm,
    pub screens: Vec<Screen>,
    font: ScreenFont,
    pub strings: Strings,
    pub facts: CarFacts,
    pub tables: Tables,
    pub passwords: password::Passwords,
    /// The sign-in board (0x800d274e on), its letters for names and
    /// passwords, and its keys' places.
    board: sign_in::Board,
    name_keys: Vec<u8>,
    password_keys: Vec<u8>,
    key_places: Vec<(i32, i32)>,
    pub rand: Rand,
    /// The system clock, ms (0x800d240c); the program advances it.
    pub clock: u32,
    pads: [PadState; 2],
    /// Each pad's buttons (control_mapping, 0x8011b3d8): the players'
    /// own, set at boot and on the controls screen.
    pub mappings: [Mapping; 2],
    /// Which front-end actions (14 to 27) each pad held at its last look,
    /// for presses (0x800bfe2c).
    held: [[bool; 14]; 2],
    /// The last eight cheat-code buttons of each pad (0x80136c70).
    codes: [[u8; 9]; 2],
    pub players: [Profile; 2],
    pub settings: Settings,
    /// The attract race was let go before it ended (0x8009b510): the
    /// host drops it.
    pub race_dropped: bool,
    default_difficulty: u8,
    /// How many play (0x800d2740).
    pub people: u8,
    /// The main menu's choice (0x800d1173) and the dialog's (0x800d275c).
    pub choice: u8,
    popup_choice: u8,
    sign_in: u8,
    /// Where the sign-in screen returns (0x800d2749).
    sign_in_back: bool,
    /// Where the title goes (0x800d1169).
    title_to_cup: bool,
    fade: Fade,
    /// The highlight's grey, cycling (0x800d10ba), and the timers of the
    /// slot clocks (0x800bfe04).
    flash: u8,
    clocks: [u32; 16],
    /// When the player last did anything (0x800d2738, 0x800d1180).
    idle_from: u32,
    menu_idle_from: u32,
    /// The race-start wait (0x800d2790) and the title's first frame.
    wait_from: u32,
    title_sound: bool,
    music_started: bool,
    /// Holding left or right: each pad's repeat delay and last step
    /// (0x800d2780, 0x800d2788), and whether it is held (0x800d11ee).
    repeat: [(u32, u32); 2],
    stepping: [bool; 2],
    /// Which pad the main menu reads first this frame (0x800d11f0).
    second_first: bool,
    /// The car previews: asked for, loaded (0x800d2796, 0x800d2794 and the
    /// second player's), when last changed (0x800d279c), shown (0x800d2798).
    car_asked: [bool; 2],
    car_loaded: [bool; 2],
    car_changed: [u32; 2],
    car_shown: [bool; 2],
    /// The passwords last saved (0x800c27ec), as what they encode.
    saved: [Option<PasswordKey>; 2],
    saving: [bool; 2],
    /// The password shown: since when (0x800d2774), and whose.
    password_from: u32,
    password_of: u8,
    pub background: Option<String>,
    helps: Vec<Help>,
    /// A frame was finished this blank.
    frame_done: bool,
    /// The frame being drawn and the last one finished.
    drawing: Vec<Glyph>,
    drawing_pieces: Vec<PieceDraw>,
    drawing_track: Option<TrackDraw>,
    drawing_cars: Vec<CarDraw>,
    drawing_decals: Vec<DecalDraw>,
    /// The car whose picture each player's slot holds (0x8011c7b0, loaded
    /// by 0x8008c544 and 0x8009a05c).
    decal_car: [Option<u8>; 2],
    /// Each player's car preview.
    pub(super) previews: [CarPreview; 2],
    /// The main menu's track map: its model (0x8008c7fc) and how it turns,
    /// grows and moves.
    track_model: Option<String>,
    track_map: TrackMap,
    /// The track map's slide clock (0x800bfddc).
    track_moved: u32,
    pub shown: Frame,
    /// Sound effects asked for this frame (interface 0x8011df44: an effect
    /// and its volume, 0 to 127), and the music started (true) or stopped
    /// (false).
    pub sounds: Vec<(u8, u8)>,
    pub music: Option<bool>,
    /// The CD asked for this blank (taken by whoever plays it).
    pub cd: Vec<crate::cd::CdAsk>,
    /// How the race's song is chosen (0x800d276d: 0 at random, 1 the
    /// track's own, 2 the one chosen) and the song (0x800d11a0).
    pub music_mode: u8,
    pub song: u8,
    /// The CD's songs, artist and title.
    songs: Vec<(String, String)>,
    /// The options screen's line (0x800d1175).
    option_line: u8,
    /// The credits: their picture (0x800d27b8), page (0x800d1176), and
    /// each page's scrolling lines.
    credits_picture: u8,
    credits_page: u8,
    scrollers: Vec<credits::Scroller>,
    /// The Hi-Scores screen, and the one-second clock (0x800d273c).
    hs: hiscores::HiScores,
    second_from: u32,
    /// The garage, and each car's qualities by car (CWHS block A +0x130:
    /// top speed, durability, stunts, class) and each player's car's as
    /// shown (0x800d2754: top speed, stunts, durability, class; class -1
    /// for a locked car).
    garage: garage::Garage,
    qualities: Vec<[u8; 4]>,
    car_stats: [[i16; 4]; 2],
    /// A race set up and waiting to be run, and how the last one ended.
    pub race: Option<RaceSetup>,
    race_end: Option<RaceResult>,
    /// The last race set up (its record at 0x80138c94), for its results.
    last_race: Option<RaceSetup>,
    /// What the last results unlocked that the players did not have.
    unlocked: results::Unlocked,
    /// The cups' screens, whether the cup screen is up (0x800d1169), the
    /// cup just won (0x800d1168), the cup round's result, the sign-in
    /// choice on the cup screen (0x800d116b).
    cup: cup::CupScreens,
    in_cup: bool,
    cup_won: bool,
    cup_result: Option<RaceResult>,
    sign_in_choice: u8,
    /// The unlock being announced, and a car a preview shows in place of
    /// the player's (an unlocked one).
    announce: unlocks::Announce,
    preview_car: [Option<u8>; 2],
    /// The cups' tracks (0x800c2560), and each track's difficulty and
    /// strategy (0x800c5c70, 0x800c5ca0).
    cup_tracks: Vec<i8>,
    track_difficulty: Vec<String>,
    track_strategy: Vec<String>,
    pub unported: BTreeSet<u32>,
    /// What the memory card holds, and whether it should be written out.
    /// The settings and tables in play (0x80138f14).
    pub card: Save,
    /// The memory cards in slots 1 and 2, each as it holds the game's save
    /// (none: no card the game can use), and which are to be written.
    pub slots: [Option<Save>; 2],
    pub save_due: [bool; 2],
    /// The card slot each player's records are on (0x800d1188), and the
    /// Load/Save screen's state.
    card_place: [u8; 2],
    cards_ui: card_menu::CardMenu,
    /// What each slot holds, whether its card has not been set aside
    /// (0x800d0f28), and the slot a card dialog is about (0x800d276c).
    card_status: [boot::CardStatus; 2],
    card_seen: [bool; 2],
    trouble_slot: u8,
    /// The controls screen's state and the buttons' names; a pad to buzz
    /// for half a second (0x8001d5a8), for the program to do.
    controls: controls::ControlsMenu,
    control_names: controls::ControlNames,
    pub buzz: Option<u8>,
    /// The save prompt's choice (0x800d277c).
    save_choice: u8,
}

/// Everything the front end reads from the disc.
pub struct Files<'a> {
    pub strings: &'a [u8],
    /// `TUNING.PRM` (0x80136a18), whose byte 0x20 is the difficulty a new
    /// game starts with.
    pub tuning: &'a [u8],
    pub cars: &'a [u8],
    pub font: &'a Font,
    /// The memory card files in slots 1 and 2, if there are any, and
    /// whether each slot has a card.
    pub card: Option<&'a [u8]>,
    pub card2: Option<&'a [u8]>,
    pub card_slots: [bool; 2],
    /// `CWHS.BMF`: every car's handling, whose block A the garage reads
    /// the cars' qualities from (+0x130).
    pub cwhs: Option<&'a [u8]>,
    /// `ENGNAME.CHM` and `ENGPWD.CHM`: the letters names and passwords are
    /// typed with.
    pub name_keys: Option<&'a [u8]>,
    pub password_keys: Option<&'a [u8]>,
}

impl Front {
    /// The front end at boot, from the executable (`byte` reads it) and the
    /// files.
    pub fn new(byte: &dyn Fn(u32) -> u8, files: Files, seed: u32) -> Option<Front> {
        let def = fsm::read_def_bytes(byte, FSM_MAIN)?;
        let strings = Strings::parse(files.strings)?;
        let font = ScreenFont::new(files.font, byte);
        let screens = screen::read_screens(byte, &strings);
        let fsm = Fsm::new(&def);
        let players = [Profile::new(&strings, 0), Profile::new(&strings, 1)];
        let difficulty = files.tuning.get(DIFFICULTY).copied().unwrap_or(0);
        let rand = Rand { seed };
        // Each slot: no card, a card without the file, a damaged save, or
        // the save.
        let mut card_status = [boot::CardStatus::Missing; 2];
        let mut slots: [Option<Save>; 2] = [None, None];
        for (k, file) in [files.card, files.card2].into_iter().enumerate() {
            if !files.card_slots[k] {
                continue;
            }
            match file.map(card::read_file) {
                None => card_status[k] = boot::CardStatus::NoFile,
                Some(None) => card_status[k] = boot::CardStatus::Damaged,
                Some(Some(save)) => {
                    card_status[k] = boot::CardStatus::Ready;
                    slots[k] = Some(save);
                }
            }
        }
        let save_due = [false; 2];
        let card = slots
            .iter()
            .flatten()
            .next()
            .cloned()
            .unwrap_or_else(|| Save::new(0, strings.get(84), Settings::new(difficulty)));
        Some(Front {
            actions: Rc::new(registry()),
            card,
            slots,
            save_due,
            card_place: [0; 2],
            card_status,
            card_seen: [true; 2],
            trouble_slot: 0,
            cards_ui: card_menu::CardMenu::default(),
            controls: controls::ControlsMenu::default(),
            control_names: controls::ControlNames::read(byte),
            buzz: None,
            save_choice: 0,
            def: Rc::new(def),
            fsm,
            screens,
            font,
            facts: CarFacts::parse(files.cars),
            tables: Tables::read(byte),
            passwords: password::Passwords::new(byte),
            board: sign_in::Board::default(),
            name_keys: files.name_keys.map(sign_in::chm_letters).unwrap_or_default(),
            password_keys: files.password_keys.map(sign_in::chm_letters).unwrap_or_default(),
            key_places: sign_in::key_places(byte),
            strings,
            rand,
            clock: 0,
            pads: [PadState::default(); 2],
            mappings: [Mapping::default(); 2],
            held: [[false; 14]; 2],
            codes: [[0; 9]; 2],
            players,
            settings: Settings::new(difficulty),
            race_dropped: false,
            default_difficulty: difficulty,
            people: 1,
            choice: byte(0x800d_1173),
            popup_choice: 0,
            sign_in: byte(0x800d_116a),
            sign_in_back: false,
            title_to_cup: byte(0x800d_1169) != 0,
            fade: Fade::default(),
            flash: 0,
            clocks: [0; 16],
            idle_from: 0,
            menu_idle_from: 0,
            wait_from: 0,
            title_sound: false,
            music_started: false,
            repeat: [(0, 0); 2],
            stepping: [false; 2],
            second_first: false,
            car_asked: [false; 2],
            car_loaded: [false; 2],
            car_changed: [0; 2],
            car_shown: [false; 2],
            saved: [None, None],
            saving: [false; 2],
            password_from: 0,
            password_of: 0,
            background: None,
            helps: (0..9).map(|k| Help::read(byte, MAIN_HELP + 24 * k)).collect(),
            frame_done: false,
            drawing: Vec::new(),
            drawing_pieces: Vec::new(),
            drawing_track: None,
            drawing_cars: Vec::new(),
            drawing_decals: Vec::new(),
            decal_car: [None; 2],
            previews: [CarPreview::new(); 2],
            track_model: None,
            track_map: TrackMap::default(),
            track_moved: 0,
            shown: Frame::default(),
            sounds: Vec::new(),
            music: None,
            cd: Vec::new(),
            music_mode: 0,
            song: 0,
            songs: crate::cd::songs(byte),
            option_line: 0,
            credits_picture: 1,
            credits_page: 0,
            hs: Default::default(),
            second_from: 0,
            garage: Default::default(),
            qualities: files
                .cwhs
                .and_then(|b| bmf_parts(b))
                .map(|parts| {
                    parts.iter().map(|p| std::array::from_fn(|k| p.get(4 + 0x130 + k).copied().unwrap_or(0))).collect()
                })
                .unwrap_or_default(),
            car_stats: [[0, 0, 0, -1]; 2],
            scrollers: credits::SCROLLERS_AT.iter().map(|&a| credits::Scroller::read(byte, a)).collect(),
            race: None,
            race_end: None,
            last_race: None,
            unlocked: Default::default(),
            cup: Default::default(),
            in_cup: false,
            cup_won: false,
            cup_result: None,
            sign_in_choice: 0,
            announce: Default::default(),
            preview_car: [None; 2],
            cup_tracks: (0..27).map(|k| byte(cup::CUP_TRACKS + k) as i8).collect(),
            track_difficulty: pointer_strings(byte, 0x800c_5c70, 12),
            track_strategy: pointer_strings(byte, 0x800c_5ca0, 12),
            unported: BTreeSet::new(),
        })
    }

    /// The state the machine is in.
    pub fn state(&self) -> usize {
        self.fsm.current
    }

    /// The actions the machine names that are not ported yet.
    pub fn unported_actions(&self) -> std::collections::BTreeSet<u32> {
        self.actions.missing(&self.def)
    }

    /// One vertical blank. The original's main loop (0x80010a5c) steps the
    /// state machine as fast as it can and only waits for the blank when a
    /// frame is finished (0x800839f8), so every step until the next frame
    /// is drawn happens within one blank, at one time on the clock. Steps
    /// that never draw (loading, waiting on a race) are cut off at
    /// `STEPS_A_BLANK`.
    pub fn tick(&mut self, pads: [PadState; 2]) {
        self.pads = pads;
        self.sounds.clear();
        self.music = None;
        self.frame_done = false;
        self.run_blank(STEPS_A_BLANK);
    }

    /// The race the front end set up has ended.
    pub fn race_over(&mut self, result: RaceResult) {
        self.race_end = Some(result);
    }

    /// 0x8008a668: the music on, if it is not already (0x800d119f).
    fn start_music(&mut self) {
        if !self.music_started {
            self.music_started = true;
            self.music = Some(true);
        }
    }

    fn note_unported(&mut self, addr: u32) {
        if self.unported.insert(addr) {
            tracing::warn!("front end: action {addr:#010x} (state {}) is not ported", self.fsm.current);
        }
    }

    // --- Framework -------------------------------------------------------

    /// 0x80088100: every screen's texts into its letters.
    fn copy_all_letters(&mut self) {
        for s in &mut self.screens {
            for t in &mut s.texts {
                t.copy_letters();
            }
        }
    }

    /// 0x800832f0: the font's kerning scaled, every screen laid out, the
    /// boxes and labels set to come on, the slide clocks at zero.
    fn boot_screens(&mut self) {
        self.font.prepare();
        for s in &mut self.screens {
            s.boot(&self.font);
            s.moved = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn set_text(
        &mut self,
        screen: usize,
        k: usize,
        text: &str,
        x: i16,
        y: i16,
        extra: i16,
        entry: u8,
        centred: bool,
        font: u8,
        colour: [u8; 3],
    ) {
        if let Some(t) = self.screens[screen].texts.get_mut(k) {
            t.set(&Line { text, x, y, extra, entry, centred, font, colour });
        }
    }

    /// 0x800834f4: slot `k`'s clock: the seconds since it last ran (4.12),
    /// half a second after a pause, 0.15 the first time.
    fn seconds(&mut self, k: usize) -> i32 {
        let last = self.clocks[k];
        let now = self.clock;
        if last == 0 {
            self.clocks[k] = now;
            return 0xf000 * 4096 / 0x64000;
        }
        let dt = now.wrapping_sub(last);
        self.clocks[k] = now;
        if dt < 501 { ((dt << 12) / 1000) as i32 } else { 4096 / 2 }
    }

    /// 0x800838fc: a frame begins (the background is drawn under it).
    fn begin_frame(&mut self) {
        self.drawing.clear();
        self.drawing_pieces.clear();
        self.drawing_track = None;
        self.drawing_cars.clear();
        self.drawing_decals.clear();
    }

    /// 0x80023660 as the menus call it: player `k`'s car picture at
    /// (`x`, `y`) while it waits for the model (0x800d2798), unless faded.
    fn draw_decal(&mut self, k: usize, x: i16, y: i16) {
        if let Some(car) = self.decal_car[k]
            && self.car_shown[k]
            && !self.fade.hidden
        {
            self.drawing_decals.push(DecalDraw { slot: k as u8, car, x, y });
        }
    }

    /// 0x800839f8: the frame is shown.
    fn end_frame(&mut self) {
        self.frame_done = true;
        let shade = fade_shade(self.fade.level);
        self.shown = Frame {
            background: self.background.clone().map(|b| (b, shade)),
            pieces: std::mem::take(&mut self.drawing_pieces),
            track: self.drawing_track.take(),
            cars: std::mem::take(&mut self.drawing_cars),
            decals: std::mem::take(&mut self.drawing_decals),
            glyphs: std::mem::take(&mut self.drawing),
        };
    }

    /// 0x80086268: screen `k` with its chosen text or label.
    fn draw_screen(&mut self, k: usize, text: Option<usize>, label: Option<usize>, hidden: bool) {
        let s = self.seconds(8);
        self.flash = self.flash.wrapping_add((fx(s, 0x1c_2000) >> 12) as u8);
        self.screens[k].draw(text, label, self.flash, hidden, &mut self.drawing, &mut self.drawing_pieces);
    }

    /// 0x8008aef0: one of the three versions of a background, at random.
    fn set_background(&mut self, name: &str) {
        let k = self.rand.below(3) + 1;
        self.background = Some(format!("{name}{k}"));
    }

    /// Effect `id` at full volume (0x80015908 with 4096).
    fn play(&mut self, id: u8) {
        self.sounds.push((id, 127));
    }

    /// 0x8008ad7c, 0x8008ad90.
    fn fade_in(&mut self) {
        self.fade.way = 1;
        self.fade.hidden = false;
    }

    fn fade_out(&mut self) {
        self.fade.way = 2;
        self.fade.hidden = true;
    }

    /// 0x8008ada8: the picture to half, the texts hidden.
    fn fade_half(&mut self) {
        self.fade.way = 3;
        self.fade.hidden = true;
    }

    /// 0x8008adc0: a step of 25 toward the end; there, event 10.
    fn fade_step(&mut self, p: &mut Poster) {
        let f = &mut self.fade;
        match f.way {
            1 => {
                if f.level < 230 {
                    f.level += 25;
                } else {
                    f.level = 255;
                    f.way = 0;
                    p.post(10);
                }
            }
            2 => {
                if f.level < 26 {
                    f.level = 0;
                    f.way = 0;
                    p.post(10);
                } else {
                    f.level -= 25;
                }
            }
            3 => {
                if f.level < 102 {
                    f.level += 25;
                } else if f.level < 153 {
                    f.level = 127;
                    f.way = 0;
                    p.post(10);
                } else {
                    f.level -= 25;
                }
            }
            _ => {}
        }
    }

    // --- Pads -----------------------------------------------------------

    /// 0x80086b18: whether pad `pad` has just pressed action `action`.
    fn pressed(&mut self, pad: usize, action: u8) -> bool {
        let k = (action - 14) as usize;
        if self.pads[pad].holds(&self.mappings[pad & 1], action) {
            if self.held[pad][k] {
                false
            } else {
                self.held[pad][k] = true;
                true
            }
        } else {
            self.held[pad][k] = false;
            false
        }
    }

    /// 0x80086ad0: a cheat-code button into the pad's last eight.
    fn code(&mut self, pad: usize, code: u8) {
        let c = &mut self.codes[pad];
        c.copy_within(1..8, 0);
        c[7] = code;
        c[8] = 0;
    }

    /// 0x80086cd8 (the first pad) and 0x80087088 (the second): the presses
    /// `mask` allows become events; true if any did.
    fn pad_events(&mut self, pad: usize, mask: u16, p: &mut Poster) -> bool {
        // (action, mask bit, sound, event for the first pad, for the second)
        const KEYS: [(u8, u16, u8, i16, i16); 7] = [
            (14, 1, 46, 24, 27),
            (15, 2, 47, 25, 28),
            (18, 16, 50, 26, 29),
            (19, 32, 51, 74, 0),
            (17, 8, 49, 47, 49),
            (16, 4, 48, 46, 48),
            (26, 64, 50, 1, 2),
        ];
        let mut any = false;
        for (action, bit, sound, first, second) in KEYS {
            if self.pressed(pad, action) && mask & bit != 0 {
                self.idle_from = self.clock;
                self.play(sound);
                p.post(if pad == 0 { first } else { second });
                any = true;
            }
        }
        for (action, code, event) in
            [(20, 49, None), (21, 50, None), (22, 51, Some(3)), (23, 52, Some(99)), (24, 53, None), (25, 54, None)]
        {
            if self.pressed(pad, action) {
                self.code(pad, code);
                if let Some(e) = event {
                    p.post(if pad == 0 { e } else { e + 1 });
                }
                any = true;
            }
        }
        any
    }

    // --- The memory card ------------------------------------------------

    /// 0x80088cbc: "Valid Memory Card Not Found.", Retry, Continue.
    fn card_missing_texts(&mut self) {
        let not_found = self.strings.get(86).to_string();
        let retry = self.strings.get(89).to_string();
        self.set_text(POPUP, 0, "", 320, 155, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 1, "Valid Memory Card", 320, 181, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 2, &not_found, 320, 207, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 3, "", 320, 233, 0, 0, true, 0, [255; 3]);
        self.set_text(POPUP, 4, &retry, 320, 259, 1, 0, true, 1, [127; 3]);
        self.set_text(POPUP, 5, "CONTINUE", 320, 286, 1, 0, true, 1, [127; 3]);
    }

    /// 0x8008b11c: whether the players' passwords changed since they were
    /// last saved; with no card, nothing can be saved, and the change is
    /// noted (events 41 if a card is in some odd state).
    fn check_passwords(&mut self, p: &mut Poster) {
        self.saving = [false; 2];
        let keys = [self.players[0].password_key(), self.players[1].password_key()];
        for (saved, key) in self.saved.iter_mut().zip(keys) {
            if saved.is_none() {
                *saved = Some(key);
            }
        }
        let changed = [self.saved[0] != Some(keys[0]), self.saved[1] != Some(keys[1])];
        if (changed[0] || changed[1]) && self.card_present() {
            self.saved = [Some(keys[0]), Some(keys[1])];
            p.post(38);
        } else if changed[0] || changed[1] {
            for k in 0..2 {
                if changed[k] {
                    self.saved[k] = Some(keys[k]);
                    self.saving[k] = true;
                }
            }
        }
    }

    /// 0x8008b6a4.
    fn save_prompt_texts(&mut self) {
        self.save_choice = 4;
        let (no, yes) = (self.strings.get(93).to_string(), self.strings.get(90).to_string());
        self.set_text(SAVE_PROMPT, 0, "Player Data", 320, 155, 0, 0, true, 0, [255; 3]);
        self.set_text(SAVE_PROMPT, 1, "has changed.", 320, 181, 0, 0, true, 0, [255; 3]);
        self.set_text(SAVE_PROMPT, 2, "Would you like", 320, 207, 0, 0, true, 0, [255; 3]);
        self.set_text(SAVE_PROMPT, 3, "to save now?", 320, 233, 0, 0, true, 0, [255; 3]);
        self.set_text(SAVE_PROMPT, 4, &no, 320, 259, 1, 0, true, 1, [255; 3]);
        self.set_text(SAVE_PROMPT, 5, &yes, 320, 286, 1, 0, true, 1, [255; 3]);
        self.screens[SAVE_PROMPT].enter(&self.font);
    }

    // --- The main menu --------------------------------------------------

    /// 0x8008a298: whether car `car` is there and unlocked for `player`.
    fn car_open(&self, car: u8, player: usize) -> bool {
        if self.tables.car_files.get(car as usize).is_none_or(Option::is_none) {
            return false;
        }
        let pl = &self.players[player];
        if car < 32 { pl.cars[0] >> car & 1 != 0 } else { pl.cars[1] >> (car - 32) & 1 != 0 }
    }

    /// 0x8008a2f0: either player has the track.
    fn track_open(&self, track: u8) -> bool {
        (self.players[0].tracks >> track & 1) != 0 || (self.players[1].tracks >> track & 1) != 0
    }

    /// 0x8008c544, 0x8008c668: the car preview's model, for a car the
    /// player has; a locked car shows no model.
    fn show_car(&mut self, k: usize) {
        if !self.car_open(self.players[k].car, k) {
            self.car_stats[k] = [0, 0, 0, -1];
            self.car_shown[k] = true;
            return;
        }
        if !self.car_shown[k] {
            // The qualities, as 0x8008c544 copies them: bytes 0, 2, 1, 3.
            let q = self.qualities.get(self.players[k].car as usize).copied().unwrap_or_default();
            self.car_stats[k] = [q[0] as i16, q[2] as i16, q[1] as i16, q[3] as i8 as i16];
            self.decal_car[k] = Some(self.players[k].car);
            self.car_shown[k] = true;
        }
    }

    /// 0x8008c124, 0x8008c280: the car preview asked for, if none is
    /// loaded or loading.
    fn ask_car(&mut self, k: usize) {
        let other = 1 - k;
        if !self.car_open(self.players[k].car, k) {
            self.car_asked[k] = false;
            self.car_loaded[k] = true;
            return;
        }
        if !self.car_loaded[k] && !self.car_asked[k] && !self.car_asked[other] {
            self.car_loaded[k] = false;
            self.car_asked[k] = true;
        }
    }

    /// 0x8008bf18.
    fn main_menu_enter(&mut self) {
        self.fade_in();
        self.set_background("psxmain");
        self.screens[MAIN_MENU].clear_texts();
        self.screens[MAIN_MENU].enter(&self.font);
        self.car_asked = [false; 2];
        for k in 0..2 {
            if !self.car_open(self.players[k].car, k) {
                self.players[k].car = 29;
            }
        }
        self.preview_place(0, 500, 500, 0);
        self.preview_place(1, 500, 500, 0);
        self.preview_aim(0, 435, 450, 0);
        self.preview_aim(1, 775, 450, 0);
        // Every model is in memory: the previews are asked for on the way
        // in, not left until the car line.
        for k in 0..self.people.min(2) as usize {
            self.ask_car(k);
        }
        self.helps[0].x = self.helps[0].start << 12;
        self.menu_arrows();
        // 0x80085858, 0x800858fc, 0x80085954: the map from nothing in the
        // middle to half size in its box.
        let m = &mut self.track_map;
        m.scale = 0;
        m.stopped = false;
        m.scale_to = 2048;
        m.spin_to = TRACK_REST;
        m.pos = world_at(500, 500, 0);
        m.pos_to = world_at(1070, 950, 0);
    }

    /// 0x8008be00: the arrows beside the chosen line, where left and right
    /// change something; hidden (size 0) elsewhere.
    fn menu_arrows(&mut self) {
        let at = match self.choice {
            1 => Some((19, 240, 485)),
            2 => Some((19, 240, 550)),
            3 => Some((19, 260, 615)),
            5 => Some((19, 390, 760)),
            _ => None,
        };
        match at {
            Some((left, right, y)) => {
                self.place_arrow("arrowlt", left, y, 100);
                self.place_arrow("arrowrt", right, y, 100);
            }
            None => {
                self.place_arrow("arrowlt", -1, -1, 0);
                self.place_arrow("arrowrt", -1, -1, 0);
            }
        }
    }

    /// 0x8008bc94: the main menu's piece `name` set to grow or shrink to
    /// `size`, and put at (x, y) unless both are negative.
    fn place_arrow(&mut self, name: &str, x: i32, y: i32, size: u16) {
        self.place_piece(MAIN_MENU, name, x, y, size);
    }

    /// 0x8008bc94 and its twins (0x80092660 for the options): screen
    /// `screen`'s piece `name` set to grow or shrink to `size`, and put at
    /// (x, y) unless both are negative.
    fn place_piece(&mut self, screen: usize, name: &str, x: i32, y: i32, size: u16) {
        let Some(p) = self.screens[screen].pieces.iter_mut().find(|p| p.model == name) else { return };
        p.size_to = size;
        if x < 0 && y < 0 {
            return;
        }
        p.pos[0] = screen::world_x(x);
        p.pos[1] = screen::world_y(y);
        p.to[0] = p.pos[0];
        p.to[1] = p.pos[1];
    }

    /// 0x8008c930: the main menu's lines.
    fn main_menu_texts(&mut self) {
        let p0 = self.players[0].clone();
        let car = self.tables.car_names.get(p0.car as usize).cloned().unwrap_or_default();
        let track = self.tables.track_names.get(p0.track as usize).cloned().unwrap_or_default();
        let modes = [62, 63, 64, 65, 59];
        let mode = self.strings.get(*modes.get(self.settings.mode as usize).unwrap_or(&59)).to_string();
        let sign_in = self.strings.get(self.tables.sign_in[self.sign_in.min(2) as usize] as usize).to_string();
        let race = self.strings.get(if self.settings.mode == 4 { 111 } else { 110 }).to_string();
        self.set_text(MAIN_MENU, 0, &p0.name, 273, 122, 0, 0, true, 0, [255; 3]);
        self.set_text(MAIN_MENU, 2, &car, 273, 238, 0, 0, true, 1, [255; 3]);
        self.set_text(MAIN_MENU, 4, &track, 505, 280, 0, 0, true, 0, [255; 3]);
        self.set_text(MAIN_MENU, 5, &mode, 23, 355, 0, 1, false, 1, [127; 3]);
        self.set_text(MAIN_MENU, 6, &sign_in, 23, 288, 0, 1, false, 1, [127; 3]);
        self.set_text(MAIN_MENU, 7, &race, 23, 188, 0, 1, false, 1, [127; 3]);
        if self.people == 2 {
            let p1 = self.players[1].clone();
            let car1 = self.tables.car_names.get(p1.car as usize).cloned().unwrap_or_default();
            self.set_text(MAIN_MENU, 1, &p1.name, 504, 122, 0, 0, true, 0, [255; 3]);
            self.set_text(MAIN_MENU, 3, &car1, 504, 238, 0, 0, true, 1, [255; 3]);
        } else {
            let start = self.strings.get(96).to_string();
            self.set_text(MAIN_MENU, 1, &start, 500, 185, 0, 0, true, 1, [255; 3]);
            self.set_text(MAIN_MENU, 3, "", 490, 238, 0, 0, true, 1, [255; 3]);
        }
    }

    /// 0x8008cccc: the menu, its choice flashing, the help line.
    fn main_menu_draw(&mut self, p: &mut Poster) {
        self.begin_frame();
        self.fade_step(p);
        let (text, label) = match self.choice {
            0 => (Some(7), None),
            1 => (None, Some(0)),
            2 => (None, Some(1)),
            3 => (Some(6), None),
            4 => (None, Some(2)),
            5 => (Some(5), None),
            _ => (None, None),
        };
        self.draw_screen(MAIN_MENU, text, label, self.fade.hidden);
        for k in 0..self.people.min(2) as usize {
            if self.car_loaded[k] {
                self.draw_preview(k);
            } else if self.car_asked[k] {
                // The program loads at once.
                self.car_loaded[k] = true;
                self.car_shown[k] = false;
                self.car_asked[k] = false;
                match k {
                    0 if self.choice == 1 => self.preview_aim(0, 445, 480, 0),
                    0 => self.preview_aim(0, 435, 450, 0),
                    _ if self.choice == 1 => self.preview_aim(1, 685, 480, 0),
                    _ => self.preview_aim(1, 775, 450, 0),
                }
                self.car_arrived(k);
            }
            self.draw_decal(k, [191, 403][k], 61);
        }
        self.draw_track_map();
        if !self.fade.hidden {
            self.draw_help(0);
        }
        self.end_frame();
    }

    /// 0x800867cc: the help line, scrolling left 45 pixels a second and
    /// starting again once it is all gone, with the buttons it names as
    /// their 3D icons.
    fn draw_help(&mut self, k: usize) {
        let h = self.helps[k];
        let text: Vec<u8> = self.strings.get(h.string as usize).bytes().collect();
        let mut x = h.x >> 12;
        let mut drawn = 0;
        for (i, &c) in text.iter().enumerate() {
            let icon = matches!(c, b'^' | b'*' | b'#' | b'%' | b']');
            if h.left < x && x < h.right {
                if icon {
                    self.drawing_pieces.push(button_icon(c, x, h.y));
                } else {
                    self.drawing.push(Glyph {
                        ch: c.to_ascii_uppercase(),
                        x: x as i16,
                        y: (h.y / 2 - 8) as i16,
                        colour: [120; 3],
                    });
                }
                drawn += 1;
            }
            x += match (icon, text.get(i + 1)) {
                (true, Some(_)) => 22,
                (true, None) => 20,
                (false, Some(&n)) => self.font.glyph_width(c) as i32 + self.font.kerning(c, n) as i32,
                (false, None) => self.font.glyph_width(c) as i32,
            };
        }
        let s = self.seconds(4);
        self.helps[k].x = if drawn == 0 && h.x < h.left { h.start << 12 } else { h.x - fx(0x2d000, s) };
    }

    /// 0x8008d264: the pads, first and second in turn as the frames go.
    fn main_menu_pads(&mut self, p: &mut Poster) {
        if (self.car_asked[0] && !self.car_loaded[0]) || (self.car_asked[1] && !self.car_loaded[1]) {
            return;
        }
        let mut first: u16 = 67;
        let mut second: u16 = if self.people == 1 { 64 } else { 32 };
        match self.choice {
            0 | 1 | 4 => first |= 16,
            3 => {
                first |= 16;
                second |= 16;
            }
            _ => {}
        }
        if !self.second_first {
            self.menu_pad(0, first, p);
            self.menu_pad(1, second, p);
            self.second_first = true;
        } else {
            self.menu_pad(1, second, p);
            self.menu_pad(0, first, p);
            self.second_first = false;
        }
    }

    /// 0x8008cff4, 0x8008d12c: the presses (and the codes they make), then
    /// left and right held, stepping after half a second and faster down to
    /// a quarter.
    fn menu_pad(&mut self, pad: usize, mask: u16, p: &mut Poster) {
        if self.pad_events(pad, mask, p) {
            // 0x8008d010: the pad's last eight code buttons against the
            // codes, for its player; a match chimes and starts them again
            // (0x80086a70, 0x80086aa0: eight spaces).
            let code: Vec<u8> = self.codes[pad].iter().copied().take_while(|&c| c != 0).collect();
            if self.tables.apply_code(&code, &mut self.players[pad]) {
                tracing::info!(
                    "player {}: code {} taken, cheats {:#x}",
                    pad + 1,
                    String::from_utf8_lossy(&code),
                    self.players[pad].cheats
                );
                self.play(52);
                self.codes[pad][..8].fill(b' ');
            }
        }
        let lets = if pad == 0 { matches!(self.choice, 1..=3 | 5) } else { self.people == 2 && self.choice == 1 };
        if !lets {
            return;
        }
        let events = if pad == 0 { (46, 47) } else { (48, 49) };
        self.held_repeat(pad, (16, events.0), (17, events.1), p);
    }

    /// 0x8008b9d0 as a car's model arrives: pad `k`'s held step starts
    /// again from no delay. The original reads a car from the CD a second
    /// and a half after the last step, so the button is always up by then;
    /// here the model is in memory and arrives within a frame or two, while
    /// a tap is still held, and clearing the delay then would step the car
    /// a second time. So it is cleared only with the button up (where
    /// letting go has cleared it already).
    fn car_arrived(&mut self, k: usize) {
        if !self.stepping[k] {
            self.repeat[k].0 = 0;
        }
    }

    /// 0x8008ba30 and its helpers: action `a` (or else `b`) held steps
    /// (posting its event) after half a second, then faster down to a
    /// quarter; let go, the delay starts again.
    fn held_repeat(&mut self, pad: usize, a: (u8, i16), b: (u8, i16), p: &mut Poster) {
        let now = self.clock;
        let (delay, last) = self.repeat[pad];
        let due = last.wrapping_add(delay) < now;
        let held = |s: &Self, a: u8| s.pads[pad].holds(&s.mappings[pad & 1], a);
        let event = if held(self, a.0) {
            Some(a.1)
        } else if held(self, b.0) {
            Some(b.1)
        } else {
            None
        };
        match event {
            Some(e) => {
                if due {
                    p.post(e);
                    let d = if delay == 0 { 500 } else { (delay / 2).max(250) };
                    self.repeat[pad] = (d, now);
                    self.stepping[pad] = true;
                }
            }
            None => {
                if self.stepping[pad] {
                    self.repeat[pad].0 = 0;
                    self.stepping[pad] = false;
                }
            }
        }
    }

    /// 0x8008bb90: on the car line, a car the preview has not caught up
    /// with is asked for. The original waits a second and a half first, so
    /// scrolling through cars doesn't start a CD read for each; every
    /// model is in memory here, so it asks at once.
    fn car_preview_timers(&mut self, p: &mut Poster) {
        if self.choice != 1 {
            return;
        }
        if !self.car_loaded[0] {
            p.post(50);
            self.car_changed[0] = self.clock;
        }
        if self.people == 2 && !self.car_loaded[1] {
            p.post(51);
            self.car_changed[1] = self.clock;
        }
    }

    /// 0x8008d948 (Start) and 0x8008d9f8 (Cross): a race of the mode
    /// chosen, or what the chosen line opens.
    fn main_menu_choose(&mut self, addr: u32, p: &mut Poster) {
        let race = || match self.settings.mode {
            0 => 61,
            1 => 62,
            2 => 63,
            3 => 64,
            4 => 65,
            _ => 60,
        };
        if addr == 0x8008_d948 {
            p.post(race());
            return;
        }
        p.post(match self.choice {
            0 => race(),
            1 => 67,
            3 => {
                self.sign_in_back = false;
                66
            }
            4 => 68,
            _ => 60,
        });
    }

    /// 0x8008d35c, 0x8008d470: up or down a line, the arrows following,
    /// the track map zooming out of the middle as the track line is left and
    /// into it as it is chosen.
    fn menu_step(&mut self, down: bool) {
        if self.choice == 1 {
            self.preview_full(0);
            self.preview_full(1);
            self.preview_aim(0, 435, 450, 0);
            self.preview_aim(1, 775, 450, 0);
        }
        if self.choice == 2 {
            self.track_map.stopped = false;
            self.track_map.scale_to = 2048;
            self.track_map.spin_to = TRACK_REST;
            self.track_map.pos_to = world_at(1070, 950, 0);
        }
        self.choice = match (down, self.choice) {
            (false, 0) => 5,
            (false, c) => c - 1,
            (true, c) if c < 5 => c + 1,
            (true, _) => 0,
        };
        if self.choice == 1 {
            self.previews[0].size_to(0x1800);
            self.previews[1].size_to(0x1800);
            self.preview_aim(0, 445, 480, 0);
            self.preview_aim(1, 685, 480, 0);
        }
        if self.choice == 2 {
            self.track_map.stopped = false;
            self.track_map.scale_to = 3072;
            self.track_map.pos_to = world_at(875, 800, 0);
        }
        self.menu_arrows();
    }

    /// 0x800859ac: the track map a frame on: turning (2/5 of a turn a
    /// second, or easing to its rest angle when stopped), growing, moving.
    fn draw_track_map(&mut self) {
        let dt = self.clock.wrapping_sub(self.track_moved);
        let f = if self.track_moved == 0 {
            0xf000 * 4096 / 0x6_4000
        } else if dt < 101 {
            ((dt as i32) << 12) / 100
        } else {
            4096 / 10
        };
        self.track_moved = self.clock;
        let m = &mut self.track_map;
        if m.stopped {
            m.spin = m.spin.wrapping_add(fx(m.spin_to.wrapping_sub(m.spin), f));
        } else {
            let s = self.seconds(3);
            let m = &mut self.track_map;
            m.spin = m.spin.wrapping_add(fx(s, TRACK_SPIN));
            while m.spin > TWO_PI {
                m.spin -= TWO_PI;
            }
        }
        let m = &mut self.track_map;
        m.scale = m.scale.wrapping_add(fx(m.scale_to.wrapping_sub(m.scale), f));
        if m.scale_to == 0 && m.scale < 4096 / 3 {
            m.scale = 0;
        }
        for k in 0..3 {
            m.pos[k] = m.pos[k].wrapping_add(fx(m.pos_to[k].wrapping_sub(m.pos[k]), f));
        }
        if let Some(model) = &self.track_model {
            self.drawing_track = Some(TrackDraw { model: model.clone(), spin: m.spin, scale: m.scale, pos: m.pos });
        }
    }

    /// 0x80084adc: player `k`'s car preview put at (x, y, z) at once.
    pub(super) fn preview_place(&mut self, k: usize, x: i32, y: i32, z: i32) {
        self.previews[k].pos = world_at(x, y, z);
    }

    /// 0x80084b54: player `k`'s car preview sent toward (x, y, z).
    pub(super) fn preview_aim(&mut self, k: usize, x: i32, y: i32, z: i32) {
        self.previews[k].pos_to = world_at(x, y, z);
    }

    /// 0x8008499c: back to full size, the rest angle its goal.
    pub(super) fn preview_full(&mut self, k: usize) {
        self.previews[k].size_to(4096);
        self.previews[k].angle_to = CAR_REST;
    }

    /// 0x80084828: the preview straight to its rest angle.
    pub(super) fn preview_rest(&mut self, k: usize) {
        self.previews[k].angle = CAR_REST;
        self.previews[k].angle_to = CAR_REST;
    }

    /// 0x80083370: how far player `k`'s preview eases toward its goals this
    /// frame: a hundredth a millisecond since the last, a tenth after a
    /// long gap, 0.15 the first time.
    fn preview_ease(&mut self, k: usize) -> i32 {
        let p = &mut self.previews[k];
        let now = self.clock;
        let f = if p.eased == 0 {
            0xf000 * 4096 / 0x6_4000
        } else {
            let dt = now.wrapping_sub(p.eased);
            if dt < 101 { ((dt as i32) << 12) / 100 } else { 4096 / 10 }
        };
        p.eased = now;
        f
    }

    /// 0x80084bcc's motion: player `k`'s preview a frame on (spinning a
    /// turn every five seconds, its wheels rolling four times as fast, or
    /// turning to its goal), growing and moving toward its goals; then
    /// drawn.
    fn draw_preview(&mut self, k: usize) {
        let f = self.preview_ease(k);
        if self.previews[k].turning {
            let p = &mut self.previews[k];
            p.angle = p.angle.wrapping_add(fx(p.angle_to.wrapping_sub(p.angle), f));
        } else {
            let s = self.seconds(k);
            let p = &mut self.previews[k];
            p.angle = p.angle.wrapping_add(fx(s, TRACK_SPIN));
            while p.angle > TWO_PI {
                p.angle -= TWO_PI;
            }
            p.wheels = fx(p.angle, 0x4000);
        }
        let p = &mut self.previews[k];
        p.scale = p.scale.wrapping_add(fx(p.scale_to.wrapping_sub(p.scale), f));
        if p.scale_to == 0 && p.scale < 4096 / 3 {
            p.scale = 0;
        }
        for i in 0..3 {
            p.pos[i] = p.pos[i].wrapping_add(fx(p.pos_to[i].wrapping_sub(p.pos[i]), f));
        }
        let car = self.preview_car[k].unwrap_or(self.players[k].car);
        let draw = CarDraw { player: k as u8, car, angle: p.angle, wheels: p.wheels, scale: p.scale, pos: p.pos };
        if self.car_open(draw.car, k) {
            self.drawing_cars.push(draw);
        }
    }

    /// 0x8008a448, 0x8008a3d4: the previous or next car the player has.
    fn car_step(&mut self, k: usize, next: bool) {
        loop {
            let c = self.players[k].car;
            self.players[k].car = match (next, c) {
                (false, 0) => 40,
                (false, c) => c - 1,
                (true, c) if c < 40 => c + 1,
                (true, _) => 0,
            };
            if self.car_open(self.players[k].car, k) {
                break;
            }
        }
    }

    /// 0x8008a598, 0x8008a4c0: the previous or next track either player
    /// has; the second player's follows.
    fn track_step(&mut self, next: bool) {
        loop {
            let t = self.players[0].track;
            self.players[0].track = match (next, t) {
                (false, 0) => 11,
                (false, t) => t - 1,
                (true, t) if t < 11 => t + 1,
                (true, _) => 0,
            };
            let t = self.players[0].track;
            if self.track_open(t) && self.tables.track_files.get(t as usize).is_some_and(Option::is_some) {
                break;
            }
        }
        self.players[1].track = self.players[0].track;
    }

    // --- The race --------------------------------------------------------

    /// 0x8009ba28: the race: the chosen track, the player's car on a random
    /// grid place, five computer cars drawn from the unlocked ones until
    /// the field's average rating is under 10213 (at most 100 draws).
    fn set_up_race(&mut self) {
        let p0 = self.players[0].clone();
        let world = (p0.track / 3) as usize;
        let number = p0.track % 3 + 1;
        let file = |c: u8| self.tables.car_files.get(c as usize).cloned().flatten().unwrap_or_default();
        let mut grid_free = [true; 6];
        let mut entrants =
            vec![Entrant { name: file(p0.car), driver: Driver::PlayerOne, car_id: p0.car, player: 0, grid: 0 }];
        let g = loop {
            let r = self.rand.below(6) as usize;
            if grid_free[r] {
                break r;
            }
        };
        entrants[0].grid = g as u8;
        grid_free[g] = false;
        let count = if self.people == 2 { 2 } else { 6 };
        if self.people == 2 {
            // Both players are drivers of kind 1, on the first two places.
            let p1 = &self.players[1];
            entrants[0].grid = 0;
            entrants.push(Entrant {
                name: file(p1.car),
                driver: Driver::PlayerOne,
                car_id: p1.car,
                player: 1,
                grid: 1,
            });
        } else {
            for _ in 1..count {
                let g = loop {
                    let r = self.rand.below(6) as usize;
                    if grid_free[r] {
                        break r;
                    }
                };
                grid_free[g] = false;
                entrants.push(Entrant {
                    name: String::new(),
                    driver: Driver::Computer,
                    car_id: 0,
                    player: 0,
                    grid: g as u8,
                });
            }
            for _ in 0..100 {
                let mut free = [true; 41];
                free[p0.car as usize % 41] = false;
                for entrant in entrants.iter_mut().take(count).skip(self.people as usize) {
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
                    entrant.name = file(c as u8);
                    entrant.car_id = c as u8;
                }
                if self.field_fair(&entrants) {
                    break;
                }
            }
        }
        let cheats = if p0.cheats != 0 { p0.cheats } else { self.players[1].cheats };
        let flags = 32
            | ((p0.cheats >> 3) & 16)
            | ((self.players[1].cheats >> 3) & 16)
            | (((p0.cheats & 256 != 0) as u32) << 6)
            | (((self.players[1].cheats & 256 != 0) as u32) << 6);
        self.race = Some(RaceSetup {
            flags,
            track: self.tables.worlds.get(world).cloned().unwrap_or_default(),
            track_number: number,
            laps: 4,
            checkpoints: self.tables.checkpoints.get(world * 3 + number as usize - 1).copied().unwrap_or(0),
            options: cheats,
            time_limit: 0,
            cars: entrants,
            difficulty: self.settings.difficulty,
            best_line: None,
            names: self.player_names(),
        });
        self.race_end = None;
        self.last_race = self.race.clone();
        self.race_music();
    }

    /// 0x8008a808: the race's song, by the music mode: one of the thirteen
    /// at random (0x8008a6cc), the track's own (0x8008a6f8: the table at
    /// 0x800be8c0, song k for track k), or the one chosen; then played
    /// (0x8008a7b4).
    fn race_music(&mut self) {
        match self.music_mode {
            0 => self.song = self.rand.below(crate::cd::SONGS as u32) as u8,
            1 => self.song = self.tables.track_songs.get(self.players[0].track as usize).copied().unwrap_or(0),
            _ => {}
        }
        self.cd.push(crate::cd::CdAsk::Play(self.song));
    }

    /// 0x8009c9f8: the field's average rating is under 10213.
    fn field_fair(&self, cars: &[Entrant]) -> bool {
        let sum: u32 = cars
            .iter()
            .map(|e| {
                let k = self.tables.car_files.iter().position(|f| f.as_deref() == Some(e.name.as_str())).unwrap_or(41);
                self.tables.car_ratings.get(k).copied().unwrap_or(0) as u32
            })
            .sum();
        sum.checked_div(cars.len() as u32).unwrap_or(0) < 0x27e5
    }
}

/// Every ported action of the front end, registered by the part of the
/// game it belongs to.
fn registry() -> Registry<Front> {
    let mut r = Registry::default();
    boot::register(&mut r);
    title::register(&mut r);
    main_menu::register(&mut r);
    race_start::register(&mut r);
    options::register(&mut r);
    credits::register(&mut r);
    hiscores::register(&mut r);
    garage::register(&mut r);
    cup::register(&mut r);
    unlocks::register(&mut r);
    sign_in::register(&mut r);
    card_menu::register(&mut r);
    controls::register(&mut r);
    r.add(0x8008_ada8, |f: &mut Front, _: &mut Poster| f.fade_half());
    r
}

/// The track map's resting angle (0x80085858), a full turn, and how fast it
/// turns, in 4.12 radians (a second).
const TRACK_REST: i32 = (71 * 0x9_1000) >> 12;
const TWO_PI: i32 = 0x6488;
const TRACK_SPIN: i32 = ((TWO_PI / 0x5000) << 12) + (((TWO_PI % 0x5000) << 12) / 0x5000);

/// The track map's motion (0x800d2724 on, 0x80136c50, 0x80136c60).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TrackMap {
    spin: i32,
    spin_to: i32,
    stopped: bool,
    scale: i32,
    scale_to: i32,
    pos: [i32; 3],
    pos_to: [i32; 3],
}

/// The car previews' rest angle (0x80084780): 2.25 radians.
const CAR_REST: i32 = (71 * 0x8_2000) >> 12;

/// A player's car preview in motion (0x800d1090 the size and 0x800d1098
/// its goal, 0x800d10a0 the angle and 0x800d10a8 its goal, 0x800d10b0
/// turning to it rather than spinning, 0x80136c10 the place and 0x80136c30
/// its goal, 0x800bfddc when it last eased).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CarPreview {
    scale: i32,
    scale_to: i32,
    angle: i32,
    angle_to: i32,
    turning: bool,
    pos: [i32; 3],
    pos_to: [i32; 3],
    eased: u32,
    wheels: i32,
}

impl CarPreview {
    const fn new() -> CarPreview {
        CarPreview {
            scale: 4096,
            scale_to: 4096,
            angle: 0,
            angle_to: 0,
            turning: false,
            pos: [0; 3],
            pos_to: [0; 3],
            eased: 0,
            wheels: 0,
        }
    }

    /// 0x80084960, 0x8008499c, 0x80084a14, 0x80084aa0, 0x80084928: spin
    /// and grow or shrink toward `scale`.
    pub(super) fn size_to(&mut self, scale: i32) {
        self.scale_to = scale;
        self.turning = false;
    }
}

/// A car preview to draw: player `player`'s car `car` (by the name table
/// at 0x800c5ce8), turned `angle` about its up axis after the front end's
/// tilt, its wheels rolled `wheels`, at `scale`, at `pos` (world units).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarDraw {
    pub player: u8,
    pub car: u8,
    pub angle: i32,
    pub wheels: i32,
    pub scale: i32,
    pub pos: [i32; 3],
}

/// A car's picture to draw: the slot it was loaded into (0x8011c7b0 a
/// player: VRAM (640, 256 + 64 × slot)), the car, and where its top left
/// goes, in the 640 by 240 screen's pixels (0x80023660).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecalDraw {
    pub slot: u8,
    pub car: u8,
    pub x: i16,
    pub y: i16,
}

/// The track map to draw: the model, turned `spin` about its up axis after
/// a fixed tilt, at `scale`, at `pos` (world units).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackDraw {
    pub model: String,
    pub spin: i32,
    pub scale: i32,
    pub pos: [i32; 3],
}

/// The front end's coordinates in world units.
fn world_at(x: i32, y: i32, z: i32) -> [i32; 3] {
    [screen::world_x(x), screen::world_y(y), screen::world_z(z)]
}

/// 0x8008664c: the 3D icon of a button the help line names (`^` Triangle,
/// `*` Circle, `#` Square, `%` Cross, `]` R1), at pixel `x` and line `y`
/// of the 640 by 480 screen, full size.
fn button_icon(c: u8, x: i32, y: i32) -> PieceDraw {
    let model = match c {
        b'^' => "psxtri",
        b'*' => "psxcircl",
        b'#' => "psxsq",
        b'%' => "psxxxx",
        _ => "r1butt",
    };
    let fx_ = (((x as u32).wrapping_mul(1000) as u64 * 0xcccc_cccd) >> 32) as u32 >> 9;
    let fy = (y as u32).wrapping_mul(1000) >> 9;
    PieceDraw {
        model: model.to_string(),
        pos: [screen::world_x(fx_ as i32 + 19), screen::world_y(fy as i32 + 75), 0],
        turn: [0; 3],
        scale: 4096,
    }
}

/// 0x8006981c: a card or player id, a random number under 400 000 000
/// and not 0.
fn card_id(rand: &mut Rand) -> u32 {
    loop {
        let id = rand.below(0x17d7_8400);
        if id != 0 {
            return id;
        }
    }
}

/// 0x800287b8: the background's brightness at fade level `level`: black
/// below 51, else half and one (128 is as drawn).
pub fn fade_shade(level: u8) -> u8 {
    if level < 51 { 0 } else { level / 2 + 1 }
}

impl Kit for Front {
    fn def(&self) -> Rc<Def<u32>> {
        Rc::clone(&self.def)
    }

    fn machine(&mut self) -> &mut Fsm {
        &mut self.fsm
    }

    fn actions(&self) -> Rc<Registry<Front>> {
        Rc::clone(&self.actions)
    }

    fn unported(&mut self, addr: u32) {
        self.note_unported(addr);
    }

    /// A frame was finished, or a race is waiting to be run.
    fn blank_over(&self) -> bool {
        self.frame_done || self.race.is_some()
    }
}

/// A BMF container's parts (u32 count, u32 4 x count, the sizes, then the
/// parts back to back).
fn bmf_parts(b: &[u8]) -> Option<Vec<&[u8]>> {
    let word = |a: usize| Some(u32::from_le_bytes(b.get(a..a + 4)?.try_into().ok()?) as usize);
    let n = word(0)?;
    let mut at = 8 + 4 * n;
    let mut parts = Vec::with_capacity(n);
    for k in 0..n {
        let size = word(8 + 4 * k)?;
        parts.push(b.get(at..at + size)?);
        at += size;
    }
    Some(parts)
}

/// `n` strings by a table of pointers at `at`.
fn pointer_strings(byte: &dyn Fn(u32) -> u8, at: u32, n: u32) -> Vec<String> {
    let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
    (0..n)
        .map(|k| {
            let p = word(at + 4 * k);
            (0..40).map(|i| byte(p + i)).take_while(|&b| b != 0).map(char::from).collect()
        })
        .collect()
}
