//! The track's collision data, the `.SCP` file (`collision_scp_load`,
//! 0x8004c084; see `docs/formats/scp.md`).
//!
//! The track is cut into zones. A plane zone is a convex volume bounded by
//! planes; a plane is a wall, the ground, or a portal into a neighbouring
//! zone. A road zone (flag 0x800) is a stretch of road between two
//! cross-sections, each a left and a right edge point.

use crate::math::Vec3;

/// A zone, 20 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zone {
    /// [`Zone::ROAD`] and gameplay flags (laps, gravity, power-ups...).
    pub flags: u16,
    /// A parameter whose meaning depends on the flags: the first
    /// cross-section for a road zone, a gravity scale, a checkpoint number.
    pub param: u16,
    /// The ground's kind under a car in this zone.
    pub surface: u8,
    /// Where the zone's coordinates are measured from, in units of 256
    /// (2²⁰ in 4.12).
    pub origin: [i8; 3],
    /// A distance along the track, in tenths (inferred: checkpoints).
    pub distance: u16,
    pub plane_count: u8,
    pub first_plane: u16,
    /// Its fences: how many, and the first in [`Scp::fences`].
    pub fence_count: u8,
    pub first_fence: u16,
}

/// A fence (table D, 20 bytes): a straight barrier inside a zone, such as a
/// sign's post or a barrier's rail, from `centre` `half` either way along
/// `along`; cars are pushed back off it and bounce from its `normal`
/// (0x8005a548).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fence {
    pub centre: Vec3,
    /// Unit vectors, 4.12.
    pub along: Vec3,
    pub normal: Vec3,
    pub half: i32,
}

impl Zone {
    /// A stretch of road between cross-sections, not a plane volume.
    pub const ROAD: u16 = 0x800;

    pub fn is_road(&self) -> bool {
        self.flags & Self::ROAD != 0
    }

    /// The zone's origin in world units, 4.12.
    pub fn origin(&self) -> Vec3 {
        self.origin.map(|c| (c as i32) << 20)
    }
}

/// A plane, 12 bytes: `n · (p - zone origin) + d`, positive inside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plane {
    /// The normal, 4.12.
    pub normal: [i16; 3],
    /// The distance, in whole units (4.12 once shifted up 12).
    pub d: i16,
    /// For a portal, the zone beyond it.
    pub target: u16,
    /// 0: a portal; 2 and from 5: the ground; others: walls and the like.
    pub kind: u8,
}

impl Plane {
    pub fn normal(&self) -> Vec3 {
        self.normal.map(|c| c as i32)
    }

    /// `n · p + d` for a point `p` relative to the zone's origin.
    pub fn distance(&self, p: Vec3) -> i32 {
        crate::math::dot(p, self.normal()).wrapping_add((self.d as i32) << 12)
    }

    pub fn is_portal(&self) -> bool {
        self.kind == 0
    }

    /// Whether wheels stand on it.
    pub fn is_ground(&self) -> bool {
        self.kind == 2 || self.kind >= 5
    }
}

/// A road edge point, 12 bytes: a position, and a second point above the
/// road from it (the road's up there, once normalised), in whole units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgePoint {
    pub pos: [i16; 3],
    pub up: [i16; 3],
}

impl EdgePoint {
    /// The position in world units, 4.12.
    pub fn pos(&self) -> Vec3 {
        self.pos.map(|c| (c as i32) << 12)
    }

    /// The point above it, 4.12.
    pub fn up(&self) -> Vec3 {
        self.up.map(|c| (c as i32) << 12)
    }
}

/// A road cross-section, 24 bytes: its left and right edge points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Section {
    pub edges: [EdgePoint; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scp {
    /// The start grid: six points (4.12) and orientations (quaternions).
    pub grid: [(Vec3, [i32; 4]); 6],
    pub zones: Vec<Zone>,
    pub sections: Vec<Section>,
    pub planes: Vec<Plane>,
    /// The zones' fences (table D).
    pub fences: Vec<Fence>,
    /// Table E (12 bytes each), not yet understood.
    pub e: Vec<[u8; 12]>,
    /// The camera's path over the track before a race (0x8003acbc).
    pub flyby: Vec<Keyframe>,
}

/// A point on the flyby: where the camera is, and what it looks at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keyframe {
    pub eye: Vec3,
    pub target: Vec3,
}

