//! `LayerHeldBlock`: the block an enderman carries, as the block model of its state in front of the
//! chest. The placement is the one of the old flat cube: 8 pixels, turned 45 degrees about y and 20
//! about x, 11 pixels down and 12 in front of the body origin, upright in the y-down model space.

use super::item::{ItemModel, Items, Models};
use super::{light_level, rotate, Class, EntityDraw, Xf, V3};

fn shows(d: &EntityDraw) -> bool {
    d.class == Class::Mob(super::mobs::ENDERMAN) && !d.invisible && d.look.carried != 0
}

/// The block model `build` needs: the carried state, as `id << 4 | meta`. Plants have a flat item
/// model, so they ask for the block form.
pub fn wanted(d: &EntityDraw) -> impl Iterator<Item = (i16, i16)> {
    let c = d.look.carried;
    let id = (c >> 4) as i16;
    let meta = if (37..=40).contains(&id) {
        super::mooshroom::BLOCK_FORM - (c & 15) as i16
    } else {
        (c & 15) as i16
    };
    shows(d).then_some((id, meta)).into_iter()
}

fn piece(out: &mut Items, d: &EntityDraw, m: &ItemModel, level: [f32; 2]) {
    let xf = Xf::new(d, false);
    let to_world = |p: V3| -> V3 {
        let q = rotate(
            [-p[0] * 8.0, -p[1] * 8.0 - 1.0, p[2] * 8.0],
            [0.0, 45f32.to_radians(), 0.0],
        );
        let q = rotate(q, [20f32.to_radians(), 0.0, 0.0]);
        xf.world([q[0], q[1] + 11.0, q[2] - 12.0])
    };
    let origin = to_world([0.0; 3]);
    let normal = |n: V3| {
        let t = to_world(n);
        [t[0] - origin[0], t[1] - origin[1], t[2] - origin[2]]
    };
    out.gl_lit = true;
    out.piece(m, &to_world, &normal, level, false, None);
    out.gl_lit = false;
}

/// Meshes the carried block of every enderman of `draws` into `out`.
pub fn build(
    out: &mut Items,
    draws: &[EntityDraw],
    models: &Models,
    light: impl Fn([f64; 3]) -> [f32; 2],
) {
    for d in draws {
        let Some(Some(m)) = wanted(d).next().and_then(|k| models.get(&k)) else {
            continue;
        };
        piece(out, d, m, light_level(d, &light, false));
    }
}
