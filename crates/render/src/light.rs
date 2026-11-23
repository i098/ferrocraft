//! Sky and block light of one chunk plus a two-block border, flood-filled like vanilla `World.checkLight`.
//! The server sends light data but the renderer is fed blocks only, so light is recomputed here.
//! The flood covers the chunk and its eight neighbours: light travels at most 15 blocks, so every source that can
//! reach the border lies inside that 48 x 48 block window.

use crate::blocks::{emission, light_opacity};
use crate::mesh::{index, Neighborhood, HEIGHT};
use crate::ChunkBlocks;
use std::collections::VecDeque;
use std::sync::LazyLock;

/// Width of the flooded window and the offset of chunk-local coordinate 0 in it.
const W: usize = 48;
const OFF: i32 = 16;
const BORDER: i32 = 2;
const RING: usize = 16 + 2 * BORDER as usize;

/// `(light_opacity, emission)` per block state.
static PROPS: LazyLock<Vec<(u8, u8)>> = LazyLock::new(|| {
    (0..4096u16)
        .map(|s| (light_opacity(s), emission(s)))
        .collect()
});

pub struct Light {
    /// `sky << 4 | block` per cell, `x` and `z` in `-2..=17`, `y` below `height`.
    cells: Vec<u8>,
    /// Layers computed; above them there is no block light.
    height: usize,
    sky_default: u8,
}

fn idx(x: i32, y: usize, z: i32) -> usize {
    (y * W + (z + OFF) as usize) * W + (x + OFF) as usize
}

fn ring_idx(x: i32, y: usize, z: i32) -> usize {
    (y * RING + (z + BORDER) as usize) * RING + (x + BORDER) as usize
}

impl Light {
    pub fn compute(hood: &Neighborhood) -> Light {
        // Above the highest block the column is open air at sky 15; block light reaches at most 14 cells above its source.
        let top = std::iter::once(hood.center)
            .chain(hood.neighbors.iter().flatten().copied())
            .filter_map(|b| b.iter().rposition(|&s| s != 0))
            .max()
            .map_or(0, |i| i / 256 + 1);
        let height = (top + 15).min(HEIGHT);
        let sky_default = if hood.has_skylight { 15 } else { 0 };
        if let Some(cells) = server_cells(hood, height) {
            return Light {
                cells,
                height,
                sky_default,
            };
        }
        let n = height * W * W;
        let mut opacity = vec![0u8; n];
        let mut sky = vec![0u8; n];
        let mut block = vec![0u8; n];
        let loaded = |x: i32, z: i32| hood.loaded(x.div_euclid(16), z.div_euclid(16));
        let mut queue = VecDeque::new();
        for z in -OFF..W as i32 - OFF {
            for x in -OFF..W as i32 - OFF {
                let known = loaded(x, z);
                let mut level = sky_default;
                for y in (0..height).rev() {
                    let i = idx(x, y, z);
                    if !known {
                        // Unloaded columns block light; the border ring copies its light from the chunk edge below.
                        opacity[i] = 15;
                        continue;
                    }
                    let (op, em) = PROPS[hood.block(x, y as i32, z) as usize & 4095];
                    opacity[i] = op;
                    if em > 0 {
                        block[i] = em;
                        queue.push_back(i);
                    }
                    // Chunk.generateSkylightMap: once the level is below 15, every block takes at least 1.
                    let step = if op == 0 && level != 15 { 1 } else { op };
                    level = level.saturating_sub(step);
                    sky[i] = level;
                }
            }
        }
        // Sky light that stays 15 straight down the column only needs to spread sideways where the neighbour is darker.
        if hood.has_skylight {
            let sky_queue = darker_edges(&sky, height);
            spread(&opacity, &mut sky, sky_queue, height);
        }
        spread(&opacity, &mut block, queue, height);

        let cells = flood_ring(height, loaded, &sky, &block);
        Light {
            cells,
            height,
            sky_default,
        }
    }

    /// `(sky, block)` light levels; `x` and `z` in `-1..=16`.
    pub fn get(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if y < 0 {
            return (0, 0);
        }
        self.sample(x.clamp(-1, 16), y, z.clamp(-1, 16))
    }

    pub(crate) fn sample(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if !(0..self.height as i32).contains(&y) {
            return (self.sky_default, 0);
        }
        let c = self.cells[ring_idx(x, y as usize, z)];
        (c >> 4, c & 15)
    }
}

