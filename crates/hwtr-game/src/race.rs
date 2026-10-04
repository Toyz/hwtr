//! The race as the front end sets it up: the track, the cars and who drives
//! them, the difficulty (the record `race_load`, 0x80033304, takes; at
//! 0x80138c94 in the original).

/// Who drives a car.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driver {
    Computer,
    PlayerOne,
    PlayerTwo,
    /// A value the game does not use; kept as it was.
    Other(u8),
}

impl Driver {
    pub fn from_byte(b: u8) -> Driver {
        match b {
            0 => Driver::Computer,
            1 => Driver::PlayerOne,
            2 => Driver::PlayerTwo,
            b => Driver::Other(b),
        }
    }

    pub fn byte(self) -> u8 {
        match self {
            Driver::Computer => 0,
            Driver::PlayerOne => 1,
            Driver::PlayerTwo => 2,
            Driver::Other(b) => b,
        }
    }

    pub fn is_player(self) -> bool {
        matches!(self, Driver::PlayerOne | Driver::PlayerTwo)
    }
}

/// One car in the race.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entrant {
    /// The car's file name (up to 8 characters: "spltimg", "mongoose").
    pub name: String,
    pub driver: Driver,
    /// The car's number in the game's car table.
    pub car_id: u8,
    /// Which player's pad drives it.
    pub player: u8,
    /// Its place on the start grid.
    pub grid: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceSetup {
    /// Bit 6: the mirrored track.
    pub flags: u32,
    /// "Desert", "Glacial", "Volcano", "Haunted", and which of its three.
    pub track: String,
    pub track_number: u8,
    pub unknown_19: [u8; 3],
    /// Bit 2: cars at a third of their size; bit 5: wheels at half.
    pub options: u32,
    pub cars: Vec<Entrant>,
    /// 0 to 255.
    pub difficulty: u8,
}

/// Where the original keeps the setup, and the codec.
pub mod layout {
    use super::{Driver, Entrant, RaceSetup};
    use crate::ram::Ram;

    pub const SETUP: u32 = 0x8013_8c94;
    pub const ENTRANTS: u32 = 0x24;
    pub const ENTRANT_SIZE: u32 = 13;

    fn name(ram: &Ram, at: u32, len: u32) -> String {
        (0..len).map(|k| ram.u8(at + k)).take_while(|&b| b != 0).map(char::from).collect()
    }

    impl RaceSetup {
        pub fn read(ram: &Ram, at: u32) -> RaceSetup {
            let count = ram.u8(at + 0x18) as u32;
            let cars = (0..count)
                .map(|k| {
                    let e = at + ENTRANTS + k * ENTRANT_SIZE;
                    Entrant {
                        name: name(ram, e, 9),
                        driver: Driver::from_byte(ram.u8(e + 9)),
                        car_id: ram.u8(e + 10),
                        player: ram.u8(e + 11),
                        grid: ram.u8(e + 12),
                    }
                })
                .collect();
            RaceSetup {
                flags: ram.i32(at) as u32,
                track: name(ram, at + 4, 19),
                track_number: ram.u8(at + 0x17),
                unknown_19: [ram.u8(at + 0x19), ram.u8(at + 0x1a), ram.u8(at + 0x1b)],
                options: ram.i32(at + 0x1c) as u32,
                cars,
                difficulty: ram.u8(at + 0x72),
            }
        }
    }
}

use crate::car::{Car, Controls, EngineSpec, Handling, Tuning};
use crate::collision::world::Step;
use crate::collision::{Collision, Scp};
use crate::rand::Rand;
use crate::math::Tables;

/// One race step: the game logic runs at 40 steps a second (`race_frame`,
/// 0x80033ed8), whatever the display does.
pub const STEP_MS: i32 = 25;

/// A race in progress: what is ported of it so far. The players' cars
/// drive on the track; computer cars, walls, laps and the rest follow.
pub struct Race {
    pub setup: RaceSetup,
    pub tables: Tables,
    pub tuning: Tuning,
    pub cars: Vec<Car>,
    pub collision: Collision,
    /// The game's random numbers.
    pub rand: Rand,
    /// The race clock (0x800d0e34), 25 ms a step.
    pub time: u32,
    /// The system clock (0x800d240c), which the host advances 17 ms a
    /// vertical blank.
    pub clock: u32,
}

impl Race {
    /// Starts the race `setup` on the track `scp`, with each car's handling
    /// (in the setup's order). Only the players' cars are built so far.
    pub fn new(
        setup: RaceSetup,
        scp: Scp,
        handling: &[(Handling, EngineSpec)],
        tables: Tables,
        tuning: Tuning,
    ) -> Race {
        let mut collision = Collision::new(scp);
        let mut cars = Vec::new();
        for (slot, entrant) in setup.cars.iter().enumerate() {
            if !entrant.driver.is_player() {
                continue;
            }
            let (h, spec) = &handling[slot];
            let grid = collision.scp.grid[entrant.grid as usize];
            let mut car = Car::load(cars.len() as u8, entrant, &setup, (h, spec), grid, &tuning);
            collision.add_car(&tables, &mut car);
            cars.push(car);
        }
        Race { setup, tables, tuning, cars, collision, rand: Rand::default(), time: 0, clock: 0 }
    }

    /// One step of `STEP_MS`, with each player's controls (in car order).
    pub fn step(&mut self, controls: &[Controls]) {
        let dt = (STEP_MS << 12) / 1000;
        for (car, c) in self.cars.iter_mut().zip(controls) {
            car.apply_controls(c, &self.tuning);
        }
        for car in &mut self.cars {
            car.update(&self.tables, &self.tuning, dt);
        }
        let mut step =
            Step { tuning: &self.tuning, rand: &mut self.rand, time: self.time, clock: self.clock };
        self.collision.update(&self.tables, &mut self.cars, &mut step);
        self.time = self.time.wrapping_add(STEP_MS as u32);
    }
}
