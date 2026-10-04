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
    /// Non-zero: the front wheels are driven, steer; the rear likewise.
    pub front_driven: u8,
    pub front_steers: u8,
    pub rear_driven: u8,
    pub rear_steers: u8,
    pub unknown_04: i32,
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
    pub unknown_6c: [i32; 2],
    /// Scaled by the race's difficulty for the cars it drives (between two
    /// tuning bytes, held to 4).
    pub skill: i32,
    /// Bit 0: the dragging surface does not drag.
    pub unknown_78: i32,
    pub wheel_count: u8,
    pub unknown_7d: [u8; 3],
    /// The point wheel mounts are measured from, and a word after it.
    pub origin: Vec3,
    pub origin_pad: i32,
    /// Width, length, height, and a word after them.
    pub size: Vec3,
    pub size_pad: i32,
    /// Each wheel's mount point and the word after it (two halfwords).
    pub mounts: [(Vec3, i32); 6],
    pub wheel_10: [i32; 6],
    pub diameters: [i32; 6],
    pub unknown_130: i32,
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
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn i32(&mut self, v: i32) {
        self.0.extend(v.to_le_bytes());
    }
    fn vec3(&mut self, v: Vec3) {
        v.into_iter().for_each(|x| self.i32(x));
    }
}

impl Handling {
    /// Block A, as it lies in the CWH (after the magic) or the car record.
    pub fn from_bytes(b: &[u8; BLOCK_A]) -> Handling {
        let mut r = Reader { b, at: 0 };
        let (front_driven, front_steers, rear_driven, rear_steers) = (r.u8(), r.u8(), r.u8(), r.u8());
        let unknown_04 = r.i32();
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
            front_driven,
            front_steers,
            rear_driven,
            rear_steers,
            unknown_04,
            mass,
            cg_along,
            cg_up,
            brake_grip,
            brake_bias,
            drag,
            front,
            rear,
            air_power: AirPower { pitch: r.i32(), roll: r.i32(), yaw: r.i32() },
            unknown_6c: [r.i32(), r.i32()],
            skill: r.i32(),
            unknown_78: r.i32(),
            wheel_count: r.u8(),
            unknown_7d: [r.u8(), r.u8(), r.u8()],
            origin: r.vec3(),
            origin_pad: r.i32(),
            size: r.vec3(),
            size_pad: r.i32(),
            mounts: std::array::from_fn(|_| (r.vec3(), r.i32())),
            wheel_10: std::array::from_fn(|_| r.i32()),
            diameters: std::array::from_fn(|_| r.i32()),
            unknown_130: r.i32(),
        }
    }

    pub fn to_bytes(&self) -> [u8; BLOCK_A] {
        let mut w = Writer::default();
        for b in [self.front_driven, self.front_steers, self.rear_driven, self.rear_steers] {
            w.u8(b);
        }
        w.i32(self.unknown_04);
        for v in [self.mass, self.cg_along, self.cg_up, self.brake_grip, self.brake_bias, self.drag] {
            w.i32(v);
        }
        for v in [self.front.downforce, self.rear.downforce, self.front.downforce_scale, self.rear.downforce_scale] {
            w.i32(v);
        }
        for a in [self.front, self.rear] {
            for v in [a.stiffness, a.grip, a.travel, a.damp_in, a.damp_out, a.ride_height] {
                w.i32(v);
            }
        }
        for v in [self.air_power.pitch, self.air_power.roll, self.air_power.yaw] {
            w.i32(v);
        }
        self.unknown_6c.into_iter().for_each(|v| w.i32(v));
        w.i32(self.skill);
        w.i32(self.unknown_78);
        w.u8(self.wheel_count);
        self.unknown_7d.into_iter().for_each(|v| w.u8(v));
        w.vec3(self.origin);
        w.i32(self.origin_pad);
        w.vec3(self.size);
        w.i32(self.size_pad);
        for (m, pad) in self.mounts {
            w.vec3(m);
            w.i32(pad);
        }
        self.wheel_10.into_iter().for_each(|v| w.i32(v));
        self.diameters.into_iter().for_each(|v| w.i32(v));
        w.i32(self.unknown_130);
        w.0.try_into().expect("block A is 308 bytes")
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

    #[test]
    fn block_a_round_trips() {
        let bytes: [u8; BLOCK_A] = std::array::from_fn(|i| (i * 7 + 3) as u8);
        assert_eq!(Handling::from_bytes(&bytes).to_bytes(), bytes);
    }
}
