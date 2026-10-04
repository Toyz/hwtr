//! A car's handling, as its CWH file gives it (part 3 of the car's BMF; see
//! `docs/formats/car.md`): block A, the handling proper, which the game
//! copies into the car record at +0x63c and reads from there, and block B,
//! the engine's specification, copied over the engine from its redline on.
//!
//! Both convert to and from their exact bytes, unknown fields included.

use super::AirPower;
use crate::math::Vec3;

/// The CWH file's magic number.
pub const CWH_MAGIC: u32 = 0x0b97_57a5;
pub const BLOCK_A: usize = 308;
pub const BLOCK_B: usize = 64;

/// What differs between the front axle and the rear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Axle {
    /// Spring stiffness.
    pub stiffness: i32,
    /// Tyre grip.
    pub grip: i32,
    /// The spring's limit (the compression its force stops growing at).
    pub travel: i32,
    /// Damping in compression and in rebound.
    pub damp_in: i32,
    pub damp_out: i32,
    /// Lowers the wheel mounts.
    pub ride_height: i32,
    /// Downforce coefficient and factor.
    pub downforce: i32,
    pub downforce_scale: i32,
}

/// CWH block A, the car's handling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Handling {
    /// Which wheels are driven, and which steer.
    pub front_driven: bool,
    pub front_steers: bool,
    pub rear_driven: bool,
    pub rear_steers: bool,
    /// The front wheels' full steering angle, radians.
    pub steer_lock: i32,
    pub mass: i32,
    /// The centre of gravity forward and up, as fractions of half the length
    /// and half the height.
    pub cg_along: i32,
    pub cg_up: i32,
    /// Braking force per unit weight, and the front axle's share.
    pub brake_grip: i32,
    pub brake_bias: i32,
    pub drag: i32,
    pub front: Axle,
    pub rear: Axle,
    pub air_power: AirPower,
    /// Scaled by the race's difficulty for the cars it drives (between two
    /// tuning bytes, held to 4).
    pub skill: i32,
    /// The dragging surface does not drag it.
    pub all_terrain: bool,
    pub wheel_count: u8,
    /// The point wheel mounts are measured from.
    pub origin: Vec3,
    /// Width, length, height.
    pub size: Vec3,
    /// Each wheel's mount point, and its diameter.
    pub mounts: [Vec3; 6],
    pub diameters: [i32; 6],
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> u8 {
        self.at += 1;
        self.b[self.at - 1]
    }
    fn i32(&mut self) -> i32 {
        self.at += 4;
        i32::from_le_bytes(self.b[self.at - 4..self.at].try_into().unwrap())
    }
    fn vec3(&mut self) -> Vec3 {
        [self.i32(), self.i32(), self.i32()]
    }
    /// Passes over bytes the game does not use.
    fn skip(&mut self, n: usize) {
        self.at += n;
    }
}


impl Handling {
    /// Block A, as it lies in the CWH (after the magic).
    pub fn from_bytes(b: &[u8; BLOCK_A]) -> Handling {
        let mut r = Reader { b, at: 0 };
        let (front_driven, front_steers, rear_driven, rear_steers) = (r.u8(), r.u8(), r.u8(), r.u8());
        let steer_lock = r.i32();
        let (mass, cg_along, cg_up, brake_grip, brake_bias, drag) =
            (r.i32(), r.i32(), r.i32(), r.i32(), r.i32(), r.i32());
        let downforce = [r.i32(), r.i32()];
        let downforce_scale = [r.i32(), r.i32()];
        let mut axle = |k: usize| Axle {
            stiffness: r.i32(),
            grip: r.i32(),
            travel: r.i32(),
            damp_in: r.i32(),
            damp_out: r.i32(),
            ride_height: r.i32(),
            downforce: downforce[k],
            downforce_scale: downforce_scale[k],
        };
        let (front, rear) = (axle(0), axle(1));
        Handling {
            front_driven: front_driven != 0,
            front_steers: front_steers != 0,
            rear_driven: rear_driven != 0,
            rear_steers: rear_steers != 0,
            steer_lock,
            mass,
            cg_along,
            cg_up,
            brake_grip,
            brake_bias,
            drag,
            front,
            rear,
            air_power: AirPower { pitch: r.i32(), roll: r.i32(), yaw: r.i32() },
            skill: {
                r.skip(8);
                r.i32()
            },
            all_terrain: r.i32() & 1 != 0,
            wheel_count: r.u8(),
            origin: {
                r.skip(3);
                r.vec3()
            },
            size: {
                r.skip(4);
                r.vec3()
            },
            mounts: {
                r.skip(4);
                std::array::from_fn(|_| {
                    let m = r.vec3();
                    r.skip(4);
                    m
                })
            },
            diameters: {
                r.skip(24);
                std::array::from_fn(|_| r.i32())
            },
        }
    }


    pub fn axle(&self, rear: bool) -> &Axle {
        if rear { &self.rear } else { &self.front }
    }
}

/// CWH block B: the engine's specification, as the engine record holds it
/// from its redline on (the torque curve's last point is not in it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineSpec {
    pub redline: i32,
    pub idle: i32,
    pub gears: i32,
    pub final_drive: i32,
    pub gear_ratios: [i32; 6],
    pub reverse_ratio: i32,
    pub peak_torque: i32,
    pub torque_curve: [u8; 16],
}

impl EngineSpec {
    pub fn from_bytes(b: &[u8; BLOCK_B]) -> EngineSpec {
        let mut r = Reader { b, at: 0 };
        EngineSpec {
            redline: r.i32(),
            idle: r.i32(),
            gears: r.i32(),
            final_drive: r.i32(),
            gear_ratios: std::array::from_fn(|_| r.i32()),
            reverse_ratio: r.i32(),
            peak_torque: r.i32(),
            torque_curve: std::array::from_fn(|_| r.u8()),
        }
    }
}

/// A CWH file: (handling, engine), or `None` if the magic is wrong.
pub fn parse_cwh(cwh: &[u8]) -> Option<(Handling, EngineSpec)> {
    if cwh.len() < 4 + BLOCK_A + BLOCK_B || u32::from_le_bytes(cwh[..4].try_into().unwrap()) != CWH_MAGIC {
        return None;
    }
    let a: &[u8; BLOCK_A] = cwh[4..4 + BLOCK_A].try_into().unwrap();
    let b: &[u8; BLOCK_B] = cwh[4 + BLOCK_A..4 + BLOCK_A + BLOCK_B].try_into().unwrap();
    Some((Handling::from_bytes(a), EngineSpec::from_bytes(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fields where the CWH keeps them, past the words the game skips.
    #[test]
    fn block_a_fields_sit_where_the_file_keeps_them() {
        let mut bytes = [0u8; BLOCK_A];
        let mut put = |at: usize, v: i32| bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(0x04, 0x123);
        put(0x74, 0x2000);
        put(0x78, 0x3);
        put(0x80, 11);
        put(0x90, 22);
        put(0xa0 + 16 * 5 + 8, 33);
        put(0x118 + 4 * 5, 44);
        bytes[0x02] = 1;
        bytes[0x7c] = 4;
        let h = Handling::from_bytes(&bytes);
        assert_eq!((h.steer_lock, h.skill, h.all_terrain, h.wheel_count), (0x123, 0x2000, true, 4));
        assert!(!h.front_driven && h.rear_driven);
        assert_eq!((h.origin[0], h.size[0], h.mounts[5][2], h.diameters[5]), (11, 22, 33, 44));
    }
}
