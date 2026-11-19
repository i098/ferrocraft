//! `CraftingManager.findMatchingRecipe` of 1.8.9 for the shaped and shapeless recipes.
//!
//! `recipes.tsv` is `CraftingManager.getRecipes()` in match order, printed by `Dumper.java`.
//! shortcut: the special recipes (armor dye, banners, fireworks, repair, map and book copies) are
//! not ported; their result slot stays empty until the server sends it.

use super::{ItemStack, Slot};
use std::sync::LazyLock;

/// `OreDictionary`-free wildcard of `new ItemStack(item, 1, 32767)`.
const ANY: i16 = 32767;

/// An ingredient `(id, meta)`; `None` is an empty cell.
type Ing = Option<(i16, i16)>;

struct Recipe {
    /// `Some((width, height))` for a shaped recipe.
    shape: Option<(usize, usize)>,
    ingredients: Vec<Ing>,
    result: ItemStack,
}

static RECIPES: LazyLock<Vec<Recipe>> = LazyLock::new(|| {
    include_str!("recipes.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let num = |s: &str| s.parse().expect("recipe table number");
            let mut r = c[3].split(':').map(num);
            let (id, damage, count) = (r.next().unwrap(), r.next().unwrap(), r.next().unwrap());
            Recipe {
                shape: (c[0] == "shaped").then(|| (num(c[1]) as usize, num(c[2]) as usize)),
                ingredients: c[4]
                    .split(' ')
                    .map(|i| {
                        let (id, meta) = i.split_once(':')?;
                        Some((id.parse().ok()?, meta.parse().ok()?))
                    })
                    .collect(),
                result: ItemStack {
                    id,
                    count: count as i8,
                    damage,
                    nbt: None,
                },
            }
        })
        .collect()
});

/// `ShapedRecipe.checkMatch` / `ShapelessRecipes` ingredient test: same item, and the same damage
/// unless the ingredient is the wildcard.
fn accepts(ing: (i16, i16), s: &ItemStack) -> bool {
    ing.0 == s.id && (ing.1 == ANY || ing.1 == s.damage)
}

fn shaped(r: &Recipe, (w, h): (usize, usize), grid: &[Slot], side: usize) -> bool {
    // vanilla always walks 3x3 cells; `InventoryCrafting.getStackInRowAndColumn` is null off the grid
    let cell = |c: usize, row: usize| {
        (c < side && row < side)
            .then(|| grid.get(c + row * side))
            .flatten()
            .and_then(|s| s.as_ref())
    };
    let at = |x: isize, y: isize, mirror: bool| -> Ing {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= w || y >= h {
            return None;
        }
        r.ingredients[if mirror { w - x - 1 } else { x } + y * w]
    };
    (0..=3 - w).any(|ox| {
        (0..=3 - h).any(|oy| {
            [true, false].into_iter().any(|mirror| {
                (0..3).all(|c| {
                    (0..3).all(|row| {
                        match (
                            cell(c, row),
                            at(c as isize - ox as isize, row as isize - oy as isize, mirror),
                        ) {
                            (None, None) => true,
                            (Some(s), Some(i)) => accepts(i, s),
                            _ => false,
                        }
                    })
                })
            })
        })
    })
}

fn shapeless(r: &Recipe, grid: &[Slot]) -> bool {
    let mut left: Vec<Ing> = r.ingredients.clone();
    for s in grid.iter().flatten() {
        let Some(at) = left.iter().position(|i| i.is_some_and(|i| accepts(i, s))) else {
            return false;
        };
        left.remove(at);
    }
    left.is_empty()
}

/// The result of the first recipe the `side` x `side` grid matches (row by row), if any.
/// `ContainerPlayer.onCraftMatrixChanged` shows it in the result slot.
pub fn find(grid: &[Slot], side: usize) -> Slot {
    RECIPES
        .iter()
        .find(|r| match r.shape {
            Some(wh) => shaped(r, wh, grid, side),
            None => shapeless(r, grid),
        })
        .map(|r| r.result.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: i16, damage: i16) -> Slot {
        Some(ItemStack {
            id,
            count: 1,
            damage,
            nbt: None,
        })
    }

    #[test]
    fn four_planks_make_a_crafting_table_in_either_grid() {
        let planks = s(5, 0);
        assert_eq!(
            find(
                &[
                    planks.clone(),
                    planks.clone(),
                    planks.clone(),
                    planks.clone()
                ],
                2
            )
            .map(|r| (r.id, r.count)),
            Some((58, 1))
        );
        let mut big = vec![None; 9];
        big[4] = planks.clone();
        big[5] = planks.clone();
        big[7] = planks.clone();
        big[8] = planks;
        assert_eq!(find(&big, 3).map(|r| r.id), Some(58));
    }

    #[test]
    fn shaped_recipes_mirror_and_shapeless_ignore_position() {
        // sticks need two planks stacked, in either column of the 2x2 grid
        let p = s(5, 0);
        assert_eq!(
            find(&[None, p.clone(), None, p.clone()], 2).map(|r| (r.id, r.count)),
            Some((280, 4))
        );
        // flint and steel is shapeless: iron ingot (265) + flint (318)
        assert_eq!(
            find(&[None, s(318, 0), s(265, 0), None], 2).map(|r| r.id),
            Some(259)
        );
        // one plank is a wooden button (143); two planks side by side a pressure plate (72)
        assert_eq!(
            find(&[p.clone(), None, None, None], 2).map(|r| r.id),
            Some(143)
        );
        assert_eq!(find(&[None, None, p.clone(), p], 2).map(|r| r.id), Some(72));
        assert_eq!(find(&[s(1, 0), s(5, 0), None, None], 2), None);
    }
}
