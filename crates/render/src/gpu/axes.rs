//! `EntityRenderer.drawDebugAxes`: while F3 shows the full debug screen the client draws three world axes
//! 0.1 in front of the camera, after the hand and ignoring its depth (vanilla clears it), in place of the crosshair. The view is
//! `translate(0, 0, -0.1)`, the pitch, the yaw and the hand projection: no bobbing, no hurt tilt. Each axis is a box
//! outline of 2 px lines (`glLineWidth(2.0)`, drawn here as two 1 px lines because wgpu has no line width): x red 0.005 long, z blue 0.005 long, y green 0.0033 long, each 0.0001 wide.

use super::*;

/// Edges of three boxes, two vertices each.
const EDGE_VERTICES: usize = 3 * 12 * 2;
/// Every edge is drawn twice, one pixel apart.
pub(super) const VERTICES: u32 = EDGE_VERTICES as u32 * 2;
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const WIDTH: f32 = 1.0e-4;

/// The axes' uniforms and vertices; drawn only while `on`.
pub(super) struct Axes {
    pub on: bool,
    globals: wgpu::Buffer,
    group: wgpu::BindGroup,
    base: Vec<Vertex>,
    lines: wgpu::Buffer,
    pipe: wgpu::RenderPipeline,
}

impl Axes {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
        pipe: wgpu::RenderPipeline,
    ) -> Self {
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("axes globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = bind_group(device, layout, &globals, sampler, view);
        let base = lines();
        let lines = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("axes lines"),
            size: (VERTICES as usize * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Axes {
            on: false,
            globals,
            group,
            base,
            lines,
            pipe,
        }
    }

    /// The lines projected on the CPU into clip space (the shader then uses an identity matrix) and widened to 2 px.
    pub fn write(&self, queue: &wgpu::Queue, cam: &Camera, size: (u32, u32)) {
        if !self.on {
            return;
        }
        let mut globals = <Globals as bytemuck::Zeroable>::zeroed();
        globals.view_proj = IDENTITY;
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        let m = view_proj(&matrix_camera(cam), size.0 as f32 / size.1 as f32);
        let out = widen(&self.base, &m, size);
        queue.write_buffer(&self.lines, 0, bytemuck::cast_slice(&out));
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if !self.on {
            return;
        }
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(&self.pipe);
        pass.set_vertex_buffer(0, self.lines.slice(..));
        pass.draw(0..VERTICES, 0..1);
    }
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Each line of `base` projected by the column-major `m`, then twice: GL covers the pixels whose centres lie within
/// 1 px of the line across its minor axis, so the second copy sits one pixel toward the nearer side. Lines behind the
/// camera are collapsed to a point.
fn widen(base: &[Vertex], m: &[[f32; 4]; 4], (w, h): (u32, u32)) -> Vec<Vertex> {
    let (w, h) = (w as f32, h as f32);
    let clip = |v: &Vertex| {
        let c = |r: usize| (0..3).map(|k| m[k][r] * v.pos[k]).sum::<f32>() + m[3][r];
        [c(0), c(1), c(2), c(3)]
    };
    let behind = Vertex {
        pos: [0.0, 0.0, 2.0],
        ..base[0]
    };
    let mut out = Vec::with_capacity(base.len() * 2);
    let mut shifted = Vec::with_capacity(base.len());
    for pair in base.as_chunks::<2>().0 {
        let (a, b) = (clip(&pair[0]), clip(&pair[1]));
        if a[3] <= 1.0e-6 || b[3] <= 1.0e-6 {
            out.extend([behind; 2]);
            shifted.extend([behind; 2]);
            continue;
        }
        let ndc = |c: [f32; 4]| [c[0] / c[3], c[1] / c[3], c[2] / c[3]];
        let (na, nb) = (ndc(a), ndc(b));
        let x_major = ((nb[0] - na[0]) * w).abs() >= ((nb[1] - na[1]) * h).abs();
        let centre = if x_major {
            (na[1] + nb[1]) * h
        } else {
            (na[0] + nb[0]) * w
        } / 4.0;
        let sign = if centre - centre.floor() <= 0.5 {
            -1.0
        } else {
            1.0
        };
        for (v, n) in pair.iter().zip([na, nb]) {
            out.push(Vertex { pos: n, ..*v });
            let pos = if x_major {
                [n[0], n[1] + sign * 2.0 / h, n[2]]
            } else {
                [n[0] + sign * 2.0 / w, n[1], n[2]]
            };
            shifted.push(Vertex { pos, ..*v });
        }
    }
    out.append(&mut shifted);
    out
}

/// A camera at the origin that looks along the player's view with the hand's field of view and nothing else
/// applied; the 0.1 offset along the view moves the axes in front of it (`translate(0, 0, -0.1)`).
fn matrix_camera(cam: &Camera) -> Camera {
    let fwd = look_vector(cam);
    Camera {
        pos: fwd.map(|c| f64::from(-0.1 * c)),
        yaw: cam.yaw,
        pitch: cam.pitch,
        fov_deg: cam.hand_fov_deg,
        ..Camera::default()
    }
}

/// The 12 edges of a unit cube as corner index pairs: corner `i` has bit `a` set when it sits at the far end of axis `a`.
fn cube_edges() -> impl Iterator<Item = (usize, usize)> {
    (0..8usize)
        .flat_map(|c| (0..3).map(move |axis| (c, axis)))
        .filter(|&(c, axis)| c & (1 << axis) == 0)
        .map(|(c, axis)| (c, c | 1 << axis))
}

/// The 12 edges of one box of `size` as line vertices.
fn box_lines(size: [f32; 3], color: [f32; 4]) -> impl Iterator<Item = Vertex> {
    let corner = move |i: usize| [0, 1, 2].map(|a| size[a] * ((i >> a) & 1) as f32);
    cube_edges().flat_map(|(a, b)| [a, b]).map(move |i| Vertex {
        pos: corner(i),
        uv: [0.0; 2],
        color,
        light: [1.0; 2],
    })
}

/// The three boxes as line vertices.
fn lines() -> Vec<Vertex> {
    [
        ([0.005, WIDTH, WIDTH], RED),
        ([WIDTH, WIDTH, 0.005], BLUE),
        ([WIDTH, 0.0033, WIDTH], GREEN),
    ]
    .into_iter()
    .flat_map(|(size, color)| box_lines(size, color))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_is_drawn_twice_one_pixel_apart() {
        let v = |x: f32, y: f32| Vertex {
            pos: [x, y, 0.5],
            uv: [0.0; 2],
            color: [1.0; 4],
            light: [1.0; 2],
        };
        // Identity matrix: a horizontal line at the pixel boundary y = 0 on a 100 x 100 target.
        let out = widen(&[v(-0.1, 0.0), v(0.1, 0.0)], &IDENTITY, (100, 100));
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].pos[1], 0.0);
        assert!(((out[2].pos[1] - out[0].pos[1]).abs() - 0.02).abs() < 1.0e-6);
        assert_eq!(out[2].pos[0], out[0].pos[0]);
    }
}
