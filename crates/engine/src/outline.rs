//! `GameRenderer.shouldRenderBlockOutline` of 1.8.9 (javap offsets 17-44 hideGui, 51-57 `abilities.canModifyWorld`, 132-173 spectator
//! `Block.hasBlockEntity` + `Inventory`, 176-203 `ItemStack.hasMineBlockOverride` / `hasPlaceOnBlockOverride`).

use crate::game::Game;
use world::inventory::items;
use world::inventory::{ItemStack, Tag};

const ADVENTURE: u8 = 2;
const SPECTATOR: u8 = 3;
/// Blocks whose tile entity is an `IInventory`: dispenser, furnace (idle, lit), chest, brewing stand, beacon,
/// trapped chest, hopper, dropper.
const INVENTORY_BLOCKS: [u16; 8] = [23, 61, 62, 54, 117, 138, 146, 154];

/// `Block.getBlockFromName`: a registry name (`minecraft:` is the default namespace) or a numeric id.
fn block_named(name: &str) -> Option<u16> {
    let key = name.strip_prefix("minecraft:").unwrap_or(name);
    (0..256i16)
        .find(|&id| items::item(id).key == key)
        .map(|id| id as u16)
        .or_else(|| key.parse::<u16>().ok().filter(|&id| id < 256))
}

/// `ItemStack.canDestroy` / `canPlaceOn`: the tag list `key` of the stack names the block.
fn lists_block(stack: &ItemStack, key: &str, block: u16) -> bool {
    let root = stack.nbt.as_ref().map(|n| &n.root);
    let tags = root.and_then(|r| r.iter().find(|(k, _)| k == key));
    let Some((_, Tag::List(names))) = tags else {
        return false;
    };
    names
        .iter()
        .any(|t| matches!(t, Tag::String(n) if block_named(n) == Some(block)))
}

/// Whether the outline draws for the player view. `target` is the block state under the crosshair.
///
/// No outline with `hideGUI`. A player who may not edit (`allowEdit` is false in adventure and spectator) sees it only
/// in spectator on a block with an inventory, or in adventure with a held item that lists the block in `CanDestroy` or
/// `CanPlaceOn`.
pub fn draws(gamemode: u8, hide_hud: bool, held: Option<&ItemStack>, target: u16) -> bool {
    if hide_hud {
        return false;
    }
    let block = target >> 4;
    match gamemode {
        SPECTATOR => INVENTORY_BLOCKS.contains(&block),
        ADVENTURE => held.is_some_and(|s| {
            lists_block(s, "CanDestroy", block) || lists_block(s, "CanPlaceOn", block)
        }),
        _ => true,
    }
}

/// Eye, yaw, pitch, viewer id and vehicle of a view.
pub type Ray = ([f64; 3], f32, f32, i32, Option<i32>);

/// The outline box for the view `ray` (eye, yaw, pitch, viewer id, vehicle), or `None` when none draws.
pub fn selection(game: &Game, hide_hud: bool, ray: Option<Ray>) -> Option<[f64; 6]> {
    let (eye, yaw, pitch, id, vehicle) = ray?;
    let (hit, _) = game.block_aim(eye, yaw, pitch, id, vehicle)?;
    let [x, y, z] = hit.pos;
    let held = game.inv.held_stack().as_ref();
    if !draws(game.gamemode, hide_hud, held, game.store.get_block(x, y, z)) {
        return None;
    }
    game.selection(hit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::inventory::{Compound, Nbt};

    const STONE: u16 = 1 << 4;
    const CHEST: u16 = 54 << 4 | 2;

    fn stack(key: &str, names: &[&str]) -> ItemStack {
        let list = Tag::List(names.iter().map(|n| Tag::String((*n).into())).collect());
        ItemStack {
            id: 1,
            count: 1,
            damage: 0,
            nbt: Some(Nbt {
                name: String::new(),
                root: Compound::from(vec![(key.to_string(), list)]),
            }),
        }
    }

    #[test]
    fn survival_and_creative_draw() {
        assert!(draws(0, false, None, STONE));
        assert!(draws(1, false, None, STONE));
    }

    #[test]
    fn hidden_hud_never_draws() {
        for mode in 0..4 {
            assert!(!draws(mode, true, None, CHEST));
        }
    }

    #[test]
    fn adventure_needs_a_listed_block() {
        assert!(!draws(2, false, None, STONE));
        assert!(!draws(
            2,
            false,
            Some(&stack("CanDestroy", &["minecraft:dirt"])),
            STONE
        ));
        assert!(draws(
            2,
            false,
            Some(&stack("CanDestroy", &["minecraft:stone"])),
            STONE
        ));
        assert!(draws(
            2,
            false,
            Some(&stack("CanPlaceOn", &["stone"])),
            STONE
        ));
        assert!(draws(2, false, Some(&stack("CanPlaceOn", &["1"])), STONE));
    }

    #[test]
    fn spectator_draws_on_inventories_only() {
        assert!(!draws(3, false, None, STONE));
        assert!(draws(3, false, None, CHEST));
    }
}
