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
    /// with the mirrored right ones, which is what the 64x64 layout expects.
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
            // (source x, source y) of the right limb region, (destination x, y) of the left one.
            for ((sx, sy), (dx, dy)) in [((40, 16), (32, 48)), ((0, 16), (16, 48))] {
                for y in 0..16 {
                    for x in 0..16 {
                        let from = ((base + sy + y) * w + sx + (15 - x)) * 4;
                        let to = ((base + dy + y) * w + dx + x) * 4;
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
