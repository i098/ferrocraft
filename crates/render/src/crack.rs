//! Block crack overlay: `RenderGlobal.renderMiningProgress` draws the model of each block being
//! mined again with the `destroy_stage_N` sprite, mapped like the block's own faces.

use crate::blocks::{cached, is_opaque_cube, Kind, UP};
use crate::mesh::{cross_quads, face_uv, rot_uv, Vertex, CORNERS, DIRS};
use crate::shapes::{boxes, Bx};

/// Vanilla `color4f(1, 1, 1, 0.5)`.
const COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.5];

/// `BlockRendererDispatcher.renderBlockDamage` retextures every quad of a plant model: its uv
/// runs over the whole `destroy_stage_N` sprite, as over the block's own sprite.
fn cross(at: [i32; 3], state: u16) -> Vec<Vertex> {
    cross_quads(state >> 4, at[0], at[2])
        .into_iter()
        .flatten()
        .map(|(rel, uv)| Vertex {
            pos: [0, 1, 2].map(|a| at[a] as f32 + rel[a]),
            uv,
            color: COLOR,
            light: [1.0; 2],
        })
        .collect()
}

/// Quads (four vertices each, world space) of the block `state` at `at`, whose faces are culled
/// against `block`, the state of any world position. Liquids have no crack model here.
pub fn quads(at: [i32; 3], state: u16, block: &dyn Fn(i32, i32, i32) -> u16) -> Vec<Vertex> {
    let unit = Bx {
        min: [0.0; 3],
        max: [1.0; 3],
        tex: [""; 6],
        tint: crate::blocks::Tint::None,
        mask: 0x3f,
        rot: 0,
        cull: true,
    };
    let list = match cached(state).kind {
        Kind::Opaque | Kind::Cutout => vec![unit],
        Kind::Shape => boxes(state, &|dx, dy, dz| {
            block(at[0] + dx, at[1] + dy, at[2] + dz)
        }),
        Kind::Cross => return cross(at, state),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for b in &list {
        for f in 0..6 {
            if b.mask >> f & 1 == 0 {
                continue;
            }
            let on_border = [
                b.max[1] >= 1.0,
                b.min[1] <= 0.0,
                b.min[2] <= 0.0,
                b.max[2] >= 1.0,
                b.min[0] <= 0.0,
                b.max[0] >= 1.0,
            ][f];
            let d = DIRS[f];
            if b.cull
                && on_border
                && is_opaque_cube(block(at[0] + d[0], at[1] + d[1], at[2] + d[2]))
            {
                continue;
            }
            for c in &CORNERS[f] {
                let p = [0, 1, 2].map(|a| if c[a] > 0.5 { b.max[a] } else { b.min[a] });
                let mut uv = face_uv(f, p);
                if f == UP {
                    uv = rot_uv(uv, b.rot);
                }
                out.push(Vertex {
                    pos: [0, 1, 2].map(|a| at[a] as f32 + p[a]),
                    uv,
                    color: COLOR,
                    light: [1.0; 2],
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_next_to_stone_hides_the_shared_face() {
        let stone = 1 << 4;
        let alone = quads([0, 0, 0], stone, &|_, _, _| 0);
        let beside = quads([0, 0, 0], stone, &|x, _, _| if x == 1 { stone } else { 0 });
        assert_eq!((alone.len(), beside.len()), (24, 20));
    }

    #[test]
    fn air_has_no_crack() {
        assert!(quads([0, 0, 0], 0, &|_, _, _| 0).is_empty());
    }

    #[test]
    fn plants_get_every_plane_of_their_model() {
        // Poppy: two crossed planes, both windings; wheat: four planes, both windings.
        assert_eq!(quads([0, 0, 0], 38 << 4, &|_, _, _| 0).len(), 16);
        assert_eq!(quads([0, 0, 0], (59 << 4) | 7, &|_, _, _| 0).len(), 32);
    }
}
