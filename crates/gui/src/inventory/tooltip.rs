//! Item tooltips: `ItemStack.getTooltip` and `GuiScreen.drawHoveringText` of 1.8.9.

use super::Canvas;
use world::inventory::items::{self, ENCHANTED_BOOK};
use world::inventory::{Compound, ItemStack, Tag};

const ITALIC: &str = "\u{a7}o";
const RESET: &str = "\u{a7}r";
const GRAY: &str = "\u{a7}7";
const BLUE: &str = "\u{a7}9";
const DARK_PURPLE: &str = "\u{a7}5";
/// `EnumRarity` colors: common, uncommon, rare, epic.
const RARITY: [&str; 4] = ["\u{a7}f", "\u{a7}e", "\u{a7}b", "\u{a7}d"];
const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];

pub fn get<'a>(c: &'a Compound, key: &str) -> Option<&'a Tag> {
    c.iter().find(|(k, _)| k == key).map(|(_, t)| t)
}

fn compound<'a>(c: &'a Compound, key: &str) -> Option<&'a Compound> {
    match get(c, key)? {
        Tag::Compound(c) => Some(c),
        _ => None,
    }
}

/// `NBTTagCompound.getInteger` / `getShort`: any numeric tag, else 0.
fn int(c: &Compound, key: &str) -> i32 {
    match get(c, key) {
        Some(Tag::Byte(v)) => i32::from(*v),
        Some(Tag::Short(v)) => i32::from(*v),
        Some(Tag::Int(v)) => *v,
        Some(Tag::Long(v)) => *v as i32,
        _ => 0,
    }
}

fn list<'a>(c: &'a Compound, key: &str) -> &'a [Tag] {
    match get(c, key) {
        Some(Tag::List(l)) => l,
        _ => &[],
    }
}

fn tag(s: &ItemStack) -> Option<&Compound> {
    s.nbt.as_ref().map(|n| &n.root)
}

/// `ItemStack.getDisplayName`: the custom name in `display.Name`, else the variant name.
pub fn name(s: &ItemStack) -> (String, bool) {
    if let Some(Tag::String(n)) = tag(s)
        .and_then(|t| compound(t, "display"))
        .and_then(|d| get(d, "Name"))
    {
        return (n.clone(), true);
    }
    let base = items::variant(s.id, s.damage)
        .map_or_else(|| items::item(s.id).key.to_string(), |v| v.name.to_string());
    (base, false)
}

/// `(id, level)` pairs of an enchantment list tag.
fn enchantments(t: &Compound, key: &str) -> Vec<(i16, i16)> {
    list(t, key)
        .iter()
        .filter_map(|e| match e {
            Tag::Compound(e) => Some((int(e, "id") as i16, int(e, "lvl") as i16)),
            _ => None,
        })
        .collect()
}

/// `Enchantment.getTranslatedName`.
fn enchantment_name(id: i16, lvl: i16) -> Option<String> {
    let e = items::enchantment(id)?;
    let level = usize::try_from(lvl - 1)
        .ok()
        .and_then(|i| ROMAN.get(i))
        .map_or_else(|| format!("enchantment.level.{lvl}"), |r| (*r).to_string());
    Some(format!("{} {level}", e.name))
}

/// `ItemStack.isItemEnchanted`.
pub fn enchanted(s: &ItemStack) -> bool {
    tag(s).is_some_and(|t| !list(t, "ench").is_empty())
}

/// `ItemStack.hasEffect`: enchanted stacks plus the items that always shine.
pub fn glint(s: &ItemStack) -> bool {
    enchanted(s)
        || (s.id == ENCHANTED_BOOK
            && tag(s).is_some_and(|t| !list(t, "StoredEnchantments").is_empty()))
        || items::variant(s.id, s.damage).is_some_and(|v| v.glint)
}

