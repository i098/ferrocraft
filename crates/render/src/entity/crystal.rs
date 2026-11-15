//! Ender crystals (object 51). Ported from `EnderCrystalModel.render` and `EnderCrystalRenderer.render`
//! of the real 1.8.9 client (the bytecode of both was read; `harness/mobmodels.sh` prints the part
//! tree): a 12x4x12 base, then two 8x8x8 glass cubes and an 8x8x8 inner cube, each smaller and
//! turned further than the one before, spinning and bobbing on the crystal's tick counter. The
//! healing beam to a dragon belongs to the dragon renderer and is not drawn here.

use super::{add_box, skin_dims, EntityDraw, Skin, Skins, V3};
use crate::hand::rotate as turn;
use crate::hand::{chain, point, scale, translate, Mat};
use crate::mesh::MeshData;

/// Whether `build` draws this object type.
pub fn handles(id: i8) -> bool {
    id == 51
}

/// A box of `EnderCrystalModel`: corner, size and texture origin; no part has a pivot.
struct Cube {
    min: V3,
    size: V3,
    uv: (f32, f32),
}

const BASE: Cube = Cube {
    min: [-6.0, 0.0, -6.0],
    size: [12.0, 4.0, 12.0],
    uv: (0.0, 16.0),
};
const GLASS: Cube = Cube {
    min: [-4.0; 3],
    size: [8.0; 3],
    uv: (0.0, 0.0),
};
const CUBE: Cube = Cube {
    min: [-4.0; 3],
    size: [8.0; 3],
    uv: (32.0, 0.0),
};

/// `glRotatef(deg, x, y, z)` for any axis; `hand::rotate` only turns about a main axis.
fn turn_about(deg: f32, axis: V3) -> Mat {
    let len = axis.iter().map(|a| a * a).sum::<f32>().sqrt();
    let [x, y, z] = axis.map(|a| a / len);
    let (s, c) = deg.to_radians().sin_cos();
    let t = 1.0 - c;
    // Column-major: `m[column][row]`.
    [
        [t * x * x + c, t * x * y + s * z, t * x * z - s * y, 0.0],
        [t * x * y - s * z, t * y * y + c, t * y * z + s * x, 0.0],
        [t * x * z + s * y, t * y * z - s * x, t * z * z + c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The `render` arguments of `EnderCrystalRenderer`: the spin in degrees and the bob in blocks.
fn spin_and_bob(d: &EntityDraw) -> (f32, f32) {
    let f = d.look.crystal_ticks as f32 + d.look.partial;
    let s = (f * 0.2).sin() / 2.0 + 0.5;
    (f * 3.0, (s * s + s) * 0.2)
}

/// The glass, the smaller glass and the inner cube with the matrix each is drawn under, from the
/// `render` of `EnderCrystalModel`; the base is drawn under `base` alone.
fn layers(d: &EntityDraw) -> (Mat, [(Mat, &'static Cube); 3]) {
    let (spin, bob) = spin_and_bob(d);
    let base = chain(&[scale(2.0), translate(0.0, -0.5, 0.0)]);
    let tilt = turn_about(60.0, [1.0, 0.0, 1.0]); // vanilla passes (0.7071, 0, 0.7071); GL normalizes
    let y = turn(spin, 1);
    let outer = chain(&[base, y, translate(0.0, 0.8 + bob, 0.0), tilt]);
    let inner = chain(&[outer, scale(0.875), tilt, y]);
    let core = chain(&[inner, scale(0.875), tilt, y]);
    (base, [(outer, &GLASS), (inner, &GLASS), (core, &CUBE)])
}

/// The crystal, textured and placed in the world.
pub fn build(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let skin = Skin::EnderCrystal;
    let color = if skins.has(skin) {
        [1.0; 3]
    } else {
        [0.8, 0.45, 0.85]
    };
    // The texture is 128x64 but the model lays it out on 64x32, so every texel of the model is 2x2
    // atlas texels: halve the atlas size and the row offset to match.
    let v_off = skins.v_offset(skin) / 2.0;
    let [tw, th] = skin_dims();
    let (base, glass) = layers(d);
    for (m, c) in std::iter::once((base, &BASE)).chain(glass) {
        let tf = |p: V3| -> V3 {
            let q = point(&m, p.map(|v| v / 16.0));
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
            [tw / 2.0, th / 2.0],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::tests::steve;
    use crate::entity::Class;

    fn bounds(ticks: i32) -> ([f32; 3], [f32; 3], usize) {
        let mut d = steve();
        d.class = Class::Object(51);
        d.pos = [0.0; 3];
        d.look.crystal_ticks = ticks;
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

    /// The base is 12 px (1.5 blocks at scale 2) wide, 4 px (0.5) tall, and its floor sits 1 block
    /// under the entity position; the glass and the cube float above it.
    #[test]
    fn the_crystal_has_a_base_and_three_cubes() {
        let (lo, hi, n) = bounds(0);
        assert_eq!(n, 4 * 24);
        assert!((lo[1] + 1.0).abs() < 1e-4, "{lo:?}");
        assert!(hi[1] > -0.5, "cubes float over the base: {hi:?}");
        assert!(hi[0] >= 0.75 - 1e-4 && lo[0] <= -0.75 + 1e-4);
    }

    /// The spin turns the cubes, the bob moves them: two ticks give two different meshes.
    #[test]
    fn the_crystal_spins_and_bobs() {
        assert_ne!(bounds(0).1, bounds(5).1);
    }
}
