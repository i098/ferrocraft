//! `MobSpawnerBaseLogic.func_180613_a` calls `onInitialSpawn` on the entity it shows. The random
//! choices of the 1.8.9 `onInitialSpawn` methods that change how a mob looks are rolled here:
//! `EntityZombie` and `EntityPigZombie` (baby, chicken jockey, armor, weapon, enchantments),
//! `EntitySkeleton` (armor, bow), `EntitySlime` and `EntityMagmaCube` (size), `EntitySheep` (fleece
//! color), `EntityVillager` (profession) and `EntityRabbit` (type).
//!
//! The mob is never added to a world, so none of it ticks. Not drawn, as the client has no model for
//! it: the zombie villager of a 5% villager roll and the wither skeleton of a Nether skeleton.
//! The Halloween pumpkin helmet depends on the real date and is not ported. The skeleton of a
//! spider jockey and the kittens of an ocelot are added to the client world at the origin, where
//! the spawner never draws them.

use crate::jrandom::JavaRandom;

pub const ZOMBIE: u8 = 54;
pub const PIG_ZOMBIE: u8 = 57;
pub const SKELETON: u8 = 51;
pub const SLIME: u8 = 55;
pub const MAGMA_CUBE: u8 = 62;
pub const SHEEP: u8 = 91;
pub const RABBIT: u8 = 101;
pub const VILLAGER: u8 = 120;

const BOW: i16 = 261;
const GOLDEN_SWORD: i16 = 283;
const IRON_SWORD: i16 = 267;
const IRON_SHOVEL: i16 = 256;
/// Item id of the leather, golden, chainmail, iron and diamond helmet; `getEquipmentForSlot` takes
/// its tier as an index into this table. The chestplate, leggings and boots follow in that order.
const HELMETS: [i16; 5] = [298, 314, 302, 306, 310];

/// The inputs of `World.getLocalDifficulty` that the client knows. A client chunk never learns its
/// `inhabitedTime`, so it counts as 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Local {
    /// `EnumDifficulty` id: 0 peaceful, 1 easy, 2 normal, 3 hard.
    pub difficulty: u8,
    /// `World.getWorldTime`.
    pub world_time: i64,
}

impl Local {
    /// `DifficultyInstance.getClampedAdditionalDifficulty`.
    pub fn multiplier(self) -> f32 {
        let id = self.difficulty.min(3);
        if id == 0 {
            return 0.0;
        }
        let ramp = ((self.world_time as f32 - 72000.0) / 1_440_000.0).clamp(0.0, 1.0) * 0.25;
        // `World.getMoonSize`; `Dimension.getMoonPhase` counts days from 0.
        const MOON: [f32; 8] = [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75];
        let phase = ((self.world_time / 24000 % 8 + 8) % 8) as usize;
        let mut extra = (MOON[phase] * 0.25).clamp(0.0, ramp);
        if id == 1 {
            extra *= 0.5;
        }
        let difficulty = f32::from(id) * (0.75 + ramp + extra);
        if difficulty < 2.0 {
            0.0
        } else if difficulty > 4.0 {
            1.0
        } else {
            (difficulty - 2.0) / 2.0
        }
    }
}

/// How the rolls of `onInitialSpawn` change the look of a spawner mob.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Initial {
    /// `isChild`: a baby zombie or zombie pigman.
    pub child: bool,
    /// A baby zombie on its chicken jockey sits (`isRiding`).
    pub riding: bool,
    /// `EntitySlime.getSlimeSize`: 1, 2 or 4.
    pub slime_size: u8,
    /// Sheep: the fleece color, metadata 16 of `EntitySheep`.
    pub fleece: u8,
    /// Villager: the profession. Rabbit: the type.
    pub variant: u8,
    /// Item id in the main hand, 0 for none.
    pub held: i16,
    /// Armor item ids, boots first.
    pub armor: [i16; 4],
    pub held_glint: bool,
    pub armor_glint: [bool; 4],
}