fn flood_ring(height: usize, loaded: impl Fn(i32, i32) -> bool, sky: &[u8], block: &[u8]) -> Vec<u8> {
    let mut cells = vec![0u8; height * RING * RING];
    for y in 0..height {
        for z in -BORDER..16 + BORDER {
            for x in -BORDER..16 + BORDER {
                let (sx, sz) = if loaded(x, z) {
                    (x, z)
                } else {
                    (x.clamp(0, 15), z.clamp(0, 15))
                };
                let from = idx(sx, y, sz);
                cells[ring_idx(x, y, z)] = sky[from] << 4 | block[from];
            }
        }
    }
    cells
}

/// The light the server sent, as `Chunk.getLightFor`, for every ring cell; `None` unless the chunk and every loaded
/// neighbour kept it (a changed chunk is flooded again).
fn server_cells(hood: &Neighborhood, height: usize) -> Option<Vec<u8>> {
    let mut slots = [None; 9];
    for (slot, light) in hood.lights.iter().enumerate() {
        let (dx, dz) = (slot as i32 % 3 - 1, slot as i32 / 3 - 1);
        match light {
            Some(l) => slots[slot] = Some(*l),
            None if hood.loaded(dx, dz) => return None,
            None => {}
        }
    }
    slots[4]?;
    let mask = if hood.has_skylight { 0xff } else { 0x0f };
    let mut cells = vec![0u8; height * RING * RING];
    for y in 0..height {
        for z in -BORDER..16 + BORDER {
            for x in -BORDER..16 + BORDER {
                let slot = (z.div_euclid(16) + 1) * 3 + x.div_euclid(16) + 1;
                // An unloaded neighbour copies the edge of the chunk, as the flooded light does.
                let (slot, x, z) = match slots[slot as usize] {
                    Some(_) => (slot as usize, x, z),
                    None => (4, x.clamp(0, 15), z.clamp(0, 15)),
                };
                let at = index(x.rem_euclid(16) as usize, y, z.rem_euclid(16) as usize);
                cells[ring_idx(x, y, z)] = slots[slot]?[at] & mask;
            }
        }
    }
    Some(cells)
}

/// The light the server sent for a chunk as `sky << 4 | block` per block, or `None` when it sent none.
/// Sections the packet lacks are dark, except that the sky reaches every cell at or above the column's highest
/// light-blocking block (`Chunk.getLightFor` with `canSeeSky`).
pub fn expand_server(chunk: &impl ChunkBlocks, blocks: &[u16]) -> Option<Vec<u8>> {
    let any = |sky| (0..16).any(|s| chunk.light_section(s, sky).is_some());
    if !any(false) {
        return None;
    }
    let has_sky = any(true);
    let mut out = vec![0u8; blocks.len()];
    for z in 0..16 {
        for x in 0..16 {
            // Chunk.generateHeightMap: one above the highest block that blocks any light.
            let sky_from = (0..HEIGHT)
                .rev()
                .find(|&y| PROPS[blocks[index(x, y, z)] as usize & 4095].0 > 0)
                .map_or(0, |y| y + 1);
            for y in 0..HEIGHT {
                let i = index(x, y, z);
                let nibble = |sky| {
                    chunk
                        .light_section(y >> 4, sky)
                        .map(|a| a[(i & 4095) >> 1] >> ((i & 1) * 4) & 15)
                };
                let block = nibble(false).unwrap_or(0);
                let sky = match nibble(true) {
                    Some(v) => v,
                    None if has_sky && y >= sky_from => 15,
                    None => 0,
                };
                out[i] = sky << 4 | block;
            }
        }
    }
    Some(out)
}

