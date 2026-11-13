//! `RenderGlobal.renderWorldBorder`: the blue force-field walls of the world border, drawn as scrolling
//! `forcefield.png` quads over the nearest 128 block high strips, fading in as the viewer nears a wall.

use crate::mesh::Vertex;

pub const TEXTURE: &str = "textures/misc/forcefield.png";

/// One frame of the border as the engine sees it.
#[derive(Clone, Copy, Debug)]
pub struct Border {
    /// `[minX, minZ, maxX, maxZ]` of the walls.
    pub walls: [f64; 4],
    /// The viewer's interpolated position.
    pub viewer: [f64; 3],
    /// Render distance in blocks (`renderDistanceChunks * 16`).
    pub range: f64,
    /// `WorldBorder.getClosestDistance` of the viewer.
    pub closest: f64,
    /// `EnumBorderStatus` color.
    pub rgb: [f32; 3],
    /// `Minecraft.getSystemTime() % 3000`: the texture scrolls once per 3 seconds.
    pub phase_ms: u32,
}

/// The wall quads relative to `eye`; empty while the viewer is farther than `range` from every wall.
pub fn quads(b: &Border, eye: [f64; 3]) -> Vec<Vertex> {
    let [min_x, min_z, max_x, max_z] = b.walls;
    let [x, _, z] = b.viewer;
    let d0 = b.range;
    if !(x >= max_x - d0 || x <= min_x + d0 || z >= max_z - d0 || z <= min_z + d0) {
        return Vec::new();
    }
    let alpha = (1.0 - b.closest / d0).powi(4) as f32;
    let color = [b.rgb[0], b.rgb[1], b.rgb[2], alpha];
    let f3 = (f64::from(b.phase_ms) / 3000.0) as f32;
    let mut out = Vec::new();
    // One strip of unit steps along a wall; `at(t)` is the wall point at the running coordinate `t`.
    let mut wall = |from: f64, to: f64, at: &dyn Fn(f64, f64) -> [f64; 3]| {
        let (mut t, mut f7) = (from, 0.0f32);
        while t < to {
            let step = (to - t).min(1.0);
            let f8 = step as f32 * 0.5;
            for (along, y, u, v) in [
                (t, 256.0, f3 + f7, f3),
                (t + step, 256.0, f3 + f8 + f7, f3),
                (t + step, 0.0, f3 + f8 + f7, f3 + 128.0),
                (t, 0.0, f3 + f7, f3 + 128.0),
            ] {
                let p = at(along, y);
                out.push(Vertex {
                    pos: [0, 1, 2].map(|i| (p[i] - eye[i]) as f32),
                    uv: [u, v],
                    color,
                    light: [1.0; 2],
                });
            }
            t += 1.0;
            f7 += 0.5;
        }
    };
    let z_lo = (z - d0).floor().max(min_z);
    let z_hi = (z + d0).ceil().min(max_z);
    if x > max_x - d0 {
        wall(z_lo, z_hi, &|t, y| [max_x, y, t]);
    }
    if x < min_x + d0 {
        wall(z_lo, z_hi, &|t, y| [min_x, y, t]);
    }
    let x_lo = (x - d0).floor().max(min_x);
    let x_hi = (x + d0).ceil().min(max_x);
    if z > max_z - d0 {
        wall(x_lo, x_hi, &|t, y| [t, y, max_z]);
    }
    if z < min_z + d0 {
        wall(x_lo, x_hi, &|t, y| [t, y, min_z]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(viewer: [f64; 3], closest: f64) -> Border {
        Border {
            walls: [-10.0, -10.0, 10.0, 10.0],
            viewer,
            range: 4.0,
            closest,
            rgb: [0.0; 3],
            phase_ms: 0,
        }
    }

    #[test]
    fn nothing_far_from_every_wall() {
        assert!(quads(&at([0.0, 64.0, 0.0], 10.0), [0.0; 3]).is_empty());
    }

    /// Near the +x wall: one quad per block along z within the range (8 blocks), alpha `(1 - 2/4)^4`.
    #[test]
    fn one_wall_of_unit_quads_with_the_distance_fade() {
        let v = quads(&at([8.0, 64.0, 0.0], 2.0), [0.0; 3]);
        assert_eq!(v.len(), 8 * 4);
        assert!(v.iter().all(|v| v.pos[0] == 10.0 && v.color[3] == 0.0625));
        assert_eq!(v[0].uv, [0.0, 0.0]);
        assert_eq!(v[2].uv, [0.5, 128.0]);
    }
}
