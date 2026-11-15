//! Boats (object 1) and minecarts (objects 10 to 12). Ported from `BoatModel`, `BoatRenderer.render`,
//! `MinecartModel` and `MinecartRenderer.render`; the part trees were read from the real 1.8.9 classes
//! (`BoatModel`: 5 parts, `MinecartModel`: 6 parts, both on a 64x32 texture). The block a cart carries
//! is placed by [`block_frame`] and meshed by `block::build`; the rail snap is
//! `world::entities::vehicle::on_rail`, applied to the draw by the caller.
//!
//! shortcut: a cart carrying a chest, ender chest or trapped chest shows no block. Vanilla draws it
//! through `AnimatedBlockEntityRenderer`; the chest model of `blockent` is not wired to carts yet.

use super::{add_box, rotate, skin_dims, Class, EntityDraw, Skin, Skins, V3};
use crate::hand::{chain, scale3, translate, Mat};
use crate::hand::{rotate as turn, scale};
use crate::mesh::MeshData;
use std::f32::consts::{FRAC_PI_2, PI};

/// `EntityBoat` and `EntityMinecart` metadata the renderers read, interpolated by the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleLook {
    /// Metadata 17, 18 and 19: `getTimeSinceHit`, `getForwardDirection` and `getDamageTaken`.
    pub hit_timer: i32,
    pub hit_direction: i32,
    pub damage: f32,
    /// `getDisplayBlock` of a cart as `id << 4 | meta`, 0 for none, and `getDisplayBlockOffset`.
    pub block: u16,
    pub block_offset: i32,
}

/// Whether `build` draws this object type.
pub fn handles(id: i8) -> bool {
    id == 1 || is_cart(id)
}

pub fn is_cart(id: i8) -> bool {
    matches!(id, 10..=12)
}

/// A box of `BoatModel` or `MinecartModel`: pivot, rotation, corner, size and texture origin.
struct Cube {
    pivot: V3,
    rot: V3,
    min: V3,
    size: V3,
    uv: (f32, f32),
}

const fn cube(pivot: V3, rot: V3, min: V3, size: V3, uv: (f32, f32)) -> Cube {
    Cube {
        pivot,
        rot,
        min,
        size,
        uv,
    }
}

const THREE_HALVES_PI: f32 = 4.712_389;

