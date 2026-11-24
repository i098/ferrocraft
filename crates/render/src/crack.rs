//! Block crack overlay: `RenderGlobal.renderMiningProgress` draws the model of each block being
//! mined again with the `destroy_stage_N` sprite, mapped like the block's own faces.

use crate::blocks::{cached, Kind, UP};
use crate::mesh::{
    cross_quads, face_uv, model_face_visible, rot_uv, view_opaque, Vertex, CORNERS, DIRS,
};
use crate::models::Models;
use crate::shapes::Bx;

/// Vanilla `color4f(1, 1, 1, 0.5)`.
const COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.5];

/// `BlockRendererDispatcher.renderBlockDamage` retextures every quad of a plant model: its uv
/// runs over the whole `destroy_stage_N` sprite, as over the block's own sprite.
fn cross(at: [i32; 3], state: u16) -> Vec<Vertex> {
    cross_quads(state >> 4, at)
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
pub fn quads(
    at: [i32; 3],
    state: u16,
    block: &dyn Fn(i32, i32, i32) -> u16,
    models: &Models,
    fancy: bool,
) -> Vec<Vertex> {
    let nb = |dx, dy, dz| block(at[0] + dx, at[1] + dy, at[2] + dz);
    let clear_pane = state >> 4 == 102;
    let pane_quads = if clear_pane {
        models.quads(state, &nb)
    } else {
        None
    };
    if let Some(quads) = pane_quads {
        return quads
            .iter()
            .filter(|q| model_face_visible(q, state, fancy, nb))
            .flat_map(|q| {
                std::array::from_fn::<_, 4, _>(|i| Vertex {
                    pos: [0, 1, 2].map(|a| at[a] as f32 + q.pos[i][a]),
                    uv: face_uv(q.face, q.pos[i]),
                    color: COLOR,
                    light: [1.0; 2],
                })
            })
            .collect();
    }
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
        Kind::Shape => crate::mesh::fallback_boxes(state, &nb, models, fancy),
        Kind::Cross => return cross(at, state),
        _ => Vec::new(),
    };
    box_vertices(at, &list, nb, !clear_pane || fancy)
}

fn box_vertices(
    at: [i32; 3],
    boxes: &[Bx],
    nb: impl Fn(i32, i32, i32) -> u16,
    fancy: bool,
) -> Vec<Vertex> {
    let mut out = Vec::new();
    for b in boxes {
        for f in 0..6 {
            if b.mask >> f & 1 == 0 {
                continue;
            }
            let on_border = crate::mesh::box_on_border(b, f);
            let [dx, dy, dz] = DIRS[f];
            if b.cull && on_border && view_opaque(nb(dx, dy, dz), fancy) {
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

    fn fallback(state: u16, block: &dyn Fn(i32, i32, i32) -> u16) -> Vec<Vertex> {
        quads([0; 3], state, block, &Models::default(), true)
    }

    #[test]
    fn stone_next_to_stone_hides_the_shared_face() {
        let stone = 1 << 4;
        let alone = fallback(stone, &|_, _, _| 0);
        let beside = fallback(stone, &|x, _, _| if x == 1 { stone } else { 0 });
        assert_eq!((alone.len(), beside.len()), (24, 20));
    }

    #[test]
    fn air_has_no_crack() {
        assert!(fallback(0, &|_, _, _| 0).is_empty());
    }

    #[test]
    fn plants_get_every_plane_of_their_model() {
        // Poppy: two crossed planes, both windings; wheat: four planes, both windings.
        assert_eq!(fallback(38 << 4, &|_, _, _| 0).len(), 16);
        assert_eq!(fallback((59 << 4) | 7, &|_, _, _| 0).len(), 32);
    }
}