impl Scp {
    /// Parses an SCP file (the six words at +216, which the game fills
    /// with pointers once loaded, are ignored).
    pub fn parse(b: &[u8]) -> Option<Scp> {
        let u16_at = |at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
        let i16_at = |at: usize| u16_at(at) as i16;
        let i32_at = |at: usize| i32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        if b.len() < 240 {
            return None;
        }
        let counts: Vec<usize> = (0..6).map(|k| i32_at(4 * k) as u32 as usize).collect();
        let sizes = [20, 24, 12, 20, 12, 32];
        let total: usize = counts.iter().zip(sizes).map(|(n, s)| n * s).sum();
        if 240 + total > b.len() {
            return None;
        }
        let mut at = 240;
        let mut table = |k: usize| {
            let start = at;
            at += counts[k] * sizes[k];
            (0..counts[k]).map(move |i| start + i * sizes[k])
        };
        let zones = table(0)
            .map(|z| Zone {
                flags: u16_at(z),
                param: u16_at(z + 2),
                surface: b[z + 4],
                origin: [b[z + 5] as i8, b[z + 6] as i8, b[z + 7] as i8],
                distance: u16_at(z + 8),
                plane_count: b[z + 10],
                first_plane: u16_at(z + 12),
                fence_count: b[z + 11],
                first_fence: u16_at(z + 14),
            })
            .collect();
        let edge = |e: usize| EdgePoint {
            pos: [i16_at(e), i16_at(e + 2), i16_at(e + 4)],
            up: [i16_at(e + 6), i16_at(e + 8), i16_at(e + 10)],
        };
        let sections = table(1).map(|s| Section { edges: [edge(s), edge(s + 12)] }).collect();
        let planes = table(2)
            .map(|p| Plane {
                normal: [i16_at(p), i16_at(p + 2), i16_at(p + 4)],
                d: i16_at(p + 6),
                target: u16_at(p + 8),
                kind: b[p + 10],
            })
            .collect();
        let fences = table(3)
            .map(|o| {
                let v = |at: usize| [i16_at(at), i16_at(at + 2), i16_at(at + 4)].map(|c| c as i32);
                Fence {
                    centre: v(o).map(|c| c << 12),
                    along: v(o + 6),
                    normal: v(o + 12),
                    half: (i16_at(o + 18) as i32) << 12,
                }
            })
            .collect();
        let e = table(4).map(|o| b[o..o + 12].try_into().unwrap()).collect();
        let flyby = table(5)
            .map(|o| Keyframe {
                eye: [i32_at(o), i32_at(o + 4), i32_at(o + 8)],
                target: [i32_at(o + 16), i32_at(o + 20), i32_at(o + 24)],
            })
            .collect();
        let grid = std::array::from_fn(|k| {
            let (p, q) = (24 + 16 * k, 120 + 16 * k);
            ([i32_at(p), i32_at(p + 4), i32_at(p + 8)], [i32_at(q), i32_at(q + 4), i32_at(q + 8), i32_at(q + 12)])
        });
        Some(Scp { grid, zones, sections, planes, fences, e, flyby })
    }

    /// The planes bounding `zone`.
    pub fn planes_of(&self, zone: &Zone) -> &[Plane] {
        let first = zone.first_plane as usize;
        &self.planes[first..first + zone.plane_count as usize]
    }

    /// 0x8004dc50: the plane zone a point is in: the first with the point on
    /// the inside of all its planes, or else the one it is least outside
    /// of. Road zones are not considered.
    pub fn zone_at(&self, p: Vec3) -> u16 {
        let mut best = (0xf15a_0000u32 as i32, 0u16);
        // Carried from zone to zone, as the original does: a zone with no
        // planes keeps the last one's.
        let mut nearest = 0;
        for (k, zone) in self.zones.iter().enumerate() {
            if zone.is_road() {
                continue;
            }
            let local = crate::math::sub(p, zone.origin());
            for (i, plane) in self.planes_of(zone).iter().enumerate() {
                let d = plane.distance(local);
                if i == 0 || d < nearest {
                    nearest = d;
                }
            }
            if nearest > 0 {
                return k as u16;
            }
            if best.0 < nearest {
                best = (nearest, k as u16);
            }
        }
        best.1
    }
}
