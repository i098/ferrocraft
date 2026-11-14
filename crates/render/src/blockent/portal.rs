//! `TileEntityEndPortalRenderer`: the star field inside an end portal. Vanilla draws 16 copies of
//! the top face at `y + 0.75`; GL texture generation (object-linear `s`, `t` and eye-linear `q`)
//! and a texture matrix turn the camera position into a different scale, tilt and parallax for
//! each copy. Every copy here is one quad whose texture coordinates are that transform applied per
//! corner. `q` is the same on all four corners, so the per-corner result equals GL's divide.
//!
//! Not ported: a camera that is not at the view entity (third person) is treated as at it.

use super::{in_range, END_PORTAL};
use crate::jrandom::JavaRandom;
use crate::mesh::{MeshData, Vertex};
use crate::Camera;

pub const SKY_TEXTURE: &str = "textures/environment/end_sky.png";
pub const PORTAL_TEXTURE: &str = "textures/entity/end_portal.png";

/// `ActiveRenderInfo.getPosition` is the centre of the near plane, 0.05 in front of the eye.
const NEAR: f64 = 0.05;
/// The texture matrix scrolls once per 700 s of `Minecraft.getSystemTime`.
const SCROLL_MS: u64 = 700_000;

/// The star field of one frame: layer 0 (`end_sky.png`, alpha blended) and layers 1 to 15
/// (`end_portal.png`, added).
#[derive(Default)]
pub struct Portals {
    pub sky: MeshData,
    pub layers: MeshData,
}

/// Per-frame constants of the texture transform that do not depend on the portal.
struct Frame {
    /// `rendererDispatcher.entityX` and `entityZ`.
    eye: [f32; 2],
    /// `ActiveRenderInfo.getPosition()` x and z, relative to the view entity.
    near: [f32; 2],
    scroll: f32,
}

/// One layer of the 16: its fixed texture scale, depth, colour and angle.
struct Layer {
    /// `f4`, how far below the plane the layer seems to lie.
    depth: f32,
    /// `f5`, the texture scale.
    scale: f32,
    color: [f32; 3],
    /// `rotate((i * i * 4321 + i * 9) * 2, 0, 0, 1)` in degrees.
    angle: f32,
}

/// The 16 layers as `renderTileEntityAt` sets them; the colours come from a `Random` seeded 31100.
fn layers() -> Vec<Layer> {
    let mut rng = JavaRandom::new(31100);
    (0..16)
        .map(|i: i32| {
            let mut depth = (16 - i) as f32;
            let mut scale = if i == 1 { 0.5 } else { 0.0625 };
            let mut shade = 1.0 / (depth + 1.0);
            let r = (rng.next_float() * 0.5 + 0.1) * shade;
            let g = (rng.next_float() * 0.5 + 0.4) * shade;
            let b = (rng.next_float() * 0.5 + 0.5) * shade;
            let mut color = [r, g, b];
            if i == 0 {
                depth = 65.0;
                scale = 0.125;
                shade = 0.1;
                color = [shade; 3];
            }
            Layer {
                depth,
                scale,
                color,
                angle: (i * i * 4321 + i * 9) as f32 * 2.0,
            }
        })
        .collect()
}

impl super::BlockEntities {
    /// Builds the star field of every end portal in range for the camera `cam` at wall-clock time
    /// `now_ms`.
    pub fn portals(&self, cam: &Camera, now_ms: u64) -> Portals {
        let look = look(cam);
        let frame = Frame {
            eye: [cam.pos[0] as f32, cam.pos[2] as f32],
            near: [(look[0] * NEAR) as f32, (look[2] * NEAR) as f32],
            scroll: (now_ms % SCROLL_MS) as f32 / SCROLL_MS as f32,
        };
        let layers = layers();
        let mut out = Portals::default();
        for (&pos, &state) in &self.blocks {
            if state >> 4 != END_PORTAL || !in_range(&pos, cam.pos) {
                continue;
            }
            // `f8`: the eye height above the plane, the near plane included.
            let above = (cam.pos[1] + look[1] * NEAR) - (f64::from(pos[1]) + 0.75);
            for (i, layer) in layers.iter().enumerate() {
                let target = if i == 0 {
                    &mut out.sky
                } else {
                    &mut out.layers
                };
                quad(target, pos, layer, &frame, above as f32, cam.pos);
            }
        }
        out
    }
}

/// The unit view direction of the camera (yaw 0 looks toward +z, pitch > 0 down).
fn look(cam: &Camera) -> [f64; 3] {
    let (yaw, pitch) = (
        f64::from(cam.yaw).to_radians(),
        f64::from(cam.pitch).to_radians(),
    );
    [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ]
}