impl Initial {
    /// Rolls `onInitialSpawn` of mob type `id`.
    pub fn roll(id: u8, local: Local, rng: &mut JavaRandom) -> Initial {
        let mut it = Initial {
            slime_size: 1,
            ..Initial::default()
        };
        let multiplier = local.multiplier();
        match id {
            ZOMBIE | PIG_ZOMBIE => {
                it.roll_baby(rng);
                if id == PIG_ZOMBIE {
                    // `EntityPigZombie.addRandomEquipment` does not call the zombie one.
                    it.held = GOLDEN_SWORD;
                } else {
                    it.roll_armor(local, multiplier, rng);
                    it.roll_sword(local, rng);
                }
                it.roll_enchantments(multiplier, rng);
            }
            SKELETON => {
                it.roll_armor(local, multiplier, rng);
                it.held = BOW;
                it.roll_enchantments(multiplier, rng);
            }
            SLIME | MAGMA_CUBE => {
                let mut exponent = rng.next_int(3);
                if exponent < 2 && rng.next_float() < 0.5 * multiplier {
                    exponent += 1;
                }
                it.slime_size = 1 << exponent;
            }
            SHEEP => it.fleece = fleece(rng),
            VILLAGER => it.variant = rng.next_int(5) as u8,
            RABBIT => it.variant = rng.next_int(6) as u8,
            _ => {}
        }
        it
    }

    /// `EntityZombie.onInitialSpawn`: the `GroupData` rolls and the chicken jockey. A baby that
    /// finds a chicken within 5 blocks of the origin rides it; the client has none there, so only
    /// a new chicken (second 5% roll) carries the baby.
    fn roll_baby(&mut self, rng: &mut JavaRandom) {
        rng.next_float(); // the villager roll; no zombie villager model
        self.child = rng.next_float() < 0.05;
        if self.child {
            let found = f64::from(rng.next_float()) < 0.05;
            self.riding = !found && f64::from(rng.next_float()) < 0.05;
        }
    }

    /// `EntityLiving.addRandomEquipment`.
    fn roll_armor(&mut self, local: Local, multiplier: f32, rng: &mut JavaRandom) {
        if rng.next_float() >= 0.15 * multiplier {
            return;
        }
        let mut tier = rng.next_int(2) as usize;
        let hard = local.difficulty == 3;
        let stop = if hard { 0.1 } else { 0.25 };
        for _ in 0..3 {
            if rng.next_float() < 0.095 {
                tier += 1;
            }
        }
        for piece in (0..4).rev() {
            if piece < 3 && rng.next_float() < stop {
                break;
            }
            // Helmet, chestplate, leggings and boots follow each other in the item table.
            self.armor[piece] = HELMETS[tier] + (3 - piece) as i16;
        }
    }

    /// `EntityZombie.addRandomEquipment`: a rare iron sword or shovel.
    fn roll_sword(&mut self, local: Local, rng: &mut JavaRandom) {
        let chance = if local.difficulty == 3 { 0.05 } else { 0.01 };
        if rng.next_float() < chance {
            self.held = if rng.next_int(3) == 0 {
                IRON_SWORD
            } else {
                IRON_SHOVEL
            };
        }
    }

    /// `EntityLiving.enchantEquipment`: an enchanted piece shows the glint.
    fn roll_enchantments(&mut self, multiplier: f32, rng: &mut JavaRandom) {
        if self.held != 0 && rng.next_float() < 0.25 * multiplier {
            self.held_glint = true;
            rng.next_int(18);
        }
        for piece in 0..4 {
            if self.armor[piece] != 0 && rng.next_float() < 0.5 * multiplier {
                self.armor_glint[piece] = true;
                rng.next_int(18);
            }
        }
    }
}

