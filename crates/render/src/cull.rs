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
    let mut cells = opaque_cells(opaque);
    let solid = cells.iter().filter(|&&c| c).count();
    if solid < 256 {
        return (1 << 36) - 1;
    }
    if solid == 4096 {
        return 0;
    }
    let mut bits = 0u64;
    for i in 0..4096 {
        if !on_edge(i) || cells[i] {
            continue;
        }
        let set = flood(&mut cells, i);
        let open = || (0..6).filter(move |a| set >> a & 1 == 1);
        for (a, b) in open().flat_map(|a| open().map(move |b| (a, b))) {
            bits |= 1 << (a + b * 6);
        }
    }
    bits
}

/// The opacity of the 4096 cells of a section, indexed by [`cell`].
fn opaque_cells(opaque: impl Fn(usize, usize, usize) -> bool) -> [bool; 4096] {
    let mut cells = [false; 4096];
    for (i, c) in cells.iter_mut().enumerate() {
        *c = opaque(i & 15, i >> 8 & 15, i >> 4 & 15);
    }
    cells
}

/// Whether the cell lies on a face of the section.
fn on_edge(i: usize) -> bool {
    [i & 15, i >> 8 & 15, i >> 4 & 15]
        .iter()
        .any(|&c| c == 0 || c == 15)
}

/// `VisGraph.getVisibleFacings`: the facings of the section reached from the block `(x, y, z)` (section-local).
pub fn visible_facings(opaque: impl Fn(usize, usize, usize) -> bool, at: [usize; 3]) -> u8 {
    flood(&mut opaque_cells(opaque), cell(at[0], at[1], at[2]))
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

/// A section `(chunk x, section y, chunk z)`.
type Key = (i32, i32, i32);

fn section_in_frustum(frustum: &Frustum, (x, y, z): Key) -> bool {
    let min = [x * 16, y * 16, z * 16].map(f64::from);
    frustum.contains(min, min.map(|v| v + 16.0))
}

/// Inside the render distance around the feet chunk and the 16 section rows.
fn in_grid((x, y, z): Key, feet: (i32, i32), distance: i32) -> bool {
    (x - feet.0).abs() <= distance && (0..16).contains(&y) && (z - feet.1).abs() <= distance
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
    let in_frustum = |k: Key| section_in_frustum(frustum, k);
    let in_grid = |k: Key| in_grid(k, feet_chunk, distance);
    let eye = cam.pos.map(|v| (v.floor() as i32).div_euclid(16));
    let eye_key = (eye[0], eye[1], eye[2]);
    let mut found = HashSet::new();
    let mut seen: HashSet<Key> = HashSet::new();
    // (section, facing it was entered through, facings travelled)
    let mut queue: VecDeque<(Key, Option<usize>, u8)> = VecDeque::new();
    let mut start = |k: Key| {
        seen.insert(k);
        queue.push_back((k, None, 0));
    };
    if !in_grid(eye_key) || !(0..16).contains(&eye[1]) {
        let y = if cam.pos[1] > 0.0 { 15 } else { 0 };
        let around = (-distance..=distance).flat_map(|x| {
            (-distance..=distance).map(move |z| (feet_chunk.0 + x, y, feet_chunk.1 + z))
        });
        around.filter(|&k| in_frustum(k)).for_each(&mut start);
    } else if eye_exits(facings(eye[0], eye[1], eye[2]), cam) == 0 {
        found.insert(eye_key);
    } else {
        start(eye_key);
    }
    while let Some((k, from, travelled)) = queue.pop_front() {
        found.insert(k);
        let own = vis(k.0, k.1, k.2);
        for (f, s) in STEP.iter().enumerate() {
            let n = (k.0 + s[0], k.1 + s[1], k.2 + s[2]);
            let back = travelled >> (f ^ 1) & 1 == 1;
            if !back && passes(from, own, f) && in_grid(n) && !seen.contains(&n) && in_frustum(n) {
                seen.insert(n);
                queue.push_back((n, Some(f), travelled | 1 << f));
            }
        }
    }
    found
}

/// The facings the view can leave the eye's section by: one open facing is narrowed to what the look direction allows.
fn eye_exits(mut set: u8, cam: &Camera) -> u8 {
    if set.count_ones() == 1 {
        set &= !(1 << (facing_along(&look(cam)) ^ 1));
    }
    set
}

/// The facing whose step has the largest dot product with `look`.
fn facing_along(look: &[f64; 3]) -> usize {
    let dot = |s: &[i32; 3]| (0..3).map(|a| look[a] * f64::from(s[a])).sum::<f64>();
    (0..6)
        .reduce(|b, f| if dot(&STEP[f]) > dot(&STEP[b]) { f } else { b })
        .unwrap_or(NORTH)
}

/// Whether a section entered through `from` lets the view out through `to` (`SetVisibility.isVisible`).
fn passes(from: Option<usize>, vis: Option<u64>, to: usize) -> bool {
    match (from, vis) {
        (Some(from), Some(bits)) => bits >> ((from ^ 1) + to * 6) & 1 == 1,
        _ => true,
    }
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
