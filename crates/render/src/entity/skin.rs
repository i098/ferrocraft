//! One texture for every entity model: 64x64 slots stacked vertically, the last slot plain white.

use crate::assets::{decode_png, AssetSource, Image};

pub const SLOT: u32 = 64;

/// Texture slots in atlas order; the path is relative to `assets/minecraft/textures/entity/`.
const SKINS: [&str; 8] = [
    "steve.png",
    "alex.png",
    "zombie/zombie.png",
    "skeleton/skeleton.png",
    "zombie_pigman.png",
    "creeper/creeper.png",
    "pig/pig.png",
    "cow/cow.png",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skin {
    Steve,
    Alex,
    Zombie,
    Skeleton,
    PigZombie,
    Creeper,
    Pig,
    Cow,
    /// Plain white: shapes take their color from the vertex tint.
    White,
}

pub const SLOTS: u32 = SKINS.len() as u32 + 1;

pub struct Skins {
    pub rgba: Vec<u8>,
    loaded: [bool; SKINS.len()],
}

impl Skins {
    pub fn width() -> u32 {
        SLOT
    }

    pub fn height() -> u32 {
        SLOT * SLOTS
    }

    /// All slots white: models draw flat-colored.
    pub fn flat() -> Skins {
        Skins {
            rgba: vec![255; (SLOT * SLOT * SLOTS * 4) as usize],
            loaded: [false; SKINS.len()],
        }
    }

    pub fn from_assets(source: &mut AssetSource) -> Skins {
        let mut s = Skins::flat();
        for (i, name) in SKINS.iter().enumerate() {
            let path = format!("assets/minecraft/textures/entity/{name}");
            let Some(img) = source.read(&path).and_then(|b| decode_png(&b)) else {
                continue;
            };
            s.blit(i as u32, &img);
            s.loaded[i] = true;
        }
        s
    }

    /// Copies a skin into its slot. A legacy 64x32 skin also fills the left arm and leg regions
    /// of the 64x64 layout, flipping each face as `ImageBufferDownload` does.
    fn blit(&mut self, slot: u32, img: &Image) {
        let base = (slot * SLOT) as usize;
        let w = SLOT as usize;
        self.rgba[base * w * 4..(base + w) * w * 4].fill(0);
        for y in 0..img.height.min(w) {
            let n = img.width.min(w);
            let dst = ((base + y) * w) * 4;
            self.rgba[dst..dst + n * 4].copy_from_slice(&img.rgba[y * img.width * 4..][..n * 4]);
        }
        if img.height <= 32 {
            // (source x, source y, destination x, rows) of each 4-wide face; the left limbs sit 32 rows down.
            const FACES: [(usize, usize, usize, usize); 12] = [
                (4, 16, 20, 4),
                (8, 16, 24, 4),
                (0, 20, 24, 12),
                (4, 20, 20, 12),
                (8, 20, 16, 12),
                (12, 20, 28, 12),
                (44, 16, 36, 4),
                (48, 16, 40, 4),
                (40, 20, 40, 12),
                (44, 20, 36, 12),
                (48, 20, 32, 12),
                (52, 20, 44, 12),
            ];
            for (sx, sy, dx, rows) in FACES {
                for y in 0..rows {
                    for x in 0..4 {
                        let from = ((base + sy + y) * w + sx + x) * 4;
                        let to = ((base + sy + 32 + y) * w + dx + 3 - x) * 4;
                        let px: [u8; 4] = self.rgba[from..from + 4].try_into().unwrap();
                        self.rgba[to..to + 4].copy_from_slice(&px);
                    }
                }
            }
        }
    }

    /// Whether the real texture of `skin` was found; a missing one is drawn white with a tint.
    pub fn has(&self, skin: Skin) -> bool {
        slot(skin).is_some_and(|s| self.loaded[s as usize])
    }

    /// Row offset in texels of the slot to sample for `skin`.
    pub fn v_offset(&self, skin: Skin) -> f32 {
        let s = match slot(skin) {
            Some(s) if self.loaded[s as usize] => s,
            _ => SLOTS - 1,
        };
        (s * SLOT) as f32
    }
}

fn slot(skin: Skin) -> Option<u32> {
    Some(match skin {
        Skin::Steve => 0,
        Skin::Alex => 1,
        Skin::Zombie => 2,
        Skin::Skeleton => 3,
        Skin::PigZombie => 4,
        Skin::Creeper => 5,
        Skin::Pig => 6,
        Skin::Cow => 7,
        Skin::White => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_skin_left_limb_faces_are_flipped_one_by_one() {
        // Every source texel holds its own (x, y), so the copied position is readable.
        let mut rgba = Vec::new();
        for y in 0..32u8 {
            for x in 0..64u8 {
                rgba.extend_from_slice(&[x, y, 0, 255]);
            }
        }
        let img = Image {
            width: 64,
            height: 32,
            rgba,
        };
        let mut skins = Skins::flat();
        skins.blit(0, &img);
        let at = |x: usize, y: usize| {
            let i = (y * SLOT as usize + x) * 4;
            (skins.rgba[i], skins.rgba[i + 1])
        };
        // ImageBufferDownload: right leg front (4,20) goes to the left leg front (20,52), flipped.
        assert_eq!(at(20, 52), (7, 20));
        assert_eq!(at(23, 63), (4, 31));
        // Right leg right face (0,20) becomes the left leg's left face (24,52).
        assert_eq!(at(24, 52), (3, 20));
        // Right arm front (44,20) goes to the left arm front (36,52); its top (44,16) to (36,48).
        assert_eq!(at(36, 52), (47, 20));
        assert_eq!(at(36, 48), (47, 16));
    }
}
