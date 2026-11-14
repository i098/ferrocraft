//! The blocks and items lists of `GuiStats` (`StatsBlock`, `StatsItem` and their base class): three
//! sortable columns of counters next to an item icon.

use std::cmp::Ordering;

use world::inventory::ItemStack;

use super::stat_format::Format;
use super::stats_table::{self as table, item};
use super::{text, Canvas, Ctx, Effect, PlayerStats, ScrollList, WHITE};

pub const STATS_ICONS: &str = "textures/gui/container/stats_icons.png";
const GRAY: u32 = 0x90_90_90;
/// Where the three columns end, from the left edge of a row.
const COLUMN_X: [i32; 3] = [115, 165, 215];
/// Where the three header buttons answer a click, from the left edge of the list.
const CLICK_X: [(i32, i32); 3] = [(79, 115), (129, 165), (179, 215)];

/// One counter column: its header icon and language key, and the `StatList` array that maps an
/// item id to its stat.
pub struct Column {
    icon: i32,
    header: &'static str,
    array: &'static [(u16, u16)],
    prefix: &'static str,
}

/// `StatsBlock`: crafted, used, mined. The mined column is the list's own stat.
pub const BLOCKS: [Column; 3] = [
    Column {
        icon: 18,
        header: "stat.crafted",
        array: table::ITEMS_CRAFTED,
        prefix: "stat.craftItem.",
    },
    Column {
        icon: 36,
        header: "stat.used",
        array: table::ITEMS_USED,
        prefix: "stat.useItem.",
    },
    Column {
        icon: 54,
        header: "stat.mined",
        array: table::BLOCKS_MINED,
        prefix: "stat.mineBlock.",
    },
];

/// `StatsItem`: depleted, crafted, used. The used column is the list's own stat.
pub const ITEMS: [Column; 3] = [
    Column {
        icon: 72,
        header: "stat.depleted",
        array: table::ITEMS_BROKEN,
        prefix: "stat.breakItem.",
    },
    Column {
        icon: 18,
        header: "stat.crafted",
        array: table::ITEMS_CRAFTED,
        prefix: "stat.craftItem.",
    },
    Column {
        icon: 36,
        header: "stat.used",
        array: table::ITEMS_USED,
        prefix: "stat.useItem.",
    },
];

/// The server's stat name of an item with a stat of kind `prefix`.
fn key(prefix: &str, id: u16) -> Option<String> {
    item(id).map(|(_, name, _)| format!("{prefix}{name}"))
}

/// `StatList` array lookup: the value of the stat in slot `id`, when the slot has one.
fn slot(stats: &PlayerStats, col: &Column, id: u16) -> Option<i32> {
    let at = col.array.binary_search_by_key(&id, |p| p.0).ok()?;
    key(col.prefix, col.array[at].1).map(|k| stats.read(&k))
}

/// The list's own stat of an item, in the last column.
fn own(stats: &PlayerStats, cols: &[Column; 3], id: u16) -> i32 {
    key(cols[2].prefix, id).map_or(0, |k| stats.read(&k))
}

/// `Comparator<ItemStat>` of the lists: the sorted column, then the item id. A missing stat sorts last.
fn compare(stats: &PlayerStats, sorted: Option<&Column>, order: i32, a: u16, b: u16) -> Ordering {
    let get = |id| sorted.and_then(|c| slot(stats, c, id));
    match (get(a), get(b)) {
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        // The real difference is an `int` that wraps; `i64` keeps the order total.
        (Some(k), Some(l)) if k != l => ((i64::from(k) - i64::from(l)) * i64::from(order)).cmp(&0),
        _ => a.cmp(&b),
    }
}

/// `GuiStats.drawSprite`: an 18x18 piece of the 128x128 `stats_icons.png`.
fn sprite(c: &mut dyn Canvas, x: i32, y: i32, u: i32, v: i32) {
    let uv = [u, v, u + 18, v + 18].map(|n| n as f32 / 128.0);
    c.blit(STATS_ICONS, x as f32, y as f32, 18.0, 18.0, uv, 0xffff_ffff);
}

/// `GuiStats.drawItemSprite`: the slot frame and the item in it.
fn item_icon(c: &mut dyn Canvas, x: i32, y: i32, id: u16, ctx: &Ctx) {
    sprite(c, x + 1, y + 1, 0, 0);
    if let Some(icons) = ctx.icons {
        let stack = ItemStack {
            id: id as i16,
            count: 1,
            damage: 0,
            nbt: None,
        };
        icons.draw(c, &stack, x + 2, y + 2);
    }
}

/// `drawGradientRect` box and white text, the hover text of the lists.
fn hover_text(c: &mut dyn Canvas, s: &str, mx: i32, my: i32) {
    let (x, y) = (mx + 12, my - 12);
    let w = c.width(s);
    c.gradient(x - 3, y - 3, x + w + 3, y + 8 + 3, 0xc000_0000, 0xc000_0000);
    text(c, s, x, y, WHITE);
}

/// `StatsBlock` or `StatsItem` without its `GuiSlot`.
pub struct Table {
    cols: &'static [Column; 3],
    /// `entries`: item ids, in the order of the list.
    entries: Vec<u16>,
    /// `field_148217_o`: the sorted column, -1 for none.
    selected: i32,
    /// `field_148215_p`: -1 up, 1 down.
    order: i32,
    /// `field_148218_l`: the header button held down, -1 for none.
    clicked: i32,
}