fn darker_edges(sky: &[u8], height: usize) -> VecDeque<usize> {
    let mut sky_queue = VecDeque::new();
    for z in -OFF..W as i32 - OFF {
        for x in -OFF..W as i32 - OFF {
            for y in 0..height {
                let i = idx(x, y, z);
                if sky[i] > 1 && has_darker_neighbor(sky, i) {
                    sky_queue.push_back(i);
                }
            }
        }
    }
    sky_queue
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

fn spread(opacity: &[u8], light: &mut [u8], mut queue: VecDeque<usize>, height: usize) {
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
        visit(y + 1 < height, i + W * W);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::index;

    fn hood_of(blocks: &[u16]) -> Neighborhood<'_> {
        Neighborhood {
            origin: (0, 0),
            has_skylight: true,
            center: blocks,
            neighbors: [None; 8],
            biomes: [None; 9],
            lights: [None; 9],
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
            _ if !(30..=50).contains(&y) => 1 << 4,
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

    #[test]
    fn torch_light_crosses_into_the_neighbour_chunk() {
        let stone_below = |y: usize| if y < 30 { 1 << 4 } else { 0 };
        let center = chunk(|_, y, _| stone_below(y));
        let east = chunk(|x, y, z| match (x, y, z) {
            (1, 40, 8) => 50 << 4 | 5,
            _ => stone_below(y),
        });
        let hood = Neighborhood {
            origin: (0, 0),
            has_skylight: true,
            center: &center,
            neighbors: [None, None, None, None, Some(&east), None, None, None],
            biomes: [None; 9],
            lights: [None; 9],
        };
        let l = Light::compute(&hood);
        assert_eq!(l.get(16, 40, 8).1, 13);
        assert_eq!(l.get(15, 40, 8).1, 12);
    }

    #[test]
    fn torch_light_crosses_a_chunk_corner() {
        let stone_below = |y: usize| if y < 30 { 1 << 4 } else { 0 };
        let center = chunk(|_, y, _| stone_below(y));
        let corner = chunk(|x, y, z| match (x, y, z) {
            (1, 40, 1) => 50 << 4 | 5,
            _ => stone_below(y),
        });
        let hood = Neighborhood {
            origin: (0, 0),
            has_skylight: true,
            center: &center,
            neighbors: [
                Some(&center),
                Some(&center),
                Some(&center),
                Some(&center),
                Some(&center),
                Some(&center),
                Some(&center),
                Some(&corner),
            ],
            biomes: [None; 9],
            lights: [None; 9],
        };
        // Two blocks across x and z plus nothing in y: 14 - 4 = 10.
        assert_eq!(Light::compute(&hood).get(15, 40, 15).1, 10);
    }

    #[test]
    fn sky_light_keeps_decaying_below_leaves() {
        // A leaf canopy over a ceiling-less shaft: 15 -> 14 in the leaf, then one less per block below.
        let c = chunk(|_, y, _| match y {
            0..=9 => 1 << 4,
            80 => 18 << 4,
            _ => 0,
        });
        let l = Light::compute(&hood_of(&c));
        assert_eq!(l.get(5, 79, 5).0, 13);
        assert_eq!(l.get(5, 70, 5).0, 4);
        assert_eq!(l.get(5, 60, 5).0, 0);
    }

    #[test]
    fn server_light_is_used_instead_of_flooding() {
        // Stone floor, open air above, but the server says the cell at (5, 70, 5) is sky 3 block 9.
        let c = chunk(|_, y, _| if y < 60 { 1 << 4 } else { 0 });
        let mut light = vec![15u8 << 4; c.len()];
        light[index(5, 70, 5)] = 3 << 4 | 9;
        let mut hood = hood_of(&c);
        hood.lights[4] = Some(&light);
        let l = Light::compute(&hood);
        assert_eq!(l.get(5, 70, 5), (3, 9));
        assert_eq!(l.get(6, 70, 5), (15, 0));
        // A missing neighbour copy makes the whole hood flood again.
        let east = c.clone();
        hood.neighbors[4] = Some(&east);
        assert_eq!(Light::compute(&hood).get(5, 70, 5), (15, 0));
    }

    struct Sent(Vec<Option<[u8; 2048]>>);
    impl ChunkBlocks for Sent {
        fn chunk_x(&self) -> i32 {
            0
        }
        fn chunk_z(&self) -> i32 {
            0
        }
        fn block(&self, _: usize, _: usize, _: usize) -> u16 {
            0
        }
        fn light_section(&self, s: usize, sky: bool) -> Option<&[u8; 2048]> {
            self.0.get(s * 2 + usize::from(sky))?.as_ref()
        }
    }

    #[test]
    fn expand_server_reads_nibbles_and_fills_missing_sky_above_the_ground() {
        let c = chunk(|_, y, _| if y < 20 { 1 << 4 } else { 0 });
        let mut block = [0u8; 2048];
        // Section 0, cell (x=1, y=2, z=0): index 513, high nibble of byte 256.
        block[256] = 0x70;
        let mut sent = vec![None; 32];
        sent[0] = Some(block);
        sent[1] = Some([0; 2048]);
        let out = expand_server(&Sent(sent), &c).unwrap();
        assert_eq!(out[index(1, 2, 0)], 7);
        // Section 1 is missing: dark below the ground (y < 20), full sky above it.
        assert_eq!(out[index(0, 18, 0)], 0);
        assert_eq!(out[index(0, 25, 0)], 15 << 4);
        assert!(expand_server(&Sent(vec![None; 32]), &c).is_none());
    }
}
