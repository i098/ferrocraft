//! Chunk-section visibility of 1.8.9: `VisGraph` and `SetVisibility` (cave culling), the `Frustum` of
//! `ClippingHelperImpl` and the breadth-first walk of `RenderGlobal.setupTerrain` that the F3 `C:` and `E:` counters read.

use crate::{view_proj, Camera};
use std::collections::{HashSet, VecDeque};

/// Facings in `EnumFacing` order: down, up, north, south, west, east.
const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;
/// Unit step of each facing.
const STEP: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// Cell index of a section-local block, `x | z << 4 | y << 8`.
fn cell(x: usize, y: usize, z: usize) -> usize {
    x | z << 4 | y << 8
}

/// The facings of the section reached from the cell `start` through non-opaque cells (`VisGraph.floodFill`).
fn flood(opaque: &mut [bool; 4096], start: usize) -> u8 {
    let mut edges = 0u8;
    let mut queue = VecDeque::from([start]);
    opaque[start] = true;
    while let Some(i) = queue.pop_front() {
        let (x, y, z) = (i & 15, i >> 8 & 15, i >> 4 & 15);
        for (c, lo, hi) in [(y, DOWN, UP), (z, NORTH, SOUTH), (x, WEST, EAST)] {
            if c == 0 {
                edges |= 1 << lo;
            } else if c == 15 {
                edges |= 1 << hi;
            }
        }
        for step in &STEP {
            let n = [x as i32 + step[0], y as i32 + step[1], z as i32 + step[2]];
            if n.iter().all(|c| (0..16).contains(c)) {
                let j = cell(n[0] as usize, n[1] as usize, n[2] as usize);
                if !opaque[j] {
                    opaque[j] = true;
                    queue.push_back(j);
                }
            }
        }
    }
    edges
}

/// `VisGraph.computeVisibility` as `SetVisibility` bits: bit `from + to * 6` is set when a face `from` sees face `to`.
/// `opaque(x, y, z)` takes section-local coordinates.
pub fn section_visibility(opaque: impl Fn(usize, usize, usize) -> bool) -> u64 {
    let mut cells = [false; 4096];
    let mut solid = 0;
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                if opaque(x, y, z) {
                    cells[cell(x, y, z)] = true;
                    solid += 1;
                }
            }
        }
    }
    if solid < 256 {
        return (1 << 36) - 1;
    }
    if solid == 4096 {
        return 0;
    }
    let mut bits = 0u64;
    for i in 0..4096 {
        let (x, y, z) = (i & 15, i >> 8 & 15, i >> 4 & 15);
        let edge = [x, y, z].iter().any(|&c| c == 0 || c == 15);
        if edge && !cells[i] {
            let set = flood(&mut cells, i);
            for a in (0..6).filter(|a| set >> a & 1 == 1) {
                for b in (0..6).filter(|b| set >> b & 1 == 1) {
                    bits |= 1 << (a + b * 6) | 1 << (b + a * 6);
                }
            }
        }
    }
    bits
}

/// `VisGraph.getVisibleFacings`: the facings of the section reached from the block `(x, y, z)` (section-local).
pub fn visible_facings(opaque: impl Fn(usize, usize, usize) -> bool, at: [usize; 3]) -> u8 {
    let mut cells = [false; 4096];
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                cells[cell(x, y, z)] = opaque(x, y, z);
            }
        }
    }
    flood(&mut cells, cell(at[0], at[1], at[2]))
}

/// The view frustum: six inward planes `a x + b y + c z + d`, normalised.
pub struct Frustum {
    planes: [[f64; 4]; 6],
}

impl Frustum {
    /// The frustum of `cam` with the far plane `far` blocks away (vanilla: render distance * 16 * sqrt 2).
    pub fn new(cam: &Camera, aspect: f32, far: f64) -> Self {
        let m = view_proj(cam, aspect);
        let row = |r: usize| [0, 1, 2, 3].map(|c| f64::from(m[c][r]));
        let add = |a: [f64; 4], b: [f64; 4], s: f64| [0, 1, 2, 3].map(|i| a[i] + s * b[i]);
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let mut planes = [
            add(r3, r0, 1.0),
            add(r3, r0, -1.0),
            add(r3, r1, 1.0),
            add(r3, r1, -1.0),
            r2,
            r2,
        ];
        for p in &mut planes {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            p.iter_mut().for_each(|v| *v /= len);
        }
        // The near plane is 0.05 ahead of the eye; the far plane faces back at `far` from the eye.
        let near = planes[4];
        planes[5] = [-near[0], -near[1], -near[2], -near[3] + far - 0.05];
        Frustum { planes }
    }

    /// `Frustum.isBoundingBoxInFrustum`: false only when all eight corners lie outside one plane.
    pub fn contains(&self, min: [f64; 3], max: [f64; 3]) -> bool {
        self.planes.iter().all(|p| {
            (0..8).any(|i| {
                let c = |a: usize| if i >> a & 1 == 0 { min[a] } else { max[a] };
                p[0] * c(0) + p[1] * c(1) + p[2] * c(2) + p[3] > 0.0
            })
        })
    }
}

