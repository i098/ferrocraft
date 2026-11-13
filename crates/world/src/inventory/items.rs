//! Item facts of 1.8.9: stack size, durability, subtypes, armor slot, names, tooltip lines,
//! furnace fuel and smelting inputs, item model per variant, and the creative tab lists.
//!
//! The `*.tsv` files beside this module come from the real 1.8.9 classes: `Dumper.java`
//! (same folder) prints them from the `Item` registry, `CreativeTabs`, `RenderItem` model
//! registrations, `FurnaceRecipes` and `TileEntityFurnace.isItemFuel`.

use std::collections::HashMap;
use std::sync::LazyLock;

pub const PUMPKIN: i16 = 86;
pub const BUCKET: i16 = 325;
pub const SKULL: i16 = 397;
pub const DIAMOND_CHESTPLATE: i16 = 311;
pub const ENCHANTED_BOOK: i16 = 403;
pub const LEATHER_HELMET: i16 = 298;
pub const LEATHER_BOOTS: i16 = 301;
pub const DYE: i16 = 351;
/// `EnumDyeColor.BLUE.getDyeDamage()`: lapis lazuli is dye 4.
pub const LAPIS_DAMAGE: i16 = 4;
const POTION: i16 = 373;
const GLASS_BOTTLE: i16 = 374;

/// `SlotPotion.canHoldPotion`: potions and glass bottles fit a brewing stand bottle slot.
pub fn holds_potion(id: i16) -> bool {
    id == POTION || id == GLASS_BOTTLE
}

/// `Item.isPotionIngredient`: the items with a `setPotionEffect` in 1.8.9, and pufferfish.
pub fn is_potion_ingredient(id: i16, damage: i16) -> bool {
    // gunpowder, redstone, glowstone dust, sugar, nether wart, ghast tear, blaze powder,
    // magma cream, spider eye, fermented spider eye, speckled melon, golden carrot, rabbit foot
    matches!(
        id,
        289 | 331 | 348 | 353 | 372 | 370 | 377 | 378 | 375 | 376 | 382 | 396 | 414
    ) || (id == 349 && damage == 3)
}

pub const SADDLE: i16 = 329;

/// `EntityHorse.isArmorItem`: iron, gold and diamond horse armor.
pub fn is_horse_armor(id: i16) -> bool {
    matches!(id, 417..=419)
}

/// `SlotBeacon.isItemValid`: iron, gold, diamond and emerald.
pub fn is_beacon_payment(id: i16) -> bool {
    matches!(id, 265 | 266 | 264 | 388)
}

#[derive(Debug)]
pub struct Item {
    pub id: i16,
    /// Registry name without `minecraft:`.
    pub key: &'static str,
    pub max_stack: i8,
    pub max_damage: i16,
    /// `Item.hasSubtypes`: the damage value picks a variant, not wear.
    pub subtypes: bool,
    /// `ItemArmor.armorType`: 0 helmet to 3 boots.
    pub armor: Option<u8>,
}

/// One `(id, meta)` variant.
#[derive(Debug)]
pub struct Variant {
    pub name: &'static str,
    /// `EnumRarity` ordinal: 0 common, 1 uncommon, 2 rare, 3 epic.
    pub rarity: u8,
    pub glint: bool,
    pub fuel: bool,
    pub smelts: bool,
    /// `Item.getColorFromItemStack` for model tint index 0 and 1 (RGB).
    pub tint: [u32; 2],
    /// `Item.addInformation` lines.
    pub info: Vec<&'static str>,
    /// Attribute modifier lines (`+7 Attack Damage`), after a blank line in the tooltip.
    pub attributes: Vec<&'static str>,
}

/// A creative inventory tab (`CreativeTabs`), in `creativeTabArray` order.
#[derive(Debug)]
pub struct Tab {
    pub index: u8,
    pub label: &'static str,
    pub icon: (i16, i16),
    /// Background, `textures/gui/container/creative_inventory/tab_<texture>`.
    pub texture: &'static str,
    pub scrollbar: bool,
    pub title: bool,
    /// `(id, meta, stored enchantment)`.
    pub entries: Vec<Entry>,
}

/// A creative tab entry: `(id, meta, stored enchantment (id, level))`.
pub type Entry = (i16, i16, Option<(i16, i16)>);

