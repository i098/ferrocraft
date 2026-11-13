//! Window sizes and slot positions of the vanilla 1.8.9 container screens (`Container*` slot
//! constructors and `Gui*` `xSize`/`ySize`), in GUI units from the window's top-left corner.

use world::inventory::Kind;

/// Gray of every container label (`4210752`).
pub const LABEL: u32 = 0xFF40_4040;

pub struct Layout {
    pub texture: &'static str,
    pub width: i32,
    pub height: i32,
    /// Window slot positions, in window slot order.
    pub slots: Vec<(i32, i32)>,
}

/// The 27 main slots then the 9 hotbar slots, with the main grid's left at `x` and top at `y`.
fn player_part(slots: &mut Vec<(i32, i32)>, x: i32, y: i32, hotbar_y: i32) {
    for row in 0..3 {
        for col in 0..9 {
            slots.push((x + col * 18, y + row * 18));
        }
    }
    for col in 0..9 {
        slots.push((x + col * 18, hotbar_y));
    }
}

fn grid(slots: &mut Vec<(i32, i32)>, x: i32, y: i32, cols: i32, rows: i32) {
    for row in 0..rows {
        for col in 0..cols {
            slots.push((x + col * 18, y + row * 18));
        }
    }
}

pub fn layout(kind: Kind) -> Layout {
    let mut slots = Vec::new();
    let (texture, height) = match kind {
        Kind::Player => {
            slots.push((144, 36));
            grid(&mut slots, 88, 26, 2, 2);
            grid(&mut slots, 8, 8, 1, 4);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/inventory.png", 166)
        }
        Kind::Chest(rows) => {
            let rows = i32::from(rows);
            grid(&mut slots, 8, 18, 9, rows);
            let shift = (rows - 4) * 18;
            player_part(&mut slots, 8, 103 + shift, 161 + shift);
            ("textures/gui/container/generic_54.png", 114 + rows * 18)
        }
        Kind::Crafting => {
            slots.push((124, 35));
            grid(&mut slots, 30, 17, 3, 3);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/crafting_table.png", 166)
        }
        Kind::Furnace => {
            slots.extend([(56, 17), (56, 53), (116, 35)]);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/furnace.png", 166)
        }
        Kind::Dispenser => {
            grid(&mut slots, 62, 17, 3, 3);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/dispenser.png", 166)
        }
        Kind::Hopper => {
            grid(&mut slots, 44, 20, 5, 1);
            player_part(&mut slots, 8, 51, 109);
            ("textures/gui/container/hopper.png", 133)
        }
        Kind::Anvil => {
            slots.extend([(27, 47), (76, 47), (134, 47)]);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/anvil.png", 166)
        }
        Kind::Enchanting => {
            slots.extend([(15, 47), (35, 47)]);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/enchanting_table.png", 166)
        }
        Kind::Brewing => {
            slots.extend([(56, 46), (79, 53), (102, 46), (79, 17)]);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/brewing_stand.png", 166)
        }
        Kind::Beacon => {
            slots.push((136, 110));
            player_part(&mut slots, 36, 137, 195);
            ("textures/gui/container/beacon.png", 219)
        }
        Kind::Merchant => {
            slots.extend([(36, 53), (62, 53), (120, 53)]);
            player_part(&mut slots, 8, 84, 142);
            ("textures/gui/container/villager.png", 166)
        }
    };
    let width = if kind == Kind::Beacon { 230 } else { 176 };
    Layout {
        texture,
        width,
        height,
        slots,
    }
}

/// Icon of an empty armor slot (`ItemArmor.EMPTY_SLOT_NAMES`), by player window slot.
pub fn empty_slot_icon(kind: Kind, slot: usize) -> Option<&'static str> {
    if kind != Kind::Player {
        return None;
    }
    Some(match slot {
        5 => "textures/items/empty_armor_slot_helmet.png",
        6 => "textures/items/empty_armor_slot_chestplate.png",
        7 => "textures/items/empty_armor_slot_leggings.png",
        8 => "textures/items/empty_armor_slot_boots.png",
        _ => return None,
    })
}
