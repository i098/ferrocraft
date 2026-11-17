//! Attribute modifier lines of `ItemStack.getTooltip` (1.8.9), with `getAttributeModifiers`.

use super::tooltip::{enchantments, get, int};
use world::inventory::{Compound, Tag};

const BLUE: &str = "\u{a7}9";
const RED: &str = "\u{a7}c";
/// `Enchantment.damageProtection`'s damage type 0 id: Sharpness.
const SHARPNESS: i16 = 16;
/// `Item.itemModifierUUID` CB3F55D3-645C-4F38-A497-9C13A33DB5CF as (most, least).
const ITEM_MODIFIER_UUID: (i64, i64) = (
    0xCB3F_55D3_645C_4F38_u64 as i64,
    0xA497_9C13_A33D_B5CF_u64 as i64,
);
const DEFAULT_PREFIX: &str = "\u{a7}9+";
const DEFAULT_SUFFIX: &str = " Attack Damage";
/// `en_US.lang` `attribute.name.*`.
const NAMES: [(&str, &str); 7] = [
    ("horse.jumpStrength", "Horse Jump Strength"),
    ("zombie.spawnReinforcements", "Zombie Reinforcements"),
    ("generic.maxHealth", "Max Health"),
    ("generic.followRange", "Mob Follow Range"),
    ("generic.knockbackResistance", "Knockback Resistance"),
    ("generic.movementSpeed", "Speed"),
    ("generic.attackDamage", "Attack Damage"),
];

struct Modifier {
    attribute: String,
    amount: f64,
    operation: i32,
    item_modifier: bool,
}

fn long(c: &Compound, key: &str) -> i64 {
    match get(c, key) {
        Some(Tag::Byte(v)) => i64::from(*v),
        Some(Tag::Short(v)) => i64::from(*v),
        Some(Tag::Int(v)) => i64::from(*v),
        Some(Tag::Long(v)) => *v,
        _ => 0,
    }
}

/// `NBTTagCompound.getDouble`: any numeric tag, else 0.
fn double(c: &Compound, key: &str) -> f64 {
    match get(c, key) {
        Some(Tag::Float(v)) => f64::from(*v),
        Some(Tag::Double(v)) => *v,
        Some(_) => long(c, key) as f64,
        None => 0.0,
    }
}

fn string<'a>(c: &'a Compound, key: &str) -> &'a str {
    match get(c, key) {
        Some(Tag::String(s)) => s,
        _ => "",
    }
}

/// `SharedMonsterAttributes.readAttributeModifierFromNBT`: `None` where vanilla throws.
fn read(c: &Compound) -> Option<Modifier> {
    let uuid = (long(c, "UUIDMost"), long(c, "UUIDLeast"));
    let operation = int(c, "Operation");
    // ItemStack.getAttributeModifiers drops a zero UUID half; AttributeModifier rejects an empty name.
    if uuid.0 == 0 || uuid.1 == 0 || string(c, "Name").is_empty() || !(0..=2).contains(&operation) {
        return None;
    }
    Some(Modifier {
        attribute: string(c, "AttributeName").to_string(),
        amount: double(c, "Amount"),
        operation,
        item_modifier: uuid == ITEM_MODIFIER_UUID,
    })
}

/// The tag list when present, else the item defaults: `ItemSword` and the tools give one
/// `+N Attack Damage` modifier under `itemModifierUUID`.
fn modifiers(t: Option<&Compound>, defaults: &[&str]) -> Vec<Modifier> {
    if let Some(Tag::List(l)) = t.and_then(|t| get(t, "AttributeModifiers")) {
        return l
            .iter()
            .filter_map(|m| match m {
                Tag::Compound(m) => read(m),
                _ => None,
            })
            .collect();
    }
    defaults
        .iter()
        .filter_map(|l| {
            l.strip_prefix(DEFAULT_PREFIX)?
                .strip_suffix(DEFAULT_SUFFIX)?
                .parse()
                .ok()
        })
        .map(|amount| Modifier {
            attribute: "generic.attackDamage".to_string(),
            amount,
            operation: 0,
            item_modifier: true,
        })
        .collect()
}

/// `EnchantmentHelper.func_152377_a` for `EnumCreatureAttribute.UNDEFINED`: only Sharpness counts.
fn damage_bonus(t: Option<&Compound>) -> f32 {
    let Some(t) = t else { return 0.0 };
    enchantments(t, "ench")
        .into_iter()
        .filter(|(id, _)| *id == SHARPNESS)
        .map(|(_, lvl)| f32::from(lvl) * 1.25)
        .sum()
}

/// `ItemStack.DECIMALFORMAT`, `#.###`: half-even, no integer zero before a fraction.
fn decimal(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    match s.strip_prefix("0.") {
        Some(frac) => format!(".{frac}"),
        None => s.to_string(),
    }
}

/// The lines under the blank separator: `attribute.modifier.plus|take.N` per modifier.
pub(super) fn lines(t: Option<&Compound>, defaults: &[&str]) -> Vec<String> {
    let bonus = f64::from(damage_bonus(t));
    // shortcut: modifiers keep tag order; vanilla's HashMultimap orders them by attribute name hash.
    modifiers(t, defaults)
        .into_iter()
        .filter_map(|m| {
            let amount = m.amount + if m.item_modifier { bonus } else { 0.0 };
            let shown = if m.operation == 0 {
                amount
            } else {
                amount * 100.0
            };
            let name = NAMES.iter().find(|(k, _)| *k == m.attribute).map_or_else(
                || format!("attribute.name.{}", m.attribute),
                |(_, v)| (*v).to_string(),
            );
            let percent = if m.operation == 0 { "" } else { "%" };
            if amount > 0.0 {
                Some(format!("{BLUE}+{}{percent} {name}", decimal(shown)))
            } else if amount < 0.0 {
                Some(format!("{RED}-{}{percent} {name}", decimal(-shown)))
            } else {
                None
            }
        })
        .collect()
}