/// The top face of the portal at `pos` with the texture coordinates of one layer.
fn quad(
    out: &mut MeshData,
    pos: [i32; 3],
    layer: &Layer,
    frame: &Frame,
    above: f32,
    eye: [f64; 3],
) {
    let f9 = above + layer.depth;
    // A camera in the portal plane makes GL divide by zero; vanilla shows garbage, so skip it.
    if above == 0.0 || f9 == 0.0 {
        return;
    }
    // `q` of the eye-linear plane (0, 1, 0, 0): the height of the face above the layer's origin.
    let q = -above / f9;
    // Translations of the texture matrix, which act on `s` and `t` scaled by `q`.
    let shift = [0, 1].map(|k| -frame.eye[k] + frame.near[k] * layer.depth / above);
    let (sin, cos) = layer.angle.to_radians().sin_cos();
    let uv = |dx: i32, dz: i32| {
        let s = ((f64::from(pos[0] + dx) - eye[0]) as f32) / q + shift[0] - 0.5;
        let t = ((f64::from(pos[2] + dz) - eye[2]) as f32) / q + shift[1] - 0.5;
        let u = layer.scale * (cos * s - sin * t + 0.5);
        let v = layer.scale * (sin * s + cos * t + 0.5) + frame.scroll;
        [u, v]
    };
    let base = out.vertices.len() as u32;
    for (dx, dz) in [(0, 0), (0, 1), (1, 1), (1, 0)] {
        out.vertices.push(Vertex {
            pos: [
                (pos[0] + dx) as f32,
                pos[1] as f32 + 0.75,
                (pos[2] + dz) as f32,
            ],
            uv: uv(dx, dz),
            color: [layer.color[0], layer.color[1], layer.color[2], 1.0],
            light: [1.0; 2],
        });
    }
    out.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal_at(pos: [i32; 3]) -> super::super::BlockEntities {
        let mut be = super::super::BlockEntities::default();
        be.set_state(pos, END_PORTAL << 4);
        be
    }

    fn cam(pos: [f64; 3]) -> Camera {
        Camera {
            pos,
            pitch: 30.0,
            ..Camera::default()
        }
    }

    #[test]
    fn layer_zero_is_dim_grey_and_the_others_fade_with_depth() {
        let all = layers();
        assert_eq!(all[0].color, [0.1; 3]);
        assert_eq!((all[0].depth, all[0].scale), (65.0, 0.125));
        // Layer 1 is the only one at scale 0.5; the rest keep 1/16.
        assert_eq!(all[1].scale, 0.5);
        assert!(all[2..].iter().all(|l| l.scale == 0.0625));
        // Colour channels lie in `[0.1, 0.6] / (depth + 1)`, `[0.4, 0.9]` and `[0.5, 1.0]`.
        for l in &all[1..] {
            let k = 1.0 / (l.depth + 1.0);
            assert!((0.1 * k..=0.6 * k).contains(&l.color[0]));
            assert!((0.4 * k..=0.9 * k).contains(&l.color[1]));
            assert!((0.5 * k..=1.0 * k).contains(&l.color[2]));
        }
        assert_eq!(all[3].angle, (9.0 * 4321.0 + 27.0) * 2.0);
    }

    #[test]
    fn one_portal_gives_one_sky_quad_and_fifteen_added_quads_at_three_quarters() {
        let p = portal_at([4, 60, 4]).portals(&cam([4.5, 62.0, 8.5]), 0);
        assert_eq!((p.sky.vertices.len(), p.layers.vertices.len()), (4, 60));
        assert!(p.sky.vertices.iter().all(|v| v.pos[1] == 60.75));
    }

    #[test]
    fn portals_beyond_64_blocks_are_skipped() {
        let p = portal_at([100, 60, 0]).portals(&cam([0.5, 62.0, 0.5]), 0);
        assert!(p.sky.vertices.is_empty() && p.layers.vertices.is_empty());
    }

    #[test]
    fn texture_scrolls_by_a_700000th_of_a_period_per_millisecond() {
        let be = portal_at([4, 60, 4]);
        let c = cam([4.5, 62.0, 8.5]);
        let at = |ms| be.portals(&c, ms).sky.vertices[0].uv[1];
        assert!((at(350_000) - at(0) - 0.5).abs() < 1e-5);
        assert!((at(700_000) - at(0)).abs() < 1e-6);
    }
}
