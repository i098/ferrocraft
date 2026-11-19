//! Skeleton and zombie variants: the wither skeleton (`RenderSkeleton`) and the zombie villager
//! (`RenderZombie` with `ModelZombieVillager`), both told apart by metadata byte 13 in `Look::variant`.

use super::{biped, Class, EntityDraw, Model, Skin};

const SKELETON: u8 = 51;
const ZOMBIE: u8 = 54;

/// The body model of a wither skeleton or a zombie villager; `None` for every other entity.
pub(super) fn model(d: &EntityDraw) -> Option<Model> {
    match d.class {
        Class::Mob(SKELETON) if d.look.variant == 1 => {
            Some(biped(d, Skin::WitherSkeleton, [1.0; 3], 2.0, 2.0, true))
        }
        Class::Mob(ZOMBIE) if d.look.variant == 1 => Some(zombie_villager(d)),
        _ => None,
    }
}

/// The armor layers: `ModelSkeleton(f, true)` keeps the 4x12x4 limbs of a biped, and
/// `ModelZombieVillager(f, 0, true)` has a plain head.
pub(super) fn armor_model(d: &EntityDraw) -> Option<Model> {
    match d.class {
        Class::Mob(SKELETON) => Some(biped(d, Skin::Skeleton, [1.0; 3], 4.0, 4.0, true)),
        Class::Mob(ZOMBIE) => Some(biped(d, Skin::Zombie, [1.0; 3], 4.0, 4.0, true)),
        _ => None,
    }
}

/// `preRenderCallback`: `RenderGiantZombie` is built with scale 6 and `RenderSkeleton` scales the
/// wither skeleton by 1.2.
pub(super) fn scale(d: &EntityDraw) -> f32 {
    match d.class {
        Class::Mob(SKELETON) if d.look.variant == 1 => 1.2,
        Class::Mob(SKELETON) => 1.0,
        _ => 6.0,
    }
}

/// `ModelZombieVillager`: a zombie whose 8x10x8 head (texture 0, 32) has a 2x4x2 nose (24, 32).
fn zombie_villager(d: &EntityDraw) -> Model {
    let mut m = biped(d, Skin::ZombieVillager, [1.0; 3], 4.0, 4.0, true);
    let mut nose = m.parts[0].clone();
    (nose.min, nose.size, nose.uv) = ([-1.0, -3.0, -6.0], [2.0, 4.0, 2.0], (24.0, 32.0));
    let head = &mut m.parts[0];
    (head.min, head.size, head.uv) = ([-4.0, -10.0, -4.0], [8.0, 10.0, 8.0], (0.0, 32.0));
    m.parts.push(nose);
    m
}