#[derive(Debug)]
pub struct Enchantment {
    pub id: i16,
    pub name: &'static str,
    pub max_level: i16,
}

fn rows(text: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| l.split('\t').collect())
}

fn num<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().expect("item table number")
}

fn lines(s: &'static str) -> Vec<&'static str> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split("\\n").collect()
    }
}

static ITEMS: LazyLock<HashMap<i16, Item>> = LazyLock::new(|| {
    rows(include_str!("items.tsv"))
        .map(|r| {
            let id = num(r[0]);
            let armor: i8 = num(r[5]);
            let item = Item {
                id,
                key: r[1],
                max_stack: num(r[2]),
                max_damage: num(r[3]),
                subtypes: r[4] == "1",
                armor: u8::try_from(armor).ok(),
            };
            (id, item)
        })
        .collect()
});

static VARIANTS: LazyLock<HashMap<(i16, i16), Variant>> = LazyLock::new(|| {
    rows(include_str!("variants.tsv"))
        .map(|r| {
            let v = Variant {
                name: r[2],
                rarity: num(r[3]),
                glint: r[4] == "1",
                fuel: r[5] == "1",
                smelts: r[6] == "1",
                tint: [
                    u32::from_str_radix(r[7], 16).expect("tint"),
                    u32::from_str_radix(r[8], 16).expect("tint"),
                ],
                info: lines(r[9]),
                attributes: lines(r[10]),
            };
            ((num(r[0]), num(r[1])), v)
        })
        .collect()
});

static MODELS: LazyLock<HashMap<(i16, i16), &'static str>> = LazyLock::new(|| {
    rows(include_str!("models.tsv"))
        .map(|r| ((num(r[0]), num(r[1])), r[2]))
        .collect()
});

static TABS: LazyLock<Vec<Tab>> = LazyLock::new(|| {
    rows(include_str!("tabs.tsv"))
        .map(|r| Tab {
            index: num(r[0]),
            label: r[1],
            icon: (num(r[2]), num(r[3])),
            texture: r[4],
            scrollbar: r[5] == "1",
            title: r[6] == "1",
            entries: r
                .get(7)
                .map_or("", |e| e)
                .split(' ')
                .filter(|e| !e.is_empty())
                .map(|e| {
                    let p: Vec<i16> = e.split(':').map(num).collect();
                    (p[0], p[1], (p.len() == 4).then(|| (p[2], p[3])))
                })
                .collect(),
        })
        .collect()
});

static ENCHANTMENTS: LazyLock<Vec<Enchantment>> = LazyLock::new(|| {
    rows(include_str!("enchantments.tsv"))
        .map(|r| Enchantment {
            id: num(r[0]),
            name: r[1],
            max_level: num(r[2]),
        })
        .collect()
});

/// What an unknown id behaves as: a plain stack of 64.
static UNKNOWN: Item = Item {
    id: -1,
    key: "unknown",
    max_stack: 64,
    max_damage: 0,
    subtypes: false,
    armor: None,
};

pub fn item(id: i16) -> &'static Item {
    ITEMS.get(&id).unwrap_or(&UNKNOWN)
}

/// The variant of a stack: damage picks it only for items with subtypes.
pub fn variant(id: i16, damage: i16) -> Option<&'static Variant> {
    let meta = if item(id).subtypes { damage } else { 0 };
    VARIANTS.get(&(id, meta)).or_else(|| VARIANTS.get(&(id, 0)))
}

/// `Item.getColorFromItemStack(stack, layer)` as RGB, for model layer `layer`: the variant tints
/// (potion liquid, spawn egg base and spots, the default leather brown) except that leather
/// armor takes its `display.color` (`dye`) on layer 0 (`ItemArmor.getColor`).
pub fn tint(id: i16, damage: i16, dye: Option<i32>, layer: usize) -> u32 {
    if let (true, Some(color), 0) = ((298..=301).contains(&id), dye, layer) {
        // `ArmorItem.getColorTint`: a negative color is white.
        return if color < 0 {
            0xFF_FFFF
        } else {
            color as u32 & 0xFF_FFFF
        };
    }
    variant(id, damage)
        .and_then(|v| v.tint.get(layer).copied())
        .unwrap_or(0xFF_FFFF)
}

