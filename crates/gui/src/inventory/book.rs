//! The open book of the enchanting table window: `GuiEnchantment.tickBook` and the `ModelBook`
//! draw of `drawGuiContainerBackgroundLayer`, projected on the CPU with the vanilla matrices.

use super::Canvas;
use render::JavaRandom;
use world::inventory::Slot;

const TEXTURE: &str = "textures/entity/enchanting_table_book.png";
type V3 = [f32; 3];
type Placed = ([[f32; 2]; 4], [[f32; 2]; 4], u32);

/// Page flip and spread state of the window's book (`field_147071_v` and friends).
pub struct Book {
    page_flip: f32,
    page_flip_prev: f32,
    flip_t: f32,
    flip_a: f32,
    spread: f32,
    spread_prev: f32,
    /// Slot 0 at the last tick; a new stack turns the pages.
    seen: Slot,
    rng: JavaRandom,
}

impl Book {
    pub fn new() -> Self {
        // `new Random()`: vanilla seeds it from the clock.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        Book {
            page_flip: 0.0,
            page_flip_prev: 0.0,
            flip_t: 0.0,
            flip_a: 0.0,
            spread: 0.0,
            spread_prev: 0.0,
            seen: None,
            rng: JavaRandom::new(seed),
        }
    }

    /// `GuiEnchantment.tickBook`: `slot0` is the item slot, `open` whether any option has a level.
    pub fn tick(&mut self, slot0: &Slot, open: bool) {
        if *slot0 != self.seen {
            self.seen = slot0.clone();
            loop {
                self.flip_t += (self.rng.next_int(4) - self.rng.next_int(4)) as f32;
                if !(self.page_flip <= self.flip_t + 1.0 && self.page_flip >= self.flip_t - 1.0) {
                    break;
                }
            }
        }
        self.page_flip_prev = self.page_flip;
        self.spread_prev = self.spread;
        self.spread += if open { 0.2 } else { -0.2 };
        self.spread = self.spread.clamp(0.0, 1.0);
        let step = ((self.flip_t - self.page_flip) * 0.4).clamp(-0.2, 0.2);
        self.flip_a += (step - self.flip_a) * 0.9;
        self.page_flip += self.flip_a;
    }

    /// The model as `drawGuiContainerBackgroundLayer` draws it into the centred 320x240 viewport
    /// of a screen `size` in GUI units.
    pub fn draw(&self, c: &mut dyn Canvas, partial: f32, size: (i32, i32)) {
        for (pts, uv, argb) in self.placed(partial, size) {
            c.quad(TEXTURE, pts, uv, [argb; 4]);
        }
    }

    /// The screen quads of the model, back to front: corners, texture coordinates, light.
    fn placed(&self, partial: f32, size: (i32, i32)) -> Vec<Placed> {
        let spread = self.spread_prev + (self.spread - self.spread_prev) * partial;
        let flip = self.page_flip_prev + (self.page_flip - self.page_flip_prev) * partial;
        let angle = |off: f32| {
            let f = flip + off;
            ((f - f.trunc()) * 1.6 - 0.3).clamp(0.0, 1.0)
        };
        let quads = render::book_quads(angle(0.25), angle(0.75), spread);
        // `glViewport` counts from the bottom: the integer half goes below, the rest above.
        let (vx, vy) = (
            ((size.0 - 320) / 2) as f32,
            (size.1 - 240 - (size.1 - 240) / 2) as f32,
        );
        let open = 1.0 - spread;
        // The modelview matrix, last call first.
        let eye = |p: V3| {
            let p = rot_x(p, 180.0);
            let p = rot_y(p, -open * 90.0 - 90.0);
            let p = [p[0] + open * 0.2, p[1] + open * 0.1, p[2] + open * 0.25];
            let p = rot_z(rot_x(p, 20.0), 180.0);
            [p[0] * 5.0, p[1] * 5.0 + 3.3, p[2] * 5.0 - 16.0]
        };
        // `translate(-0.34, 0.23)` then `gluPerspective(90, 1.3333334, 9, 80)`.
        let screen = |p: V3| {
            let w = -p[2];
            let (nx, ny) = (p[0] / 1.333_333_4 / w - 0.34, p[1] / w + 0.23);
            [vx + (nx + 1.0) * 160.0, vy + (1.0 - ny) * 120.0]
        };
        let mut drawn: Vec<(f32, Placed, [f32; 4])> = quads
            .iter()
            .filter_map(|(pts, uv)| {
                let p = pts.map(eye);
                let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
                // Culling is still on from the world pass: a face turned away is not drawn.
                if n[0] * p[0][0] + n[1] * p[0][1] + n[2] * p[0][2] >= 0.0 {
                    return None;
                }
                let depth = p.iter().map(|q| q[2]).sum::<f32>();
                Some((depth, (p.map(screen), *uv, shade(n)), p.map(|q| -q[2])))
            })
            .collect();
        let shown = visible(&drawn.iter().map(|d| (d.1 .0, d.2)).collect::<Vec<_>>());
        let mut shown = shown.into_iter();
        drawn.retain(|_| shown.next() == Some(true));
        // Canvas has no depth buffer: far quads first, equal depth keeps draw order (`GL_LEQUAL`).
        drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
        drawn.into_iter().map(|(_, q, _)| q).collect()
    }
}

