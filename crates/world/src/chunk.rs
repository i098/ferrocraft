//! Chunk storage for 1.8.9 and decoding of the Chunk Data / Map Chunk Bulk payload.
//!
//! A block state is `id << 4 | meta` (16 bits). Sections are 16x16x16, indexed `y << 8 | z << 4 | x`.

use std::collections::HashMap;
use std::fmt;

const SECTION_BLOCKS: usize = 4096;
const BLOCK_BYTES: usize = SECTION_BLOCKS * 2;
const LIGHT_BYTES: usize = 2048;
const BIOME_BYTES: usize = 256;
const PLAINS: u8 = 1;

/// 16x16x16 block states of one chunk section.
pub type Section = [u16; SECTION_BLOCKS];

/// A 16x16x256 column of blocks. Missing sections are air.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    sections: [Option<Box<Section>>; 16],
    biomes: [u8; BIOME_BYTES],
}

/// The payload ended before every announced section was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Truncated {
    pub needed: usize,
    pub got: usize,
}

impl fmt::Display for Truncated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "chunk data truncated: need {} bytes, got {}",
            self.needed, self.got
        )
    }
}

impl std::error::Error for Truncated {}

fn index(x: usize, y: usize, z: usize) -> (usize, usize) {
    (y >> 4, (y & 15) << 8 | z << 4 | x)
}

impl Chunk {
    pub fn new(x: i32, z: i32) -> Chunk {
        Chunk {
            x,
            z,
            sections: Default::default(),
            biomes: [PLAINS; BIOME_BYTES],
        }
    }

    /// Biome id at chunk-local `x`, `z` (0..16); plains until a ground-up payload sets it.
    pub fn biome(&self, x: usize, z: usize) -> u8 {
        self.biomes[(z & 15) << 4 | (x & 15)]
    }

    /// Block state at chunk-local `x`, `z` (0..16) and `y` (0..256). Out of range is air.
    pub fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        if x >= 16 || z >= 16 || y >= 256 {
            return 0;
        }
        let (s, i) = index(x, y, z);
        self.sections[s].as_ref().map_or(0, |sec| sec[i])
    }

    /// Sets a block state; out-of-range positions are ignored.
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, state: u16) {
        if x >= 16 || z >= 16 || y >= 256 {
            return;
        }
        let (s, i) = index(x, y, z);
        match &mut self.sections[s] {
            Some(sec) => sec[i] = state,
            None if state != 0 => {
                let mut sec = Box::new([0u16; SECTION_BLOCKS]);
                sec[i] = state;
                self.sections[s] = Some(sec);
            }
            None => {}
        }
    }

    /// Blocks of section `index` (0..16, bottom to top), or `None` if it is all air.
    pub fn section(&self, index: usize) -> Option<&Section> {
        self.sections.get(index)?.as_deref()
    }

    /// Decodes the `data` of a Chunk Data packet (or one chunk of a Map Chunk Bulk packet).
    ///
    /// `bitmask` tells which sections are present; `sky_light` tells whether sky light arrays
    /// follow the block light arrays; `ground_up` adds the 256 biome bytes at the end.
    /// Returns the chunk and the number of bytes used. Light is read past, not kept.
    pub fn decode(
        x: i32,
        z: i32,
        bitmask: u16,
        data: &[u8],
        sky_light: bool,
        ground_up: bool,
    ) -> Result<(Chunk, usize), Truncated> {
        let count = bitmask.count_ones() as usize;
        let lights = if sky_light { 2 } else { 1 };
        let needed =
            count * (BLOCK_BYTES + lights * LIGHT_BYTES) + if ground_up { BIOME_BYTES } else { 0 };
        if data.len() < needed {
            return Err(Truncated {
                needed,
                got: data.len(),
            });
        }
        let mut chunk = Chunk::new(x, z);
        let present = (0..16).filter(|i| bitmask >> i & 1 == 1);
        for (n, s) in present.enumerate() {
            let bytes = &data[n * BLOCK_BYTES..(n + 1) * BLOCK_BYTES];
            let mut sec = Box::new([0u16; SECTION_BLOCKS]);
            for (dst, b) in sec.iter_mut().zip(bytes.as_chunks::<2>().0) {
                *dst = u16::from_le_bytes(*b);
            }
            chunk.sections[s] = Some(sec);
        }
        if ground_up {
            chunk
                .biomes
                .copy_from_slice(&data[needed - BIOME_BYTES..needed]);
        }
        Ok((chunk, needed))
    }
}

