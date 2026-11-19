//! `EntityRenderer.drawDebugAxes`: while F3 shows the full debug screen the client draws three world axes
//! 0.1 in front of the camera, after the hand on a cleared depth buffer, in place of the crosshair. The view is
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
}

impl Axes {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
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
        }
    }

    /// The view-projection of the frame's camera orientation.
    pub fn write(&self, queue: &wgpu::Queue, cam: &Camera, aspect: f32) {
        let mut globals = <Globals as bytemuck::Zeroable>::zeroed();
        globals.view_proj = view_proj(&matrix_camera(cam), aspect);
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        line_pipe: &'a wgpu::RenderPipeline,
    ) {
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(line_pipe);
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

/// The 12 edges of each box as line vertices.
fn lines() -> Vec<Vertex> {
    let boxes = [
        ([0.005, WIDTH, WIDTH], RED),
        ([WIDTH, WIDTH, 0.005], BLUE),
        ([WIDTH, 0.0033, WIDTH], GREEN),
    ];
    let mut v = Vec::with_capacity(VERTICES as usize);
    for (size, color) in boxes {
        let corner = |i: usize| [0, 1, 2].map(|a| size[a] * ((i >> a) & 1) as f32);
        for a in 0..8usize {
            for axis in 0..3 {
                if a & (1 << axis) == 0 {
                    v.extend([a, a | 1 << axis].map(|i| Vertex {
                        pos: corner(i),
                        uv: [0.0; 2],
                        color,
                        light: [1.0; 2],
                    }));
                }
            }
        }
    }
    v
}