/// `BoatModel`: the 24x16x4 floor turned flat and four 20x6x2 walls.
const BOAT: [Cube; 5] = [
    cube(
        [0.0, 4.0, 0.0],
        [FRAC_PI_2, 0.0, 0.0],
        [-12.0, -8.0, -3.0],
        [24.0, 16.0, 4.0],
        (0.0, 8.0),
    ),
    cube(
        [-11.0, 4.0, 0.0],
        [0.0, THREE_HALVES_PI, 0.0],
        [-10.0, -7.0, -1.0],
        [20.0, 6.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [11.0, 4.0, 0.0],
        [0.0, FRAC_PI_2, 0.0],
        [-10.0, -7.0, -1.0],
        [20.0, 6.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [0.0, 4.0, -9.0],
        [0.0, PI, 0.0],
        [-10.0, -7.0, -1.0],
        [20.0, 6.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [0.0, 4.0, 9.0],
        [0.0; 3],
        [-10.0, -7.0, -1.0],
        [20.0, 6.0, 2.0],
        (0.0, 0.0),
    ),
];

/// `MinecartModel`: the 20x16x2 floor, four 16x8x2 walls and the 18x14x1 cover, whose pivot
/// `MinecartModel.render` lifts to `4 - (-0.1)`.
const CART: [Cube; 6] = [
    cube(
        [0.0, 4.0, 0.0],
        [FRAC_PI_2, 0.0, 0.0],
        [-10.0, -8.0, -1.0],
        [20.0, 16.0, 2.0],
        (0.0, 10.0),
    ),
    cube(
        [-9.0, 4.0, 0.0],
        [0.0, THREE_HALVES_PI, 0.0],
        [-8.0, -9.0, -1.0],
        [16.0, 8.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [9.0, 4.0, 0.0],
        [0.0, FRAC_PI_2, 0.0],
        [-8.0, -9.0, -1.0],
        [16.0, 8.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [0.0, 4.0, -7.0],
        [0.0, PI, 0.0],
        [-8.0, -9.0, -1.0],
        [16.0, 8.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [0.0, 4.0, 7.0],
        [0.0; 3],
        [-8.0, -9.0, -1.0],
        [16.0, 8.0, 2.0],
        (0.0, 0.0),
    ),
    cube(
        [0.0, 4.1, 0.0],
        [-FRAC_PI_2, 0.0, 0.0],
        [-9.0, -7.0, -1.0],
        [18.0, 14.0, 1.0],
        (44.0, 10.0),
    ),
];

/// The hit wobble about x: `sin(t) * t * damage / 10 * direction` degrees, `t` the hit timer less
/// the partial tick, `damage` the damage taken less the partial tick, floored at 0.
fn wobble(d: &EntityDraw) -> f32 {
    let v = d.look.vehicle;
    let (t, damage) = (
        v.hit_timer as f32 - d.look.partial,
        (v.damage - d.look.partial).max(0.0),
    );
    if t > 0.0 {
        world::sin(t) * t * damage / 10.0 * v.hit_direction as f32
    } else {
        0.0
    }
}

/// Placement of a boat or cart relative to its draw position, from the `render` of its renderer:
/// the boat sits 0.25 up, a cart 0.375 up and nudged by up to 0.004 per axis (a hash of the entity
/// id, against z-fighting of two carts), then yaw, the cart's pitch and the wobble.
fn frame(d: &EntityDraw) -> Mat {
    let Class::Object(id) = d.class else {
        return translate(0.0, 0.0, 0.0);
    };
    if !is_cart(id) {
        return chain(&[
            translate(0.0, 0.25, 0.0),
            turn(180.0 - d.yaw, 1),
            turn(wobble(d), 0),
        ]);
    }
    let seed = i64::from(d.look.id).wrapping_mul(493_286_711);
    let seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(4_392_167_121)
        .wrapping_add(seed.wrapping_mul(98_761));
    let jitter = |shift: u32| ((((seed >> shift) & 7) as f32 + 0.5) / 8.0 - 0.5) * 0.004;
    chain(&[
        translate(jitter(16), jitter(20), jitter(24)),
        translate(0.0, 0.375, 0.0),
        turn(180.0 - d.yaw, 1),
        turn(-d.pitch, 2),
        turn(wobble(d), 0),
    ])
}

/// Where the block a cart carries sits: `scale(0.75)`, down to the block's offset and turned a
/// quarter as `renderAsItem` does, for a model centred on the origin.
pub fn block_frame(d: &EntityDraw) -> Mat {
    let lift = (d.look.vehicle.block_offset - 8) as f32 / 16.0 + 0.5;
    chain(&[
        frame(d),
        scale(0.75),
        translate(0.0, lift, 0.0),
        turn(90.0, 1),
    ])
}

/// The boat or cart model, textured and placed in the world.
pub fn build(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let Class::Object(id) = d.class else { return };
    let (skin, cubes, tint): (Skin, &[Cube], [f32; 3]) = if is_cart(id) {
        (Skin::Minecart, &CART, [0.6; 3])
    } else {
        (Skin::Boat, &BOAT, [0.6, 0.45, 0.25])
    };
    let color = if skins.has(skin) { [1.0; 3] } else { tint };
    let v_off = skins.v_offset(skin);
    // `scalef(-1, -1, 1)` turns the model's y-down pixels upright.
    let model = chain(&[frame(d), scale3([-1.0, -1.0, 1.0])]);
    for c in cubes {
        let tf = |p: V3| -> V3 {
            let q = rotate(p, c.rot);
            let q = super::item::point(&model, [0, 1, 2].map(|i| (q[i] + c.pivot[i]) / 16.0));
            [0, 1, 2].map(|i| (d.pos[i] + f64::from(q[i])) as f32)
        };
        add_box(
            out,
            &tf,
            c.min,
            c.size,
            c.size,
            c.uv,
            false,
            v_off,
            color,
            skin_dims(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(id: i8) -> EntityDraw {
        let mut d = super::super::tests::steve();
        d.class = Class::Object(id);
        d.pos = [0.0; 3];
        d.yaw = 0.0;
        d
    }

    fn bounds(id: i8, mutate: impl Fn(&mut EntityDraw)) -> ([f32; 3], [f32; 3], usize) {
        let mut d = draw(id);
        mutate(&mut d);
        let mut out = MeshData::default();
        build(&mut out, &d, &Skins::flat());
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &out.vertices {
            for i in 0..3 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
            }
        }
        (lo, hi, out.vertices.len())
    }

    /// 5 boat boxes and 6 cart boxes of 24 vertices each. The boat is 24 px wide along x and 20 px
    /// long along z; its walls rise to 0.4375 over the feet. The cart is 20 px by 16 px.
    #[test]
    fn the_boat_and_cart_have_the_vanilla_boxes_and_extents() {
        let (lo, hi, n) = bounds(1, |_| {});
        assert_eq!(n, 5 * 24);
        let size = [hi[0] - lo[0], hi[2] - lo[2]];
        assert!(
            (size[0] - 1.5).abs() < 1e-4 && (size[1] - 1.25).abs() < 1e-4,
            "{lo:?} {hi:?}"
        );
        assert!((hi[1] - 0.4375).abs() < 1e-4);
        let (lo, hi, n) = bounds(10, |_| {});
        assert_eq!(n, 6 * 24);
        let size = [hi[0] - lo[0], hi[2] - lo[2]];
        assert!(
            (size[0] - 1.25).abs() < 1e-4 && (size[1] - 1.0).abs() < 1e-4,
            "{lo:?} {hi:?}"
        );
    }

    /// `BoatRenderer.render` only wobbles while the hit timer runs, about x.
    #[test]
    fn a_hit_boat_tilts_and_an_idle_one_does_not() {
        let still = bounds(1, |_| {});
        let hit = bounds(1, |d| {
            d.look.vehicle = VehicleLook {
                hit_timer: 8,
                hit_direction: 1,
                damage: 20.0,
                ..Default::default()
            };
        });
        assert_ne!(still.0, hit.0);
        let expired = bounds(1, |d| {
            d.look.vehicle.hit_timer = 0;
            d.look.vehicle.damage = 30.0;
        });
        assert_eq!(still.0, expired.0);
    }

    /// `MinecartRenderer`: the pitch turns the cart about z, so a pitched cart is taller along z.
    #[test]
    fn a_pitched_cart_changes_its_extent() {
        let flat = bounds(10, |_| {});
        let up = bounds(10, |d| d.pitch = 30.0);
        assert_ne!(flat.0[1], up.0[1]);
    }

    /// A cart's carried block: its offset lowers it by sixteenths and `0.75` shrinks it.
    #[test]
    fn the_carried_block_sits_in_the_hull() {
        let mut d = draw(10);
        d.look.vehicle.block_offset = 8;
        let m = block_frame(&d);
        let c = super::super::item::point(&m, [0.0; 3]);
        assert!((c[1] - (0.375 + 0.75 * 0.5)).abs() < 0.01, "{c:?}");
    }
}