/// `RenderGlobal.setupTerrain`: the sections `(cx, sy, cz)` that survive frustum and cave culling.
/// `eye` is the camera, `feet_chunk` the chunk of the view entity's feet, `facings` what
/// [`visible_facings`] returned for the eye block; `vis(cx, sy, cz)` is a compiled section's
/// [`section_visibility`], `None` while it is not compiled (everything visible).
pub fn visible_sections(
    cam: &Camera,
    frustum: &Frustum,
    feet_chunk: (i32, i32),
    distance: i32,
    facings: impl FnOnce(i32, i32, i32) -> u8,
    vis: impl Fn(i32, i32, i32) -> Option<u64>,
) -> HashSet<(i32, i32, i32)> {
    type Key = (i32, i32, i32);
    let section_box = |(x, y, z): Key| {
        let min = [x * 16, y * 16, z * 16].map(f64::from);
        (min, min.map(|v| v + 16.0))
    };
    let in_frustum = |k: Key| {
        let (min, max) = section_box(k);
        frustum.contains(min, max)
    };
    let in_grid = |(x, y, z): Key| {
        (x - feet_chunk.0).abs() <= distance
            && (0..16).contains(&y)
            && (z - feet_chunk.1).abs() <= distance
    };
    let eye = cam.pos.map(|v| (v.floor() as i32).div_euclid(16));
    let mut found = HashSet::new();
    let mut seen: HashSet<Key> = HashSet::new();
    // (section, facing it was entered through, facings travelled)
    let mut queue: VecDeque<(Key, Option<usize>, u8)> = VecDeque::new();
    let eye_key = (eye[0], eye[1], eye[2]);
    if !in_grid(eye_key) || !(0..16).contains(&eye[1]) {
        let y = if cam.pos[1] > 0.0 { 15 } else { 0 };
        for x in -distance..=distance {
            for z in -distance..=distance {
                let k = (feet_chunk.0 + x, y, feet_chunk.1 + z);
                if in_frustum(k) {
                    seen.insert(k);
                    queue.push_back((k, None, 0));
                }
            }
        }
    } else {
        let mut set = facings(eye[0], eye[1], eye[2]);
        if set.count_ones() == 1 {
            let look = look(cam);
            let mut best = (f64::MIN, NORTH);
            for (f, s) in STEP.iter().enumerate() {
                let d: f64 = (0..3).map(|a| look[a] * f64::from(s[a])).sum();
                if d > best.0 {
                    best = (d, f);
                }
            }
            set &= !(1 << (best.1 ^ 1));
        }
        if set == 0 {
            found.insert(eye_key);
        } else {
            seen.insert(eye_key);
            queue.push_back((eye_key, None, 0));
        }
    }
    while let Some((k, from, travelled)) = queue.pop_front() {
        found.insert(k);
        let own = vis(k.0, k.1, k.2);
        for (f, s) in STEP.iter().enumerate() {
            let n = (k.0 + s[0], k.1 + s[1], k.2 + s[2]);
            let back = travelled >> (f ^ 1) & 1 == 1;
            let open = match (from, own) {
                (Some(from), Some(bits)) => bits >> ((from ^ 1) + f * 6) & 1 == 1,
                _ => true,
            };
            if !back && open && in_grid(n) && !seen.contains(&n) && in_frustum(n) {
                seen.insert(n);
                queue.push_back((n, Some(f), travelled | 1 << f));
            }
        }
    }
    found
}

/// `RenderGlobal.getViewVector`.
fn look(cam: &Camera) -> [f64; 3] {
    let (yaw, pitch) = (
        f64::from(cam.yaw).to_radians(),
        f64::from(cam.pitch).to_radians(),
    );
    [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_across_the_section_splits_its_faces() {
        // A solid slab at y = 8 closes the section vertically: down cannot see up, but down sees down.
        let bits = section_visibility(|_, y, _| y == 8);
        let sees = |a: usize, b: usize| bits >> (a + b * 6) & 1 == 1;
        assert!(!sees(DOWN, UP));
        assert!(sees(DOWN, DOWN));
        assert!(sees(DOWN, NORTH));
        assert!(sees(NORTH, EAST) && sees(UP, NORTH));
    }

    #[test]
    fn sparse_and_solid_sections() {
        assert_eq!(section_visibility(|_, _, _| false), (1 << 36) - 1);
        assert_eq!(section_visibility(|_, _, _| true), 0);
    }

    #[test]
    fn frustum_keeps_what_is_ahead() {
        let cam = Camera::default();
        let f = Frustum::new(&cam, 16.0 / 9.0, 200.0);
        // yaw 0 looks toward +z
        assert!(f.contains([-1.0, 63.0, 10.0], [1.0, 65.0, 12.0]));
        assert!(!f.contains([-1.0, 63.0, -12.0], [1.0, 65.0, -10.0]));
        assert!(!f.contains([-1.0, 63.0, 300.0], [1.0, 65.0, 302.0]));
    }
}
