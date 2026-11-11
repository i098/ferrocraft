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

/// Mobs with fixed sounds: (type id, hurt, death, volume). `getHurtSound`/`getDeathSound` and
/// `getSoundVolume` of each 1.8.9 class. Slimes, guardians and horses depend on metadata.
const FIXED: &[(u8, &str, &str, f32)] = &[
    (50, "mob.creeper.say", "mob.creeper.death", 1.0),
    (51, "mob.skeleton.hurt", "mob.skeleton.death", 1.0),
    (52, "mob.spider.say", "mob.spider.death", 1.0),
    (59, "mob.spider.say", "mob.spider.death", 1.0),
    (54, "mob.zombie.hurt", "mob.zombie.death", 1.0),
    (56, "mob.ghast.scream", "mob.ghast.death", 10.0),
    (57, "mob.zombiepig.zpighurt", "mob.zombiepig.zpigdeath", 1.0),
    (58, "mob.endermen.hit", "mob.endermen.death", 1.0),
    (60, "mob.silverfish.hit", "mob.silverfish.kill", 1.0),
    (67, "mob.silverfish.hit", "mob.silverfish.kill", 1.0),
    (61, "mob.blaze.hit", "mob.blaze.death", 1.0),
    (63, "mob.enderdragon.hit", "game.neutral.die", 5.0),
    (64, "mob.wither.hurt", "mob.wither.death", 1.0),
    (65, "mob.bat.hurt", "mob.bat.death", 0.1),
    (90, "mob.pig.say", "mob.pig.death", 1.0),
    (91, "mob.sheep.say", "mob.sheep.say", 1.0),
    (92, "mob.cow.hurt", "mob.cow.hurt", 0.4),
    (96, "mob.cow.hurt", "mob.cow.hurt", 0.4),
    (93, "mob.chicken.hurt", "mob.chicken.hurt", 1.0),
    (95, "mob.wolf.hurt", "mob.wolf.death", 0.4),
    (98, "mob.cat.hitt", "mob.cat.hitt", 0.4),
    (99, "mob.irongolem.hit", "mob.irongolem.death", 1.0),
    (101, "mob.rabbit.hurt", "mob.rabbit.death", 1.0),
    (120, "mob.villager.hit", "mob.villager.death", 1.0),
    // Giant: `EntityMob` defaults.
    (53, "game.hostile.hurt", "game.hostile.die", 1.0),
    (49, "game.hostile.hurt", "game.hostile.die", 1.0),
];

/// Mobs that play no hurt or death sound: witch, squid and snowman.
const SILENT: &[u8] = &[66, 94, 97];

/// Guardian sound: elder variants and the land variants depend on metadata and water.
fn guardian(elder: bool, in_water: bool, hurt: bool) -> &'static str {
    const NAMES: [[&str; 2]; 3] = [
        ["mob.guardian.land.hit", "mob.guardian.land.death"],
        ["mob.guardian.hit", "mob.guardian.death"],
        ["mob.guardian.elder.hit", "mob.guardian.elder.death"],
    ];
    let row = usize::from(in_water) + usize::from(in_water && elder);
    NAMES[row][usize::from(!hurt)]
}

/// Horse sound by `getType`: 0 horse, 1 donkey, 2 mule, 3 zombie, 4 skeleton.
fn horse(kind: u8, hurt: bool) -> &'static str {
    const NAMES: [[&str; 2]; 4] = [
        ["mob.horse.hit", "mob.horse.death"],
        ["mob.horse.donkey.hit", "mob.horse.donkey.death"],
        ["mob.horse.zombie.hit", "mob.horse.zombie.death"],
        ["mob.horse.skeleton.hit", "mob.horse.skeleton.death"],
    ];
    let row = match kind {
        1 | 2 => 1,
        3 => 2,
        4 => 3,
        _ => 0,
    };
    NAMES[row][usize::from(!hurt)]
}

fn sound(kind: Kind, meta: &SizeMeta, in_water: bool, hurt: bool) -> Option<Living> {
    let pick = |h, d| if hurt { h } else { d };
    let id = match kind {
        Kind::Player => return Some((pick("game.player.hurt", "game.player.die"), 1.0)),
        Kind::Mob(id) => id,
        _ => return None,
    };
    if SILENT.contains(&id) {
        return None;
    }
    if let Some(&(_, h, d, v)) = FIXED.iter().find(|r| r.0 == id) {
        return Some((pick(h, d), v));
    }
    let size = meta.slime.unwrap_or(1);
    Some(match id {
        55 | 62 => (
            if size > 1 {
                "mob.slime.big"
            } else {
                "mob.slime.small"
            },
            0.4 * f32::from(size),
        ),
        68 => (
            guardian(meta.guardian.is_some_and(|g| g & 4 != 0), in_water, hurt),
            1.0,
        ),
        100 => (horse(meta.horse.unwrap_or(0), hurt), 0.8),
        _ => (pick("game.neutral.hurt", "game.neutral.die"), 1.0),
    })
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