/// `models/item/<name>.json` of a stack (`ItemModelMesher`: damageable items use meta 0).
///
/// A negative `damage` is a variant from [`held_variant`], drawn only in a hand.
pub fn model(id: i16, damage: i16) -> Option<&'static str> {
    if damage < 0 {
        // The table rows 261:1..3 and 346:1 are the pulling and cast models.
        return MODELS.get(&(id, -damage)).copied();
    }
    let meta = if item(id).max_damage > 0 { 0 } else { damage };
    MODELS
        .get(&(id, meta))
        .or_else(|| MODELS.get(&(id, 0)))
        .copied()
}

const BOW: i16 = 261;
const FISHING_ROD: i16 = 346;

/// The `damage` that `model` takes for a held item: `ItemRenderer.renderItemInHand` swaps the
/// model of a bow in use by its draw time (`useDuration - useTimer`: 18 or more is stage 2, over 13
/// stage 1, over 0 stage 0) and of a rod while the player has a bobber out. Else `damage`.
pub fn held_variant(id: i16, damage: i16, drawn: i32, cast: bool) -> i16 {
    match id {
        BOW if drawn > 0 => -1 - i16::from(drawn > 13) - i16::from(drawn >= 18),
        FISHING_ROD if cast => -1,
        _ => damage,
    }
}

/// Every item model name the table uses, for loading them up front.
pub fn model_names() -> impl Iterator<Item = &'static str> {
    let mut names: Vec<&'static str> = MODELS.values().copied().collect();
    names.sort_unstable();
    names.dedup();
    names.into_iter()
}

/// `TileEntityFurnace.isItemFuel`.
pub fn is_fuel(id: i16, damage: i16) -> bool {
    variant(id, damage).is_some_and(|v| v.fuel)
}

/// `FurnaceRecipes.getSmeltingResult(stack) != null`.
pub fn smelts(id: i16, damage: i16) -> bool {
    variant(id, damage).is_some_and(|v| v.smelts)
}

pub fn tabs() -> &'static [Tab] {
    &TABS
}

pub fn enchantment(id: i16) -> Option<&'static Enchantment> {
    ENCHANTMENTS.iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_hold_vanilla_facts() {
        assert_eq!(item(276).max_damage, 1561);
        assert_eq!(item(DIAMOND_CHESTPLATE).armor, Some(1));
        assert_eq!(item(368).max_stack, 16);
        assert_eq!(variant(35, 14).map(|v| v.name), Some("Red Wool"));
        assert_eq!(variant(276, 100).map(|v| v.name), Some("Diamond Sword"));
        assert_eq!(model(35, 8), Some("silver_wool"));
        assert!(is_fuel(263, 0) && smelts(15, 0) && !smelts(1, 0));
        assert_eq!(tabs().len(), 12);
        let bow = |drawn| model(BOW, held_variant(BOW, 0, drawn, false));
        assert_eq!(bow(0), Some("bow"));
        assert_eq!(bow(1), Some("bow_pulling_0"));
        assert_eq!(bow(13), Some("bow_pulling_0"));
        assert_eq!(bow(14), Some("bow_pulling_1"));
        assert_eq!(bow(17), Some("bow_pulling_1"));
        assert_eq!(bow(18), Some("bow_pulling_2"));
        let rod = |cast| model(FISHING_ROD, held_variant(FISHING_ROD, 5, 0, cast));
        assert_eq!(rod(false), Some("fishing_rod"));
        assert_eq!(rod(true), Some("fishing_rod_cast"));
        assert_eq!(tabs()[0].entries[0], (1, 0, None));
    }

    /// Tints dumped from the 1.8.9 jar: leather brown, a dyed cap, the overlay layer, a
    /// regeneration potion and a creeper egg.
    #[test]
    fn tint_follows_item_color() {
        assert_eq!(tint(298, 0, None, 0), 0xA06540);
        assert_eq!(tint(298, 0, Some(0xFF_0000), 0), 0xFF_0000);
        assert_eq!(tint(298, 0, Some(0xFF_0000), 1), 0xFF_FFFF);
        assert_eq!(tint(373, 16385, None, 0), 0xCD5CAB);
        assert_eq!(tint(373, 16385, None, 1), 0xFF_FFFF);
        assert_eq!(tint(383, 50, None, 0), 0x0D_A70B);
        assert_eq!(tint(383, 50, None, 1), 0);
    }
}
