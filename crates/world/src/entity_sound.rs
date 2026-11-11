//! Hurt and death sounds of living entities: `getHurtSound`, `getDeathSound` and
//! `getSoundVolume` of the 1.8.9 entity classes, as played on entity status 2 and 3.

use crate::entities::{Kind, SizeMeta};

/// Sound event and volume.
pub type Living = (&'static str, f32);

/// `EntityLivingBase.getHurtSound` for status 2; `None` plays nothing.
pub fn hurt(kind: Kind, meta: &SizeMeta, in_water: bool) -> Option<Living> {
    sound(kind, meta, in_water, true)
}

/// `EntityLivingBase.getDeathSound` for status 3; `None` plays nothing.
pub fn death(kind: Kind, meta: &SizeMeta, in_water: bool) -> Option<Living> {
    sound(kind, meta, in_water, false)
}

fn sound(kind: Kind, meta: &SizeMeta, in_water: bool, hurt: bool) -> Option<Living> {
    let pick = |h, d| if hurt { h } else { d };
    let one = |h, d| Some((pick(h, d), 1.0));
    match kind {
        Kind::Player => one("game.player.hurt", "game.player.die"),
        Kind::Mob(id) => match id {
            50 => one("mob.creeper.say", "mob.creeper.death"),
            51 => one("mob.skeleton.hurt", "mob.skeleton.death"),
            52 | 59 => one("mob.spider.say", "mob.spider.death"),
            54 => one("mob.zombie.hurt", "mob.zombie.death"),
            55 | 62 => {
                let name = if meta.slime.unwrap_or(1) > 1 {
                    "mob.slime.big"
                } else {
                    "mob.slime.small"
                };
                Some((name, 0.4 * f32::from(meta.slime.unwrap_or(1))))
            }
            56 => Some((pick("mob.ghast.scream", "mob.ghast.death"), 10.0)),
            57 => one("mob.zombiepig.zpighurt", "mob.zombiepig.zpigdeath"),
            58 => one("mob.endermen.hit", "mob.endermen.death"),
            60 | 67 => one("mob.silverfish.hit", "mob.silverfish.kill"),
            61 => one("mob.blaze.hit", "mob.blaze.death"),
            63 => Some((pick("mob.enderdragon.hit", "game.neutral.die"), 5.0)),
            64 => one("mob.wither.hurt", "mob.wither.death"),
            65 => Some((pick("mob.bat.hurt", "mob.bat.death"), 0.1)),
            66 | 94 => None,
            68 => {
                let elder = meta.guardian.is_some_and(|g| g & 4 != 0);
                Some((
                    match (in_water, elder, hurt) {
                        (false, _, true) => "mob.guardian.land.hit",
                        (false, _, false) => "mob.guardian.land.death",
                        (true, true, true) => "mob.guardian.elder.hit",
                        (true, true, false) => "mob.guardian.elder.death",
                        (true, false, true) => "mob.guardian.hit",
                        (true, false, false) => "mob.guardian.death",
                    },
                    1.0,
                ))
            }
            90 => one("mob.pig.say", "mob.pig.death"),
            91 => one("mob.sheep.say", "mob.sheep.say"),
            92 | 96 => Some(("mob.cow.hurt", 0.4)),
            93 => one("mob.chicken.hurt", "mob.chicken.hurt"),
            95 => Some((pick("mob.wolf.hurt", "mob.wolf.death"), 0.4)),
            97 => None,
            98 => Some(("mob.cat.hitt", 0.4)),
            99 => one("mob.irongolem.hit", "mob.irongolem.death"),
            100 => {
                let name = match (meta.horse.unwrap_or(0), hurt) {
                    (3, true) => "mob.horse.zombie.hit",
                    (3, false) => "mob.horse.zombie.death",
                    (4, true) => "mob.horse.skeleton.hit",
                    (4, false) => "mob.horse.skeleton.death",
                    (1 | 2, true) => "mob.horse.donkey.hit",
                    (1 | 2, false) => "mob.horse.donkey.death",
                    (_, true) => "mob.horse.hit",
                    (_, false) => "mob.horse.death",
                };
                Some((name, 0.8))
            }
            101 => one("mob.rabbit.hurt", "mob.rabbit.death"),
            120 => one("mob.villager.hit", "mob.villager.death"),
            // Giant, unknown living mobs: `EntityMob`/`EntityLivingBase` defaults.
            53 | 49 => one("game.hostile.hurt", "game.hostile.die"),
            _ => one("game.neutral.hurt", "game.neutral.die"),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_follow_metadata_and_water() {
        let m = SizeMeta::default();
        assert_eq!(
            hurt(Kind::Mob(54), &m, false),
            Some(("mob.zombie.hurt", 1.0))
        );
        assert_eq!(
            death(Kind::Player, &m, false),
            Some(("game.player.die", 1.0))
        );
        assert_eq!(hurt(Kind::Mob(94), &m, true), None);
        let slime = SizeMeta {
            slime: Some(4),
            ..m
        };
        assert_eq!(
            hurt(Kind::Mob(55), &slime, false),
            Some(("mob.slime.big", 1.6))
        );
        let elder = SizeMeta {
            guardian: Some(4),
            ..m
        };
        assert_eq!(
            death(Kind::Mob(68), &elder, true).unwrap().0,
            "mob.guardian.elder.death"
        );
        assert_eq!(
            hurt(Kind::Mob(68), &elder, false).unwrap().0,
            "mob.guardian.land.hit"
        );
        let skeleton_horse = SizeMeta {
            horse: Some(4),
            ..m
        };
        assert_eq!(
            hurt(Kind::Mob(100), &skeleton_horse, false),
            Some(("mob.horse.skeleton.hit", 0.8))
        );
    }
}
