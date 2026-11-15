//! The horse textures. `LayeredTexture` draws the coat, the markings and the armor of a horse over each
//! other into one texture; this table holds that result for every coat and marking, because the entity
//! pass does not blend. The armor is drawn as a second layer (see `entity::horse`). Every texture is
//! 128x128 and takes two slots.

use super::{AssetSource, Image, Skins, SKINS};
use crate::assets::decode_png;

/// Coats (`HORSE_VARIANT_TEXTURE_PATHS`), in variant order.
const COATS: [&str; 7] = [
    "horse_white",
    "horse_creamy",
    "horse_chestnut",
    "horse_brown",
    "horse_black",
    "horse_gray",
    "horse_darkbrown",
];
/// Markings (`HORSE_MARKINGS_TEXTURE_PATHS` after the empty first entry), in marking order.
const MARKINGS: [&str; 4] = [
    "horse_markings_white",
    "horse_markings_whitefield",
    "horse_markings_whitedots",
    "horse_markings_blackdots",
];
/// Donkey, mule, zombie horse and skeleton horse (`getTextureLocation`).
const OTHER: [&str; 4] = ["donkey", "mule", "horse_zombie", "horse_skeleton"];
/// Iron, gold and diamond horse armor (`HORSE_ARMOR_TEXTURE_PATHS` after the empty first entry).
const ARMOR: [&str; 3] = [
    "armor/horse_armor_iron",
    "armor/horse_armor_gold",
    "armor/horse_armor_diamond",
];

pub const COATS_COUNT: u8 = COATS.len() as u8;
pub const MARKINGS_COUNT: u8 = MARKINGS.len() as u8;
/// First index of the donkey, mule, zombie horse and skeleton horse.
pub const OTHER_FIRST: u8 = COATS_COUNT;
/// Index of the iron armor; gold and diamond follow.
pub const ARMOR_FIRST: u8 = OTHER_FIRST + OTHER.len() as u8;
const MARKED_FIRST: u8 = ARMOR_FIRST + ARMOR.len() as u8;
/// Textures in the table.
pub const COUNT: usize = MARKED_FIRST as usize + COATS.len() * MARKINGS.len();
/// Slots the table takes in the atlas.
pub const SLOTS: u32 = COUNT as u32 * 2;

/// Index of coat `coat` with marking `marking` (1 to 4) drawn over it.
pub const fn marked(coat: u8, marking: u8) -> u8 {
    MARKED_FIRST + coat * MARKINGS_COUNT + marking - 1
}

/// The slot of texture `index`.
pub const fn slot(index: u8) -> u32 {
    SKINS.len() as u32 + index as u32 * 2
}

fn read(source: &mut AssetSource, name: &str) -> Option<Image> {
    let path = format!("assets/minecraft/textures/entity/horse/{name}.png");
    decode_png(&source.read(&path)?).filter(|i| i.width == 128 && i.height == 128)
}

/// `Graphics.drawImage` of `top` over `base`: source-over with straight alpha.
fn over(base: &Image, top: &Image) -> Image {
    let mut rgba = base.rgba.clone();
    let (px, _) = rgba.as_chunks_mut::<4>();
    for (b, t) in px.iter_mut().zip(top.rgba.as_chunks::<4>().0) {
        let (ta, ba) = (u32::from(t[3]), u32::from(b[3]));
        let out = ta * 255 + ba * (255 - ta);
        if out == 0 {
            continue;
        }
        for i in 0..3 {
            b[i] = ((u32::from(t[i]) * ta * 255 + u32::from(b[i]) * ba * (255 - ta)) / out) as u8;
        }
        b[3] = (out / 255) as u8;
    }
    Image {
        width: base.width,
        height: base.height,
        rgba,
    }
}

impl Skins {
    /// Reads the horse textures and the coat and marking composites; a missing file leaves its slots white.
    pub(super) fn load_horses(&mut self, source: &mut AssetSource) {
        let coats: Vec<Option<Image>> = COATS.iter().map(|n| read(source, n)).collect();
        let markings: Vec<Option<Image>> = MARKINGS.iter().map(|n| read(source, n)).collect();
        let plain = COATS.iter().chain(&OTHER).chain(&ARMOR);
        for (i, name) in plain.enumerate() {
            if let Some(img) = read(source, name) {
                self.store_horse(i as u8, &img);
            }
        }
        for (c, coat) in coats.iter().enumerate() {
            for (m, marking) in markings.iter().enumerate() {
                if let (Some(coat), Some(marking)) = (coat, marking) {
                    self.store_horse(marked(c as u8, m as u8 + 1), &over(coat, marking));
                }
            }
        }
    }

    fn store_horse(&mut self, index: u8, img: &Image) {
        self.blit_plain(slot(index), img);
        self.horse_loaded[usize::from(index)] = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: [u8; 4]) -> Image {
        Image {
            width: 1,
            height: 1,
            rgba: rgba.to_vec(),
        }
    }

    #[test]
    fn a_marking_blends_over_the_coat_and_adds_to_nothing() {
        let coat = pixel([200, 100, 0, 255]);
        assert_eq!(over(&coat, &pixel([0, 0, 0, 0])).rgba, [200, 100, 0, 255]);
        assert_eq!(over(&coat, &pixel([255, 255, 255, 255])).rgba, [255; 4]);
        let half = over(&coat, &pixel([100, 100, 100, 128])).rgba;
        assert_eq!(half, [149, 100, 50, 255]);
        let bare = over(&pixel([9, 9, 9, 0]), &pixel([0, 0, 0, 0]));
        assert_eq!(bare.rgba, [9, 9, 9, 0]);
    }

    #[test]
    fn the_table_is_dense() {
        assert_eq!(marked(6, 4) as usize, COUNT - 1);
        assert_eq!(slot(ARMOR_FIRST + 2) + 2, slot(marked(0, 1)));
    }
}