impl Table {
    /// The items of `source` that have a counter: `StatsBlock` and `StatsItem` constructors.
    pub fn new(cols: &'static [Column; 3], source: &[u16], stats: &PlayerStats) -> Table {
        let wanted = |id: u16| {
            own(stats, cols, id) > 0
                || cols[..2]
                    .iter()
                    .any(|c| slot(stats, c, id).is_some_and(|v| v > 0))
        };
        Table {
            cols,
            entries: source.iter().copied().filter(|&id| wanted(id)).collect(),
            selected: -1,
            order: 0,
            clicked: -1,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `click(column)`: the next sort state, then the sort.
    fn sort_by_column(&mut self, col: i32, stats: &PlayerStats) {
        if col != self.selected {
            (self.selected, self.order) = (col, -1);
        } else if self.order == -1 {
            self.order = 1;
        } else {
            (self.selected, self.order) = (-1, 0);
        }
        let sorted = usize::try_from(self.selected).ok().map(|c| &self.cols[c]);
        let order = self.order;
        self.entries
            .sort_by(|&a, &b| compare(stats, sorted, order, a, b));
    }

    /// `headerClicked`: `x` is measured from the left edge of the list's rows.
    pub fn header_clicked(&mut self, x: i32, ctx: &mut Ctx) {
        self.clicked = CLICK_X
            .iter()
            .position(|&(lo, hi)| (lo..hi).contains(&x))
            .map_or(-1, |c| c as i32);
        if self.clicked >= 0 {
            self.sort_by_column(self.clicked, ctx.stats);
            ctx.effects.push(Effect::Sound("gui.button.press"));
        }
    }

    /// `renderHeader`: the column buttons with their icons and the sort arrow.
    pub fn draw_header(&mut self, c: &mut dyn Canvas, x: i32, y: i32, left_down: bool) {
        if !left_down {
            self.clicked = -1;
        }
        for (i, off) in COLUMN_X.into_iter().enumerate() {
            let v = if self.clicked == i as i32 { 0 } else { 18 };
            sprite(c, x + off - 18, y + 1, 0, v);
        }
        if let Ok(col) = usize::try_from(self.selected) {
            let u = if self.order == 1 { 36 } else { 18 };
            sprite(c, x + [79, 129, 179][col], y + 1, u, 0);
        }
        for (i, (off, col)) in COLUMN_X.into_iter().zip(self.cols).enumerate() {
            if self.clicked == i as i32 {
                sprite(c, x + off - 18 + 1, y + 2, col.icon, 18);
            } else {
                sprite(c, x + off - 18, y + 1, col.icon, 18);
            }
        }
    }

    /// `renderEntry`: the item icon and its three counters.
    pub fn draw_row(&self, c: &mut dyn Canvas, i: usize, x: i32, y: i32, ctx: &Ctx) {
        let id = self.entries[i];
        item_icon(c, x + 40, y, id, ctx);
        for (n, off) in COLUMN_X.into_iter().enumerate() {
            let value = if n == 2 {
                Some(own(ctx.stats, self.cols, id))
            } else {
                slot(ctx.stats, &self.cols[n], id)
            };
            draw_stat(c, value, x + off, y, i.is_multiple_of(2));
        }
    }

    /// `renderDecorations`: the item name over an icon, the column name over a header button.
    pub fn draw_hover(&self, c: &mut dyn Canvas, list: &ScrollList, mx: i32, my: i32, ctx: &Ctx) {
        if my < list.top || my > list.bottom {
            return;
        }
        let j = list.width / 2 - 92 - 16;
        let label = match list.row_at(mx, my, self.entries.len()) {
            Some(i) if (j + 40..=j + 60).contains(&mx) => {
                let Some((_, _, unlocalized)) = item(self.entries[i]) else {
                    return;
                };
                ctx.tr(&format!("{unlocalized}.name"))
            }
            Some(_) => return,
            None => {
                let col = COLUMN_X
                    .iter()
                    .position(|&off| (j + off - 18..=j + off).contains(&mx));
                let Some(col) = col else { return };
                ctx.tr(self.cols[col].header)
            }
        };
        let label = label.trim();
        if !label.is_empty() {
            hover_text(c, label, mx, my);
        }
    }
}

/// `renderStat`: a right-aligned counter, `-` when the item has no such stat.
fn draw_stat(c: &mut dyn Canvas, value: Option<i32>, x: i32, y: i32, even: bool) {
    let s = value.map_or_else(|| "-".to_owned(), |v| Format::Number.format(v));
    let w = c.width(&s);
    text(c, &s, x - w, y + 5, if even { WHITE } else { GRAY });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(values: &[(&str, i32)]) -> PlayerStats {
        let mut s = PlayerStats::default();
        for (k, v) in values {
            s.set(k, *v);
        }
        s
    }

    /// Stone is id 1, dirt 3, planks 5 (only a crafted counter). The first click sorts the mined column down, the second
    /// up, the third restores the id order.
    #[test]
    fn header_clicks_cycle_the_sort() {
        let s = stats(&[
            ("stat.mineBlock.minecraft.stone", 5),
            ("stat.mineBlock.minecraft.dirt", 9),
            ("stat.craftItem.minecraft.planks", 1),
        ]);
        let mut t = Table::new(&BLOCKS, &[1, 3, 5], &s);
        assert_eq!(t.entries, [1, 3, 5]);
        t.sort_by_column(2, &s);
        assert_eq!(t.entries, [3, 1, 5]);
        t.sort_by_column(2, &s);
        assert_eq!(t.entries, [5, 1, 3]);
        t.sort_by_column(2, &s);
        assert_eq!((t.selected, t.order), (-1, 0));
        assert_eq!(t.entries, [1, 3, 5]);
    }
}
