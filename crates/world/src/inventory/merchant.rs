//! `MC|TrList` (S3F) and `MC|TrSel` (C17): the trades of the villager window, from
//! `MerchantRecipeList.readRecipiesFromPacket`.

use super::{ItemStack, Slot};
use ferrocraft_protocol::codec::{Reader, Result, Wire};

/// `MerchantRecipe`: what the player gives and gets, and how often the trade ran.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub buy: Slot,
    pub buy2: Slot,
    pub sell: Slot,
    pub uses: i32,
    pub max_uses: i32,
}

impl Recipe {
    /// `MerchantRecipe.isRecipeDisabled`.
    pub fn disabled(&self) -> bool {
        self.uses >= self.max_uses
    }
}

/// `MerchantRecipe.getItemToBuy` and `getSecondItemToBuy` against the offered stacks
/// (`MerchantRecipeList.canRecipeBeUsed`): same items, enough of each.
fn matches(r: &Recipe, a: &ItemStack, b: &Slot) -> bool {
    let Some(buy) = &r.buy else { return false };
    if a.id != buy.id || a.count < buy.count {
        return false;
    }
    match (&r.buy2, b) {
        (None, None) => true,
        (Some(want), Some(have)) => want.id == have.id && have.count >= want.count,
        _ => false,
    }
}

/// `MerchantRecipeList.canRecipeBeUsed`: the trade at `index` (index 0 searches all).
fn usable(recipes: &[Recipe], a: &ItemStack, b: &Slot, index: usize) -> Option<usize> {
    if index > 0 && index < recipes.len() {
        return matches(&recipes[index], a, b).then_some(index);
    }
    recipes.iter().position(|r| matches(r, a, b))
}

/// `InventoryMerchant.resetRecipeAndSlots`: the open trade for the two input slots, if any.
pub fn find(recipes: &[Recipe], index: usize, in0: &Slot, in1: &Slot) -> Option<usize> {
    let (a, b) = if in0.is_some() {
        (in0, in1)
    } else {
        (in1, &None)
    };
    let a = a.as_ref()?;
    let open = |i: Option<usize>| i.filter(|&i| !recipes[i].disabled());
    open(usable(recipes, a, b, index)).or_else(|| {
        let b = b.as_ref()?;
        open(usable(recipes, b, &Some(a.clone()), index))
    })
}

/// `SlotMerchantResult.doTrade`: take the price out of `a` and `b`; false when they do not pay it.
pub fn pay(r: &Recipe, a: &mut Slot, b: &mut Slot) -> bool {
    let (Some(sa), Some(buy)) = (a.as_mut(), &r.buy) else {
        return false;
    };
    if sa.id != buy.id {
        return false;
    }
    match (&r.buy2, b.as_mut()) {
        (Some(want), Some(sb)) if want.id == sb.id => {
            sa.count -= buy.count;
            sb.count -= want.count;
            true
        }
        (None, None) => {
            sa.count -= buy.count;
            true
        }
        _ => false,
    }
}

/// The window id and the recipes of a `MC|TrList` payload.
pub fn parse_list(data: &[u8]) -> Result<(i32, Vec<Recipe>)> {
    let mut r = Reader::new(data);
    let window = i32::read(&mut r)?;
    let count = u8::read(&mut r)?;
    let mut recipes = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let buy = Slot::read(&mut r)?;
        let sell = Slot::read(&mut r)?;
        let buy2 = if bool::read(&mut r)? {
            Slot::read(&mut r)?
        } else {
            None
        };
        let disabled = bool::read(&mut r)?;
        let uses = i32::read(&mut r)?;
        let max_uses = i32::read(&mut r)?;
        // `MerchantRecipe.compensateToolUses`
        let uses = if disabled { max_uses } else { uses };
        recipes.push(Recipe {
            buy,
            buy2,
            sell,
            uses,
            max_uses,
        });
    }
    Ok((window, recipes))
}

/// The `MC|TrSel` payload: the selected trade as a big-endian int.
pub fn select_payload(index: usize) -> Vec<u8> {
    (index as i32).to_be_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemStack;

    fn stack(id: i16, count: i8) -> Slot {
        Some(ItemStack {
            id,
            count,
            damage: 0,
            nbt: None,
        })
    }

    #[test]
    fn parses_trade_list_with_second_input_and_disabled_flag() {
        let mut b = 7i32.to_be_bytes().to_vec();
        b.push(2);
        // 24 paper for 1 emerald, no second input, 2 of 7 uses
        stack(339, 24).write(&mut b);
        stack(388, 1).write(&mut b);
        b.extend([0, 0]);
        b.extend(2i32.to_be_bytes());
        b.extend(7i32.to_be_bytes());
        // 1 emerald + 1 book for an enchanted book, flagged disabled at 3 of 12 uses
        stack(388, 1).write(&mut b);
        stack(403, 1).write(&mut b);
        b.push(1);
        stack(340, 1).write(&mut b);
        b.push(1);
        b.extend(3i32.to_be_bytes());
        b.extend(12i32.to_be_bytes());

        let (window, recipes) = parse_list(&b).unwrap();
        assert_eq!(window, 7);
        assert_eq!(recipes.len(), 2);
        assert_eq!(recipes[0].buy, stack(339, 24));
        assert_eq!(recipes[0].sell, stack(388, 1));
        assert_eq!(recipes[0].buy2, None);
        assert!(!recipes[0].disabled());
        assert_eq!(recipes[1].buy2, stack(340, 1));
        assert_eq!(recipes[1].uses, 12);
        assert!(recipes[1].disabled());
        assert!(parse_list(&b[..b.len() - 3]).is_err());
    }

    #[test]
    fn open_trade_needs_enough_of_each_input_and_pays_them() {
        let r = |uses| Recipe {
            buy: stack(339, 24),
            buy2: None,
            sell: stack(388, 1),
            uses,
            max_uses: 7,
        };
        let list = [r(0)];
        assert_eq!(find(&list, 0, &stack(339, 23), &None), None);
        assert_eq!(find(&list, 0, &stack(339, 30), &None), Some(0));
        assert_eq!(
            find(&list, 0, &None, &stack(339, 30)),
            Some(0),
            "inputs may sit in slot 1"
        );
        assert_eq!(
            find(&[r(7)], 0, &stack(339, 30), &None),
            None,
            "a used up trade is closed"
        );
        let (mut a, mut b) = (stack(339, 30), None);
        assert!(pay(&list[0], &mut a, &mut b));
        assert_eq!(a, stack(339, 6));
        assert!(!pay(&list[0], &mut stack(1, 5), &mut None));
    }

    #[test]
    fn select_payload_is_a_big_endian_int() {
        assert_eq!(select_payload(3), [0, 0, 0, 3]);
    }
}