/// Samples per GUI unit of the visibility buffer.
const SAMPLES: f32 = 4.0;

/// Sorting by centre depth cannot order the faces inside the closed book. Vanilla's depth test
/// hides a face that is behind another one at every pixel, so this finds the quads (screen corners
/// and eye depth `w` of each corner) that win a sample of a depth buffer and keeps only those.
fn visible(quads: &[([[f32; 2]; 4], [f32; 4])]) -> Vec<bool> {
    let all = || quads.iter().flat_map(|q| q.0);
    let lo = |k: usize| all().map(|p| p[k]).fold(f32::MAX, f32::min).floor();
    let hi = |k: usize| all().map(|p| p[k]).fold(f32::MIN, f32::max).ceil();
    let origin = [lo(0), lo(1)];
    let width = ((hi(0) - origin[0]) * SAMPLES) as usize + 1;
    let height = ((hi(1) - origin[1]) * SAMPLES) as usize + 1;
    let mut nearest = vec![f32::MAX; width * height];
    // Every sample of a quad: its buffer index and eye depth.
    let samples = |(pts, w): &([[f32; 2]; 4], [f32; 4])| {
        let to_buffer = |i: usize| {
            [
                (pts[i][0] - origin[0]) * SAMPLES,
                (pts[i][1] - origin[1]) * SAMPLES,
            ]
        };
        let mut out = Vec::new();
        for t in [[0, 1, 2], [0, 2, 3]] {
            let [a, b, c] = t.map(to_buffer);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if area.abs() < 1e-6 {
                continue;
            }
            let span = |k: usize| {
                let (l, h) = (a[k].min(b[k]).min(c[k]), a[k].max(b[k]).max(c[k]));
                ((l - 0.5).ceil() as usize)..=((h - 0.5).floor() as usize)
            };
            for y in span(1) {
                for x in span(0).filter(|&x| x < width && y < height) {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let l1 = ((px - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (py - a[1])) / area;
                    let l2 = ((b[0] - a[0]) * (py - a[1]) - (px - a[0]) * (b[1] - a[1])) / area;
                    let l0 = 1.0 - l1 - l2;
                    if l0 >= 0.0 && l1 >= 0.0 && l2 >= 0.0 {
                        // Perspective: 1/w is linear on the screen.
                        let inv = l0 / w[t[0]] + l1 / w[t[1]] + l2 / w[t[2]];
                        out.push((y * width + x, 1.0 / inv));
                    }
                }
            }
        }
        out
    };
    for (i, w) in quads.iter().flat_map(samples) {
        nearest[i] = nearest[i].min(w);
    }
    quads
        .iter()
        .map(|q| samples(q).iter().any(|&(i, w)| w <= nearest[i] + 1e-3))
        .collect()
}

/// `enableStandardItemLighting` under an identity modelview: 0.4 ambient and two 0.6 lights.
fn shade(n: V3) -> u32 {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return 0xFF00_0000;
    }
    let lit: f32 = [[0.2, 1.0, -0.7], [-0.2, 1.0, 0.7]]
        .iter()
        .map(|l: &V3| {
            let ll = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            ((n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / (len * ll)).max(0.0)
        })
        .sum();
    let g = ((0.4 + 0.6 * lit).min(1.0) * 255.0 + 0.5) as u32;
    0xFF00_0000 | g << 16 | g << 8 | g
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `glRotatef(deg, 1, 0, 0)` on a point.
fn rot_x(p: V3, deg: f32) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

/// `glRotatef(deg, 0, 1, 0)` on a point.
fn rot_y(p: V3, deg: f32) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

/// `glRotatef(deg, 0, 0, 1)` on a point.
fn rot_z(p: V3, deg: f32) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The book of an open table lands in the left panel of the 176x166 window.
    #[test]
    fn open_book_sits_in_the_left_panel() {
        let mut book = Book::new();
        for _ in 0..10 {
            book.tick(&None, true);
        }
        let quads = book.placed(0.0, (427, 240));
        let (l, t) = ((427 - 176) / 2, (240 - 166) / 2);
        let pts: Vec<[f32; 2]> = quads.iter().flat_map(|q| q.0).collect();
        assert!(!pts.is_empty());
        let n = pts.len() as f32;
        let (cx, cy) = (
            pts.iter().map(|p| p[0]).sum::<f32>() / n,
            pts.iter().map(|p| p[1]).sum::<f32>() / n,
        );
        assert!((l as f32 + 15.0..l as f32 + 55.0).contains(&cx), "x {cx}");
        assert!((t as f32 + 10.0..t as f32 + 55.0).contains(&cy), "y {cy}");
    }
}

#[cfg(test)]
mod hidden_faces {
    use super::*;

    /// Closed book at page flip 0: the dark edge of the page stack lies behind the cover.
    #[test]
    fn closed_book_hides_the_faces_inside() {
        let quads = Book::new().placed(0.0, (427, 241));
        assert!(!quads.is_empty());
        assert!(quads.iter().all(|q| q.2 != 0xFF66_6666), "dark edge shows");
    }
}