/// `EntitySheep.getRandomSheepColor`: the dye metadata of the fleece.
fn fleece(rng: &mut JavaRandom) -> u8 {
    match rng.next_int(100) {
        0..=4 => 15,
        5..=9 => 7,
        10..=14 => 8,
        15..=17 => 12,
        _ if rng.next_int(500) == 0 => 6,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_multiplier_follows_the_local_difficulty_formula() {
        let at = |difficulty, world_time| {
            Local {
                difficulty,
                world_time,
            }
            .multiplier()
        };
        // Hard on a new world: 3 * 0.75 = 2.25, so (2.25 - 2) / 2.
        assert!((at(3, 0) - 0.125).abs() < 1e-6);
        assert_eq!(at(2, 0), 0.0);
        assert_eq!(at(0, 10_000_000), 0.0);
        // After 1.5 million ticks, on a full moon day, the ramp is 0.25 and the moon adds 0.25: 3 * 1.25 = 3.75.
        assert!((at(3, 1_536_000) - 0.875).abs() < 1e-6);
        // Easy halves the moon and inhabited term: 1 * (0.75 + 0.25 + 0.125) < 2.
        assert_eq!(at(1, 1_536_000), 0.0);
    }

    #[test]
    fn sheep_colors_follow_the_vanilla_odds() {
        let mut rng = JavaRandom::new(7);
        let mut count = [0u32; 16];
        for _ in 0..100_000 {
            count[usize::from(fleece(&mut rng))] += 1;
        }
        let pct = |c: u32| f64::from(c) / 1000.0;
        for (color, want) in [(15, 5.0), (7, 5.0), (8, 5.0), (12, 3.0)] {
            assert!((pct(count[color]) - want).abs() < 0.5, "{color}");
        }
        assert!(count[6] > 0 && count[6] < 400, "pink is rare");
        assert!(pct(count[0]) > 81.0);
    }

    #[test]
    fn slimes_roll_sizes_one_two_and_four() {
        let local = Local {
            difficulty: 3,
            world_time: 1_536_000,
        };
        let mut rng = JavaRandom::new(3);
        let mut sizes = [0u32; 5];
        for _ in 0..30_000 {
            sizes[usize::from(Initial::roll(SLIME, local, &mut rng).slime_size)] += 1;
        }
        assert_eq!(sizes[0] + sizes[3], 0);
        // Each exponent starts at 1/3; below 2 it rises with chance 0.4375 (0.5 * multiplier).
        let share = |n: u32| f64::from(n) / 30_000.0;
        assert!((share(sizes[1]) - (1.0 - 0.4375) / 3.0).abs() < 0.02);
        assert!((share(sizes[2]) - 1.0 / 3.0).abs() < 0.02);
        assert!((share(sizes[4]) - (1.0 + 0.4375) / 3.0).abs() < 0.02);
    }

    #[test]
    fn a_zombie_rolls_babies_jockeys_armor_and_weapons() {
        let local = Local {
            difficulty: 3,
            world_time: 1_536_000,
        };
        let mut rng = JavaRandom::new(11);
        let (mut babies, mut riders, mut armored, mut armed, mut glints) = (0, 0, 0, 0, 0);
        let n = 100_000;
        for _ in 0..n {
            let z = Initial::roll(ZOMBIE, local, &mut rng);
            babies += i32::from(z.child);
            riders += i32::from(z.riding);
            armored += i32::from(z.armor[3] != 0);
            armed += i32::from(z.held != 0);
            glints += i32::from(z.armor_glint.contains(&true) || z.held_glint);
            assert!(!z.riding || z.child);
            // Pieces come as one tier, helmet down to the first miss.
            if z.armor[3] != 0 {
                assert_eq!((z.armor[3] - 298) % 4, 0, "helmet id {}", z.armor[3]);
            }
        }
        let pct = |c: i32| f64::from(c) * 100.0 / f64::from(n);
        assert!((pct(babies) - 5.0).abs() < 0.3);
        // 5% of babies, and not when the first 5% roll found a chicken.
        assert!((pct(riders) - 5.0 * 0.95 * 0.05).abs() < 0.1);
        assert!(armored > 0 && armed > 0 && glints > 0);
    }

    #[test]
    fn a_zombie_pigman_always_holds_a_golden_sword_and_wears_nothing() {
        let local = Local {
            difficulty: 3,
            world_time: 1_536_000,
        };
        let mut rng = JavaRandom::new(5);
        for _ in 0..200 {
            let p = Initial::roll(PIG_ZOMBIE, local, &mut rng);
            assert_eq!((p.held, p.armor), (GOLDEN_SWORD, [0; 4]));
        }
    }
}
