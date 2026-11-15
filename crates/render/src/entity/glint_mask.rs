//! Texel mask of the glint in the GUI. Vanilla 1.8.9 draws the glint with `GL_EQUAL` depth after the
//! alpha-tested model, so it shows only where the model texture is opaque. A GUI overlay quad has one
//! texture, so the glint quads are cut here along the texel grid of the model texture and the
//! transparent texels are dropped.

use crate::mesh::{MeshData, Vertex};

/// Alpha test of `GL_GREATER, 0.1`: a texel of alpha 26 or more is drawn.
const ALPHA_MIN: u8 = 26;

/// Which texels of a texture the model draws.
pub(crate) struct Mask {
    w: usize,
    h: usize,
    opaque: Vec<bool>,
}

impl Mask {
    /// The `w` x `h` texels at row `row0` of an RGBA image `stride` texels wide.
    pub fn from_rgba(rgba: &[u8], stride: usize, row0: usize, w: usize, h: usize) -> Mask {
        let opaque = (0..w * h)
            .map(|i| rgba[((row0 + i / w) * stride + i % w) * 4 + 3] >= ALPHA_MIN)
            .collect();
        Mask { w, h, opaque }
    }

    fn at(&self, x: usize, y: usize) -> bool {
        x < self.w && y < self.h && self.opaque[y * self.w + x]
    }
}

type V2 = [f32; 2];

fn lerp3(a: [f32; 3], b: [f32; 3], c: [f32; 3], s: f32, t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + s * (b[i] - a[i]) + t * (c[i] - a[i]))
}

/// Cuts each quad of `mesh` (four vertices, parallelogram, texture coordinates in `0..=1` of the
/// masked texture) into its opaque texels. Each corner's texture coordinate is `map` of its
/// original one; other vertex data is the first corner's. A quad without an area in texture space
/// (the edge of a sprite) is kept whole: its texel is opaque by construction.
pub(crate) fn split(mesh: &MeshData, mask: &Mask, map: &dyn Fn(V2) -> V2) -> MeshData {
    let mut out = MeshData::default();
    for tri in mesh.indices.as_chunks::<6>().0 {
        let base = tri[0] as usize;
        let q = &mesh.vertices[base..base + 4];
        for (corners, uv) in cells(q, mask) {
            let first = out.vertices.len() as u32;
            for (c, uv) in corners.into_iter().zip(uv) {
                out.vertices.push(Vertex {
                    pos: c,
                    uv: map(uv),
                    ..q[0]
                });
            }
            out.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
        }
    }
    out
}

/// The positions and texture coordinates of the opaque texel pieces of one quad.
fn cells(q: &[Vertex], mask: &Mask) -> Vec<([[f32; 3]; 4], [V2; 4])> {
    let uv: [V2; 4] = std::array::from_fn(|i| q[i].uv);
    let pos: [[f32; 3]; 4] = std::array::from_fn(|i| q[i].pos);
    let du = [uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]];
    let dv = [uv[3][0] - uv[0][0], uv[3][1] - uv[0][1]];
    let (uu, vv) = (du[0] * du[0] + du[1] * du[1], dv[0] * dv[0] + dv[1] * dv[1]);
    if uu == 0.0 || vv == 0.0 {
        return vec![(pos, uv)];
    }
    let bound = |axis: usize| {
        let lo = uv.iter().map(|c| c[axis]).fold(f32::MAX, f32::min);
        let hi = uv.iter().map(|c| c[axis]).fold(f32::MIN, f32::max);
        (lo, hi)
    };
    let ((ulo, uhi), (vlo, vhi)) = (bound(0), bound(1));
    let (w, h) = (mask.w as f32, mask.h as f32);
    // Texel coordinates are snapped to the grid up to float noise.
    let span = |lo: f32, hi: f32, n: f32| {
        (
            (lo * n + 1e-3).floor() as usize,
            (hi * n - 1e-3).ceil() as usize,
        )
    };
    let (i0, i1) = span(ulo, uhi, w);
    let (j0, j1) = span(vlo, vhi, h);
    let mut out = Vec::new();
    for j in j0..j1 {
        for i in i0..i1 {
            if !mask.at(i, j) {
                continue;
            }
            let (cu, cv) = (
                [ulo.max(i as f32 / w), uhi.min((i + 1) as f32 / w)],
                [vlo.max(j as f32 / h), vhi.min((j + 1) as f32 / h)],
            );
            // A corner of the piece lies on the high side of an axis when the quad's corner does.
            let pick = |k: usize, c: [f32; 2], axis: usize| {
                let (lo, hi) = bound(axis);
                c[usize::from(uv[k][axis] * 2.0 > lo + hi)]
            };
            let sub: [V2; 4] = std::array::from_fn(|k| [pick(k, cu, 0), pick(k, cv, 1)]);
            let corners = sub.map(|c| {
                let d = [c[0] - uv[0][0], c[1] - uv[0][1]];
                let s = (d[0] * du[0] + d[1] * du[1]) / uu;
                let t = (d[0] * dv[0] + d[1] * dv[1]) / vv;
                lerp3(pos[0], pos[1], pos[3], s, t)
            });
            out.push((corners, sub));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(uv: [V2; 4]) -> MeshData {
        let pos = [
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let vertices = (0..4)
            .map(|i| Vertex {
                pos: pos[i],
                uv: uv[i],
                color: [1.0; 4],
                light: [1.0; 2],
            })
            .collect();
        MeshData {
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    fn mask(rows: &[&str]) -> Mask {
        let (w, h) = (rows[0].len(), rows.len());
        let rgba = rows
            .iter()
            .flat_map(|r| {
                r.bytes()
                    .flat_map(|b| [0, 0, 0, if b == b'#' { 255 } else { 0 }])
            })
            .collect::<Vec<_>>();
        Mask::from_rgba(&rgba, w, 0, w, h)
    }

    #[test]
    fn only_opaque_texels_remain() {
        let m = mask(&["#.", ".#"]);
        let q = quad([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        let out = split(&q, &m, &|uv| uv);
        assert_eq!(out.vertices.len(), 8);
        // Texel (0, 0) is the top-left quarter: x in 0..0.5, y in 0.5..1.
        let first = &out.vertices[..4];
        assert_eq!(first[0].pos, [0.0, 1.0, 0.0]);
        assert_eq!(first[2].pos, [0.5, 0.5, 0.0]);
        assert_eq!(first[2].uv, [0.5, 0.5]);
    }

    #[test]
    fn mirrored_texture_coordinates_keep_their_corners() {
        let m = mask(&["#."]);
        // u runs from 1 to 0 along the quad: the left half of the quad shows texel 1, which is clear.
        let q = quad([[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
        let out = split(&q, &m, &|uv| uv);
        assert_eq!(out.vertices.len(), 4);
        assert_eq!(out.vertices[0].pos, [0.5, 1.0, 0.0]);
        assert_eq!(out.vertices[1].pos, [1.0, 1.0, 0.0]);
    }

    #[test]
    fn quad_without_texture_area_is_kept() {
        let m = mask(&["."]);
        let q = quad([[0.5, 0.5]; 4]);
        assert_eq!(split(&q, &m, &|uv| uv).vertices.len(), 4);
    }
}