/// All loaded chunks; the block source for collision.
#[derive(Default, Debug)]
pub struct ChunkStore {
    chunks: HashMap<(i32, i32), Chunk>,
}

impl ChunkStore {
    pub fn new() -> ChunkStore {
        ChunkStore::default()
    }

    pub fn insert(&mut self, chunk: Chunk) {
        self.chunks.insert((chunk.x, chunk.z), chunk);
    }

    pub fn remove(&mut self, x: i32, z: i32) -> Option<Chunk> {
        self.chunks.remove(&(x, z))
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    pub fn get(&self, x: i32, z: i32) -> Option<&Chunk> {
        self.chunks.get(&(x, z))
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Chunk coordinates of every loaded chunk.
    pub fn positions(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.chunks.keys().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Block state at world position; unloaded chunks and `y` outside 0..256 are air.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u16 {
        if !(0..256).contains(&y) {
            return 0;
        }
        self.get(x >> 4, z >> 4).map_or(0, |c| {
            c.block((x & 15) as usize, y as usize, (z & 15) as usize)
        })
    }

    /// Sets a block in a loaded chunk; ignored when the chunk is not loaded.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state: u16) {
        if !(0..256).contains(&y) {
            return;
        }
        if let Some(c) = self.chunks.get_mut(&(x >> 4, z >> 4)) {
            c.set_block((x & 15) as usize, y as usize, (z & 15) as usize, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(sections: &[(usize, u16)], sky: bool, biomes: bool) -> (u16, Vec<u8>) {
        let mut mask = 0u16;
        let mut data = Vec::new();
        for &(s, state) in sections {
            mask |= 1 << s;
            let mut blocks = vec![0u8; BLOCK_BYTES];
            // block (1, 2, 3) of each section
            let i = (2 << 8 | 3 << 4 | 1) * 2;
            blocks[i..i + 2].copy_from_slice(&state.to_le_bytes());
            data.extend(blocks);
        }
        let lights = if sky { 2 } else { 1 };
        data.extend(vec![0xFF; sections.len() * lights * LIGHT_BYTES]);
        if biomes {
            data.extend((0..BIOME_BYTES).map(|i| i as u8));
        }
        (mask, data)
    }

    #[test]
    fn decodes_sparse_sections_and_reports_consumed() {
        let (mask, mut data) = payload(&[(0, 1 << 4), (5, 4 << 4 | 3)], true, true);
        let used = data.len();
        data.extend([9, 9, 9]); // next chunk of a bulk packet
        let (c, n) = Chunk::decode(-2, 7, mask, &data, true, true).unwrap();
        assert_eq!(n, used);
        assert_eq!((c.x, c.z), (-2, 7));
        assert_eq!(c.block(1, 2, 3), 1 << 4);
        assert_eq!(c.block(1, 5 * 16 + 2, 3), 4 << 4 | 3);
        assert_eq!(c.block(0, 0, 0), 0);
        assert!(c.section(1).is_none());
        // Biome bytes are indexed `z << 4 | x`.
        assert_eq!((c.biome(0, 0), c.biome(3, 2)), (0, 35));
    }

    #[test]
    fn truncated_data_is_an_error() {
        let (mask, data) = payload(&[(0, 16)], false, true);
        let err = Chunk::decode(0, 0, mask, &data[..data.len() - 1], false, true).unwrap_err();
        assert_eq!(err.needed, data.len());
    }

    #[test]
    fn store_maps_world_coordinates_incl_negative() {
        let mut s = ChunkStore::new();
        s.insert(Chunk::new(-1, -1));
        s.set_block(-1, 70, -16, 2 << 4);
        assert_eq!(s.get_block(-1, 70, -16), 2 << 4);
        assert_eq!(s.get(-1, -1).unwrap().block(15, 70, 0), 2 << 4);
        assert_eq!(s.get_block(-1, 300, -16), 0);
        assert_eq!(s.get_block(40, 70, 40), 0);
    }
}