/// `ItemStack.getRarity` color code.
fn rarity(s: &ItemStack) -> &'static str {
    let r = usize::from(items::variant(s.id, s.damage).map_or(0, |v| v.rarity)).min(3);
    let r = if enchanted(s) && r < 2 {
        2
    } else if enchanted(s) && r == 2 {
        3
    } else {
        r
    };
    RARITY[r]
}

/// The tooltip's first line: the name plus the id suffix of advanced tooltips.
fn head_line(s: &ItemStack, advanced: bool) -> String {
    let item = items::item(s.id);
    let (n, custom) = name(s);
    let mut head = if custom { format!("{ITALIC}{n}") } else { n };
    head.push_str(RESET);
    // `head` always holds RESET here, so vanilla's empty-name branch never runs.
    if advanced && item.subtypes {
        head.push_str(&format!(" (#{:04}/{})", s.id, s.damage));
    } else if advanced {
        head.push_str(&format!(" (#{:04})", s.id));
    } else if !custom && item.key == "filled_map" {
        head.push_str(&format!(" #{}", s.damage));
    }
    head
}

/// Lines from the stack's tag: enchantments, then the dye color and lore of `display`.
fn tag_lines(t: &Compound, hide: i32, advanced: bool, out: &mut Vec<String>) {
    if hide & 1 == 0 {
        out.extend(
            enchantments(t, "ench")
                .into_iter()
                .filter_map(|(id, l)| enchantment_name(id, l)),
        );
    }
    let Some(d) = compound(t, "display") else {
        return;
    };
    if matches!(get(d, "color"), Some(Tag::Int(_))) {
        out.push(if advanced {
            format!("Color: #{:X}", int(d, "color"))
        } else {
            format!("{ITALIC}Dyed")
        });
    }
    out.extend(list(d, "Lore").iter().filter_map(|l| match l {
        Tag::String(l) => Some(format!("{DARK_PURPLE}{ITALIC}{l}")),
        _ => None,
    }));
}

/// The `CanDestroy` and `CanPlaceOn` block lists of adventure mode.
fn block_lines(t: Option<&Compound>, hide: i32, out: &mut Vec<String>) {
    for (key, label, bit) in [
        ("CanDestroy", "Can break:", 8),
        ("CanPlaceOn", "Can be placed on:", 16),
    ] {
        let blocks = t.map_or(&[][..], |t| list(t, key));
        if blocks.is_empty() || hide & bit != 0 {
            continue;
        }
        out.push(String::new());
        out.push(format!("{GRAY}{label}"));
        // shortcut: block ids are shown raw (vanilla prints the block's local name); upgrade with a block name table.
        out.extend(blocks.iter().filter_map(|b| match b {
            Tag::String(b) => Some(format!("{GRAY}{b}")),
            _ => None,
        }));
    }
}

/// `ItemStack.getTooltip`, with the colors `renderToolTip` adds.
pub fn lines(s: &ItemStack, advanced: bool) -> Vec<String> {
    let item = items::item(s.id);
    let mut out = vec![head_line(s, advanced)];
    let t = tag(s);
    let hide = t.map_or(0, |t| int(t, "HideFlags"));
    let variant = items::variant(s.id, s.damage);
    if hide & 32 == 0 {
        out.extend(
            variant
                .iter()
                .flat_map(|v| v.info.iter())
                .map(|l| (*l).to_string()),
        );
        // ItemEnchantedBook.addInformation lists the stored enchantments.
        if let (true, Some(t)) = (s.id == ENCHANTED_BOOK, t) {
            out.extend(
                enchantments(t, "StoredEnchantments")
                    .into_iter()
                    .filter_map(|(id, l)| enchantment_name(id, l)),
            );
        }
    }
    if let Some(t) = t {
        tag_lines(t, hide, advanced, &mut out);
    }
    // shortcut: AttributeModifiers tags are not read; the item defaults are shown, as for untagged stacks.
    if let Some(v) = variant.filter(|v| !v.attributes.is_empty() && hide & 2 == 0) {
        out.push(String::new());
        out.extend(v.attributes.iter().map(|l| (*l).to_string()));
    }
    let unbreakable = t.is_some_and(|t| int(t, "Unbreakable") != 0);
    if unbreakable && hide & 4 == 0 {
        out.push(format!("{BLUE}Unbreakable"));
    }
    block_lines(t, hide, &mut out);
    if advanced && item.max_damage > 0 && !unbreakable && s.damage > 0 {
        out.push(format!(
            "Durability: {} / {}",
            item.max_damage - s.damage,
            item.max_damage
        ));
    }
    // GuiScreen.renderToolTip colors line 0 by rarity and the rest gray.
    for (i, l) in out.iter_mut().enumerate() {
        let color = if i == 0 { rarity(s) } else { GRAY };
        l.insert_str(0, color);
    }
    out
}

