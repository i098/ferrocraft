//! Break speed of the local player: `PlayerEntity.getMiningSpeed` and `Block.getMiningSpeed` of
//! 1.8.9 with the held tool, Efficiency, Haste, Mining Fatigue and water. Checked against the
//! real client for every block through `oracle` (`minewith`).

use crate::dig::{hardness, highest_enchant_level, requires_tool};
use crate::effects::Effects;
use crate::inventory::Slot;

const EFFICIENCY: i32 = 32;
const AQUA_AFFINITY: i32 = 6;
/// `Potion.digSpeed` (Haste) and `Potion.digSlowdown` (Mining Fatigue).
const HASTE: i8 = 3;
const MINING_FATIGUE: i8 = 4;

/// What changes the break speed besides the block: the held item, its Efficiency level, the
/// effect amplifiers and whether the eye is under water without Aqua Affinity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mining {
    /// Item id in the selected slot.
    pub item: Option<i16>,
    pub efficiency: i32,
    pub haste: Option<i8>,
    pub fatigue: Option<i8>,
    /// `isSubmergedIn(WATER) && !getAquaAffinityModifier`.
    pub slowed_by_water: bool,
}

impl Mining {
    /// Reads the held stack, the worn armor (window 0 slots 5 to 8), the effects and the eye.
    pub fn of_player(held: &Slot, armor: &[Slot], effects: &Effects, eyes_in_water: bool) -> Self {
        Self {
            item: held.as_ref().map(|s| s.id),
            efficiency: highest_enchant_level(std::iter::once(held), EFFICIENCY),
            haste: effects.get(HASTE).map(|e| e.amplifier),
            fatigue: effects.get(MINING_FATIGUE).map(|e| e.amplifier),
            slowed_by_water: eyes_in_water && highest_enchant_level(armor, AQUA_AFFINITY) <= 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Pickaxe,
    Shovel,
    Axe,
    Sword,
    Shears,
}

/// Tool kind and tier (wood 0, stone 1, iron 2, diamond 3, gold 0) of an item id.
fn tool(item: i16) -> Option<(Kind, u8)> {
    Some(match item {
        270 => (Kind::Pickaxe, 0),
        274 => (Kind::Pickaxe, 1),
        257 => (Kind::Pickaxe, 2),
        278 => (Kind::Pickaxe, 3),
        285 => (Kind::Pickaxe, 0),
        269 => (Kind::Shovel, 0),
        273 => (Kind::Shovel, 1),
        256 => (Kind::Shovel, 2),
        277 => (Kind::Shovel, 3),
        284 => (Kind::Shovel, 0),
        271 => (Kind::Axe, 0),
        275 => (Kind::Axe, 1),
        258 => (Kind::Axe, 2),
        279 => (Kind::Axe, 3),
        286 => (Kind::Axe, 0),
        268 => (Kind::Sword, 0),
        272 => (Kind::Sword, 1),
        267 => (Kind::Sword, 2),
        276 => (Kind::Sword, 3),
        283 => (Kind::Sword, 0),
        359 => (Kind::Shears, 0),
        _ => return None,
    })
}

/// `ToolMaterial.efficiencyOnProperMaterial` by tier; gold (tier 0) is the exception.
fn tier_speed(item: i16, tier: u8) -> f32 {
    if matches!(item, 283..=286) {
        return 12.0;
    }
    [2.0, 4.0, 6.0, 8.0][usize::from(tier)]
}

/// `Item.getMiningSpeed(stack, block)`.
fn tool_speed(item: Option<i16>, id: u16) -> f32 {
    let Some(item) = item else { return 1.0 };
    let Some((kind, tier)) = tool(item) else {
        return 1.0;
    };
    let fast = tier_speed(item, tier);
    match kind {
        Kind::Pickaxe if pickaxe_digs(id) => fast,
        Kind::Shovel if matches!(id, 2 | 3 | 12 | 13 | 60 | 78 | 80 | 82 | 88 | 110) => fast,
        Kind::Axe if axe_digs(id) => fast,
        Kind::Sword if id == 30 => 15.0,
        Kind::Sword if matches!(id, 18 | 86 | 91 | 103 | 106 | 127 | 161) => 1.5,
        Kind::Shears if matches!(id, 18 | 30 | 161) => 15.0,
        Kind::Shears if id == 35 => 5.0,
        _ => 1.0,
    }
}

/// Rock, iron and anvil blocks (every `requires_tool` block but web and snow), rails, ice.
fn pickaxe_digs(id: u16) -> bool {
    (requires_tool(id) && !matches!(id, 30 | 78 | 80))
        || matches!(id, 27 | 28 | 66 | 79 | 157 | 174)
}

/// Wood, plant and vine blocks the axe is made for.
fn axe_digs(id: u16) -> bool {
    matches!(
        id,
        5 | 17 | 25 | 47 | 53 | 54 | 58 | 63 | 64 | 65 | 68 | 72 | 84..=86 | 91 | 96 | 99 | 100
            | 103 | 106 | 107 | 125..=127 | 134..=136 | 146 | 151 | 162..=164 | 176..=178
            | 183..=197
    )
}

/// `ItemStack.canHarvestBlock` for the blocks that `requires_tool`.
fn can_harvest(item: Option<i16>, id: u16) -> bool {
    let Some((kind, tier)) = item.and_then(tool) else {
        return false;
    };
    match kind {
        Kind::Pickaxe => match id {
            49 => tier == 3,
            14 | 41 | 56 | 57 | 73 | 74 | 129 | 133 => tier >= 2,
            15 | 21 | 22 | 42 => tier >= 1,
            _ => pickaxe_digs(id),
        },
        Kind::Shovel => matches!(id, 78 | 80),
        Kind::Sword | Kind::Shears => id == 30,
        Kind::Axe => false,
    }
}

/// `PlayerEntity.getMiningSpeed`.
fn player_speed(id: u16, on_ground: bool, m: &Mining) -> f32 {
    let mut speed = tool_speed(m.item, id);
    if speed > 1.0 && m.efficiency > 0 && m.item.is_some() {
        speed += (m.efficiency * m.efficiency + 1) as f32;
    }
    if let Some(amp) = m.haste {
        speed *= 1.0 + (f32::from(amp) + 1.0) * 0.2;
    }
    if let Some(amp) = m.fatigue {
        speed *= match amp {
            0 => 0.3,
            1 => 0.09,
            2 => 0.0027,
            _ => 8.1E-4,
        };
    }
    if m.slowed_by_water {
        speed /= 5.0;
    }
    if !on_ground {
        speed /= 5.0;
    }
    speed
}

/// `Block.getPlayerRelativeBlockHardness`: the share of the block that one tick of digging
/// removes. At 1.0 or more the block breaks at once.
pub fn relative_hardness(id: u16, on_ground: bool, m: &Mining) -> f32 {
    let h = hardness(id);
    if h < 0.0 {
        return 0.0;
    }
    let harvest = !requires_tool(id) || can_harvest(m.item, id);
    player_speed(id, on_ground, m) / h / if harvest { 30.0 } else { 100.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::Effect;
    use crate::inventory::{ItemStack, Nbt, Tag};

    #[derive(Clone, Copy)]
    struct Case {
        item: i16,
        efficiency: i16,
        haste: i8,
        fatigue: i8,
        aqua: bool,
    }

    const NONE: Case = Case {
        item: 0,
        efficiency: 0,
        haste: -1,
        fatigue: -1,
        aqua: false,
    };

    fn enchanted(id: i16, item: i16, lvl: i16) -> Slot {
        let ench = Tag::Compound(vec![
            ("id".into(), Tag::Short(id)),
            ("lvl".into(), Tag::Short(lvl)),
        ]);
        Some(ItemStack {
            id: item,
            count: 1,
            damage: 0,
            nbt: Some(Nbt {
                name: String::new(),
                root: vec![("ench".into(), Tag::List(vec![ench]))],
            }),
        })
    }

    /// The `Mining` of a case, read the way the game reads the player.
    fn mining(c: Case, in_water: bool) -> Mining {
        let held = if c.item != 0 {
            enchanted(EFFICIENCY as i16, c.item, c.efficiency)
        } else {
            None
        };
        let helmet = if c.aqua {
            enchanted(AQUA_AFFINITY as i16, 298, 1)
        } else {
            None
        };
        let mut effects = Effects::default();
        for (id, amplifier) in [(HASTE, c.haste), (MINING_FATIGUE, c.fatigue)] {
            if amplifier >= 0 {
                effects.add(Effect {
                    id,
                    amplifier,
                    duration: 600,
                    permanent: false,
                    ambient: false,
                    particles: true,
                });
            }
        }
        Mining::of_player(&held, &[None, None, None, helmet], &effects, in_water)
    }

    /// Every block, with every tool kind and tier, Efficiency, Haste, Mining Fatigue and water,
    /// against the real client.
    #[test]
    fn mining_speed_matches_the_real_client() {
        let pick = |item| Case { item, ..NONE };
        let mut dry = vec![NONE];
        // wood, stone, iron, diamond and gold of pickaxe, shovel, axe, sword; shears; a hoe; a block
        dry.extend(
            [
                270, 274, 257, 278, 285, 269, 273, 256, 277, 284, 271, 275, 258, 279, 286,
            ]
            .into_iter()
            .chain([268, 272, 267, 276, 283, 359, 292, 1, 296])
            .map(pick),
        );
        for item in [257, 256, 258, 359, 1] {
            for efficiency in [1, 3, 5] {
                dry.push(Case {
                    efficiency,
                    ..pick(item)
                });
            }
        }
        for amp in [0, 1, 2, 3, 4] {
            dry.push(Case {
                haste: amp,
                ..pick(257)
            });
            dry.push(Case {
                fatigue: amp,
                ..pick(257)
            });
            dry.push(Case {
                fatigue: amp,
                ..NONE
            });
        }
        dry.push(Case {
            efficiency: 5,
            haste: 1,
            fatigue: 0,
            ..pick(278)
        });
        dry.push(Case {
            aqua: true,
            ..pick(257)
        });
        let cases = |water: bool| -> Vec<(Case, bool)> {
            let list = if water {
                vec![
                    pick(257),
                    Case {
                        aqua: true,
                        ..pick(257)
                    },
                    NONE,
                ]
            } else {
                dry.clone()
            };
            list.into_iter().map(|c| (c, water)).collect()
        };
        for water in [false, true] {
            // A water block at the eye of the player standing at 0 64 0.
            let mut script = String::from(if water { "block 0 65 0 water\n" } else { "" });
            script += "start 0 64 0 0\n";
            let all = cases(water);
            for (c, _) in &all {
                for id in 0..198 {
                    script += &format!(
                        "minewith {id} {} {} {} {} {}\n",
                        c.item,
                        c.efficiency,
                        c.haste,
                        c.fatigue,
                        u8::from(c.aqua)
                    );
                }
            }
            let lines = oracle::Oracle::get().run(&script).unwrap();
            assert_eq!(lines.len(), all.len() * 198);
            let mut wrong = vec![];
            for (n, line) in lines.iter().enumerate() {
                let (c, w) = all[n / 198];
                let id = (n % 198) as u16;
                let bits: Vec<u32> = line
                    .split_whitespace()
                    .map(|s| u32::from_str_radix(s, 16).unwrap())
                    .collect();
                let m = mining(c, w);
                let got = [
                    relative_hardness(id, true, &m).to_bits(),
                    relative_hardness(id, false, &m).to_bits(),
                ];
                if got != bits[..] {
                    wrong.push(format!(
                        "block {id} item {} eff {} haste {} fatigue {} aqua {} water {w}: got {:?} want {:?}",
                        c.item,
                        c.efficiency,
                        c.haste,
                        c.fatigue,
                        c.aqua,
                        got.map(f32::from_bits),
                        bits.iter().map(|b| f32::from_bits(*b)).collect::<Vec<_>>()
                    ));
                }
            }
            assert!(
                wrong.is_empty(),
                "{} wrong:\n{}",
                wrong.len(),
                wrong[..wrong.len().min(30)].join("\n")
            );
        }
    }

    /// An iron pickaxe on end stone (hardness 3): 6 / 3 / 30 per tick, 15 ticks with the press.
    #[test]
    fn iron_pickaxe_breaks_end_stone_in_fifteen_ticks() {
        let m = mining(Case { item: 257, ..NONE }, false);
        let step = relative_hardness(121, true, &m);
        let (mut progress, mut ticks) = (0.0_f32, 0);
        while progress < 1.0 {
            progress += step;
            ticks += 1;
        }
        assert_eq!(ticks, 15);
    }
}
