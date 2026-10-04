//! The best line (`<TRACK><n>.BLD`, `bestline_load` 0x8007b19c): the
//! racing line along the track, as points with their zone, their distance
//! along the lap and their heading. Computer cars follow it; a reset car is
//! put back on it.

use crate::math::Vec3;

/// One point of the line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinePoint {
    pub zone: u16,
    /// Distance along the lap, in tenths of the lap distance's unit.
    pub distance: u16,
    pub pos: Vec3,
    /// The way along the line, unit length.
    pub heading: Vec3,
}

/// A track's best line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BestLine {
    /// The lap's length, in the units of a car's lap distance
    /// (`bestline_load` keeps it at 0x801323d8 for the laps and results).
    pub lap_length: i32,
    /// The first stream, not yet understood.
    pub stream: Vec<u8>,
    pub points: Vec<LinePoint>,
}

impl BestLine {
    const MAGIC: u16 = 0xdf00;

    /// Parses a BLD file: a 22-byte header (magic, lap length, the two
    /// streams' lengths, six counts), a stream not yet understood, then the
    /// points, 16 bytes each.
    pub fn parse(b: &[u8]) -> Option<BestLine> {
        let u16_at = |at: usize| Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]));
        let i16_at = |at: usize| u16_at(at).map(|v| v as i16 as i32);
        if u16_at(0)? != Self::MAGIC {
            return None;
        }
        let lap_length = i32::from_le_bytes(b.get(2..6)?.try_into().ok()?);
        let (first, second) = (u16_at(6)? as usize, u16_at(8)? as usize);
        let stream = b.get(22..22 + first)?.to_vec();
        let points = (0..second / 16)
            .map(|k| {
                let at = 22 + first + 16 * k;
                Some(LinePoint {
                    zone: u16_at(at)?,
                    distance: u16_at(at + 2)?,
                    pos: [i16_at(at + 4)? << 12, i16_at(at + 6)? << 12, i16_at(at + 8)? << 12],
                    heading: [i16_at(at + 10)?, i16_at(at + 12)?, i16_at(at + 14)?],
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(BestLine { lap_length, stream, points })
    }

    /// 0x8007c418: the first point at or past `lap_distance` (wrapping to
    /// the start past the last) that is clear of the cars at `others`: no
    /// car within 120 inches of it on every axis. None for an empty line.
    pub fn free_point(&self, lap_distance: u32, others: &[Vec3]) -> Option<LinePoint> {
        let count = self.points.len();
        if count == 0 {
            return None;
        }
        let key = lap_distance / 10;
        let (mut low, mut high) = (0, count - 1);
        while low < high {
            let mid = (low + high) / 2;
            let d = self.points[mid].distance as u32;
            if d < key {
                low = mid + 1;
            } else if key < d {
                high = mid;
            } else {
                low = mid;
                high = mid;
            }
        }
        let mut at = if (self.points[low].distance as u32) < key { 0 } else { low };
        // Each crowded point moves the search on one; the original keeps
        // going round until it finds room.
        for _ in 0..=count {
            let p = self.points[at];
            let crowded = others.iter().any(|o| {
                let d = [0, 1, 2].map(|k| p.pos[k].wrapping_sub(o[k]).wrapping_abs());
                d[0].max(d[1]).max(d[2]) < 120 << 12
            });
            if !crowded {
                return Some(p);
            }
            at = (at + 1) % count;
        }
        None
    }
}
