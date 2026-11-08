//! Sky and block light of one chunk plus a one-block border, flood-filled like vanilla `World.checkLight`.
//! The server sends light data but the renderer is fed blocks only, so light is recomputed here.

use crate::blocks::{emission, light_opacity};
use crate::mesh::{Neighborhood, HEIGHT};
use std::collections::VecDeque;
use std::sync::LazyLock;

const W: usize = 18;

/// `(light_opacity, emission)` per block state.
static PROPS: LazyLock<Vec<(u8, u8)>> =
    LazyLock::new(|| (0..4096u16).map(|s| (light_opacity(s), emission(s))).collect());

pub struct Light {
    /// `sky << 4 | block` per cell, `x` and `z` in `-1..=16`.
    cells: Vec<u8>,
}

fn idx(x: i32, y: usize, z: i32) -> usize {
    (y * W + (z + 1) as usize) * W + (x + 1) as usize
}

impl Light {
    pub fn compute(hood: &Neighborhood) -> Light {
        let n = HEIGHT * W * W;
        let mut opacity = vec![0u8; n];
        let mut sky = vec![0u8; n];
        let mut block = vec![0u8; n];
        let loaded = |x: i32, z: i32| match (x, z) {
            (0..=15, 0..=15) => true,
            (-1, 0..=15) => hood.neighbors[0].is_some(),
            (16, 0..=15) => hood.neighbors[1].is_some(),
            (0..=15, -1) => hood.neighbors[2].is_some(),
            (0..=15, 16) => hood.neighbors[3].is_some(),
            _ => false,
        };
        let mut queue = VecDeque::new();
        for z in -1..=16 {
            for x in -1..=16 {
                let known = loaded(x, z);
                let mut level = 15u8;
                for y in (0..HEIGHT).rev() {
                    let i = idx(x, y, z);
                    if !known {
                        // Unloaded border columns block light; their light is copied from the chunk edge below.
                        opacity[i] = 15;
                        continue;
                    }
                    let (op, em) = PROPS[hood.block(x, y as i32, z) as usize & 4095];
                    opacity[i] = op;
                    if em > 0 {
                        block[i] = em;
                        queue.push_back(i);
                    }
                    if op > 0 {
                        level = level.saturating_sub(op);
                    }
                    sky[i] = level;
                }
            }
        }
        // Sky light that stays 15 straight down the column only needs to spread sideways where the neighbour is darker.
        let mut sky_queue = VecDeque::new();
        for z in -1..=16 {
            for x in -1..=16 {
                for y in 0..HEIGHT {
                    let i = idx(x, y, z);
                    if sky[i] > 1 && has_darker_neighbor(&sky, i) {
                        sky_queue.push_back(i);
                    }
                }
            }
        }
        spread(&opacity, &mut sky, sky_queue);
        spread(&opacity, &mut block, queue);

        let mut cells = vec![0u8; n];
        for y in 0..HEIGHT {
            for z in -1..=16 {
                for x in -1..=16 {
                    let (sx, sz) = if loaded(x, z) { (x, z) } else { (x.clamp(0, 15), z.clamp(0, 15)) };
                    let from = idx(sx, y, sz);
                    cells[idx(x, y, z)] = sky[from] << 4 | block[from];
                }
            }
        }
        Light { cells }
    }

    /// `(sky, block)` light levels; `x` and `z` in `-1..=16`.
    pub fn get(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if y >= HEIGHT as i32 {
            return (15, 0);
        }
        if y < 0 {
            return (0, 0);
        }
        let c = self.cells[idx(x.clamp(-1, 16), y as usize, z.clamp(-1, 16))];
        (c >> 4, c & 15)
    }
}

fn has_darker_neighbor(light: &[u8], i: usize) -> bool {
    let l = light[i];
    let y = i / (W * W);
    let rest = i % (W * W);
    let (z, x) = (rest / W, rest % W);
    (x > 0 && light[i - 1] + 1 < l)
        || (x + 1 < W && light[i + 1] + 1 < l)
        || (z > 0 && light[i - W] + 1 < l)
        || (z + 1 < W && light[i + W] + 1 < l)
        || (y > 0 && light[i - W * W] + 1 < l)
}

fn spread(opacity: &[u8], light: &mut [u8], mut queue: VecDeque<usize>) {
    while let Some(i) = queue.pop_front() {
        let l = light[i];
        if l <= 1 {
            continue;
        }
        let y = i / (W * W);
        let rest = i % (W * W);
        let (z, x) = (rest / W, rest % W);
        let mut visit = |ok: bool, j: usize| {
            if !ok || opacity[j] >= 15 {
                return;
            }
            let next = l.saturating_sub(opacity[j].max(1));
            if next > light[j] {
                light[j] = next;
                queue.push_back(j);
            }
        };
        visit(x > 0, i.wrapping_sub(1));
        visit(x + 1 < W, i + 1);
        visit(z > 0, i.wrapping_sub(W));
        visit(z + 1 < W, i + W);
        visit(y > 0, i.wrapping_sub(W * W));
        visit(y + 1 < HEIGHT, i + W * W);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::index;

    fn hood_of(blocks: &[u16]) -> Neighborhood<'_> {
        Neighborhood {
            origin: (0, 0),
            center: blocks,
            neighbors: [None; 4],
        }
    }

    fn chunk(f: impl Fn(usize, usize, usize) -> u16) -> Vec<u16> {
        let mut v = vec![0u16; 16 * 16 * HEIGHT];
        for y in 0..HEIGHT {
            for z in 0..16 {
                for x in 0..16 {
                    v[index(x, y, z)] = f(x, y, z);
                }
            }
        }
        v
    }

    #[test]
    fn open_air_is_full_sky_light() {
        let c = chunk(|_, y, _| if y < 60 { 1 << 4 } else { 0 });
        let l = Light::compute(&hood_of(&c));
        assert_eq!(l.get(5, 70, 5), (15, 0));
        assert_eq!(l.get(5, 30, 5), (0, 0));
    }

    #[test]
    fn torch_light_falls_off_by_one_per_block() {
        let c = chunk(|x, y, z| match (x, y, z) {
            (8, 40, 8) => 50 << 4 | 5,
            _ if y < 30 || y > 50 => 1 << 4,
            _ if (x, z) == (0, 0) => 0,
            _ => 0,
        });
        let l = Light::compute(&hood_of(&c));
        assert_eq!(l.get(8, 40, 8).1, 14);
        assert_eq!(l.get(11, 40, 8).1, 11);
    }

    #[test]
    fn roof_shades_the_floor_and_light_creeps_in_from_the_side() {
        // A one-block-high slab roof from x=0..=7 over open ground: light under it comes from x=8.
        let c = chunk(|x, y, _| match y {
            0..=59 => 1 << 4,
            62 if x < 8 => 1 << 4,
            _ => 0,
        });
        let l = Light::compute(&hood_of(&c));
        assert_eq!(l.get(7, 60, 4).0, 14);
        assert_eq!(l.get(4, 60, 4).0, 11);
    }
}
