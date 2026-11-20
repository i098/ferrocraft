//! `WorldRenderer.sortVertexData`: the translucent layer draws its quads far to near, so blending stacks the
//! faces the way vanilla does.

use crate::mesh::MeshData;

/// The quads of one chunk's blended mesh: six indices each, with the quad centre in chunk coordinates.
pub(crate) struct QuadOrder {
    centers: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl QuadOrder {
    pub fn new(mesh: &MeshData) -> Self {
        let centers = mesh
            .indices
            .as_chunks::<6>()
            .0
            .iter()
            .map(|q| {
                let base = q.iter().copied().min().unwrap() as usize;
                let mut c = [0.0f32; 3];
                for v in &mesh.vertices[base..base + 4] {
                    c.iter_mut().zip(v.pos).for_each(|(c, p)| *c += p * 0.25);
                }
                c
            })
            .collect();
        Self {
            centers,
            indices: mesh.indices.clone(),
        }
    }

    /// The indices ordered by decreasing distance of the quad centre from `cam` (chunk coordinates).
    pub fn far_to_near(&self, cam: [f32; 3]) -> Vec<u32> {
        let dist = |c: &[f32; 3]| (0..3).map(|i| (cam[i] - c[i]).powi(2)).sum::<f32>();
        let mut order: Vec<usize> = (0..self.centers.len()).collect();
        order.sort_by(|&a, &b| dist(&self.centers[b]).total_cmp(&dist(&self.centers[a])));
        order
            .into_iter()
            .flat_map(|q| self.indices[q * 6..q * 6 + 6].iter().copied())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Vertex;

    #[test]
    fn nearest_quad_is_drawn_last() {
        let v = |x: f32| Vertex {
            pos: [x, 0.0, 0.0],
            uv: [0.0; 2],
            color: [1.0; 4],
            light: [0.0; 2],
        };
        let mut m = MeshData::default();
        for x in [1.0, 9.0, 5.0] {
            let base = m.vertices.len() as u32;
            m.vertices.extend([v(x); 4]);
            m.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        }
        let order = QuadOrder::new(&m).far_to_near([0.0; 3]);
        assert_eq!(order[0], 4, "the quad at x 9 comes first");
        assert_eq!(order[12], 0, "the quad at x 1 comes last");
    }
}
