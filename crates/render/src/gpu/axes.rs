//! `EntityRenderer.drawDebugAxes`: while F3 shows the full debug screen the client draws three world axes
//! 0.1 in front of the camera, after the hand and ignoring its depth (vanilla clears it), in place of the crosshair. The view is
//! `translate(0, 0, -0.1)`, the pitch, the yaw and the hand projection: no bobbing, no hurt tilt. Each axis is a box
//! outline of 1 px lines: x red 0.005 long, z blue 0.005 long, y green 0.0033 long, each 0.0001 wide.

use super::*;

/// Edges of three boxes, two vertices each.
pub(super) const VERTICES: u32 = 3 * 12 * 2;
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const WIDTH: f32 = 1.0e-4;

/// The axes' uniforms and vertices; drawn only while `on`.
pub(super) struct Axes {
    pub on: bool,
    globals: wgpu::Buffer,
    group: wgpu::BindGroup,
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
        let lines = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("axes lines"),
            contents: bytemuck::cast_slice(&lines()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Axes {
            on: false,
            globals,
            group,
            lines,
            pipe,
        }
    }

    /// The view-projection of the frame's camera orientation.
    pub fn write(&self, queue: &wgpu::Queue, cam: &Camera, aspect: f32) {
        if !self.on {
            return;
        }
        let mut globals = <Globals as bytemuck::Zeroable>::zeroed();
        globals.view_proj = view_proj(&matrix_camera(cam), aspect);
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
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