/// `GuiScreen.drawHoveringText`.
pub fn draw(c: &mut dyn Canvas, lines: &[String], x: i32, y: i32, width: i32, height: i32) {
    if lines.is_empty() {
        return;
    }
    let w = lines.iter().map(|l| c.width(l)).max().unwrap_or(0);
    let mut l = x + 12;
    let mut t = y - 12;
    let h = if lines.len() > 1 {
        8 + 2 + (lines.len() as i32 - 1) * 10
    } else {
        8
    };
    if l + w > width {
        l -= 28 + w;
    }
    if t + h + 6 > height {
        t = height - h - 6;
    }
    let bg = 0xF010_0010;
    c.gradient(l - 3, t - 4, l + w + 3, t - 3, bg, bg);
    c.gradient(l - 3, t + h + 3, l + w + 3, t + h + 4, bg, bg);
    c.gradient(l - 3, t - 3, l + w + 3, t + h + 3, bg, bg);
    c.gradient(l - 4, t - 3, l - 3, t + h + 3, bg, bg);
    c.gradient(l + w + 3, t - 3, l + w + 4, t + h + 3, bg, bg);
    let top = 0x5050_00FF_u32;
    let bottom = ((top & 0x00FE_FEFE) >> 1) | (top & 0xFF00_0000);
    c.gradient(l - 3, t - 3 + 1, l - 3 + 1, t + h + 3 - 1, top, bottom);
    c.gradient(l + w + 2, t - 3 + 1, l + w + 3, t + h + 3 - 1, top, bottom);
    c.gradient(l - 3, t - 3, l + w + 3, t - 3 + 1, top, top);
    c.gradient(l - 3, t + h + 2, l + w + 3, t + h + 3, bottom, bottom);
    for (i, line) in lines.iter().enumerate() {
        c.text(line, l as f32, t as f32, 0xFFFF_FFFF, true);
        t += if i == 0 { 12 } else { 10 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::inventory::{Nbt, Tag};

    fn stack(id: i16, damage: i16, nbt: Option<Compound>) -> ItemStack {
        ItemStack {
            id,
            count: 1,
            damage,
            nbt: nbt.map(|root| Nbt {
                name: String::new(),
                root,
            }),
        }
    }

    #[test]
    fn vanilla_lines() {
        assert_eq!(lines(&stack(1, 1, None), false), ["\u{a7}fGranite\u{a7}r"]);
        assert_eq!(
            lines(&stack(1, 1, None), true),
            ["\u{a7}fGranite\u{a7}r (#0001/1)"]
        );
        let sword = lines(&stack(276, 10, None), true);
        assert_eq!(sword.last().unwrap(), "\u{a7}7Durability: 1551 / 1561");
        assert!(sword.contains(&"\u{a7}7".to_string()));
        let ench = vec![(
            "ench".to_string(),
            Tag::List(vec![Tag::Compound(vec![
                ("id".into(), Tag::Short(16)),
                ("lvl".into(), Tag::Short(5)),
            ])]),
        )];
        let l = lines(&stack(276, 0, Some(ench)), false);
        assert_eq!(l[0], "\u{a7}bDiamond Sword\u{a7}r");
        assert_eq!(l[1], "\u{a7}7Sharpness V");
    }
}
