//! wgpu device, pipelines and frame rendering.

use crate::assets::{AssetSource, Atlas, Font};
use crate::mesh::{index, mesh_chunk, MeshData, Neighborhood, Vertex, HEIGHT};
use crate::text::{layout, HudVertex};
use crate::{Camera, ChunkBlocks, Hud};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use wgpu::util::DeviceExt;

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SKY: [f64; 3] = [0.47, 0.65, 1.0];
const OFFSCREEN: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    sky: [f32; 4],
    screen: [f32; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

#[derive(Default)]
struct ChunkMesh {
    opaque: Option<GpuMesh>,
    water: Option<GpuMesh>,
}

enum Target {
    Window {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    Offscreen {
        texture: wgpu::Texture,
        width: u32,
        height: u32,
    },
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: Target,
    depth: wgpu::TextureView,
    globals: wgpu::Buffer,
    world_group: wgpu::BindGroup,
    hud_group: wgpu::BindGroup,
    opaque_pipe: wgpu::RenderPipeline,
    water_pipe: wgpu::RenderPipeline,
    hud_pipe: wgpu::RenderPipeline,
    atlas: Atlas,
    font: Option<Font>,
    blocks: HashMap<(i32, i32), Vec<u16>>,
    meshes: HashMap<(i32, i32), ChunkMesh>,
}

impl Renderer {
    /// Window renderer. `assets` is the 1.8.9 client jar or an extracted assets directory; `None` draws flat colors and no text.
    pub fn new(window: Arc<winit::window::Window>, assets: Option<&Path>) -> Renderer {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).expect("create surface");
        let (adapter, device, queue) =
            pollster::block_on(request_device(&instance, Some(&surface)));
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface supported");
        // Colors are authored in display space like vanilla, so avoid an sRGB surface.
        config.format = config.format.remove_srgb_suffix();
        config.view_formats.clear();
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let format = config.format;
        Self::build(
            device,
            queue,
            Target::Window { surface, config },
            format,
            assets,
        )
    }

    /// Headless renderer that draws into an RGBA texture (tests, screenshots).
    pub fn new_offscreen(width: u32, height: u32, assets: Option<&Path>) -> Renderer {
        let instance = wgpu::Instance::default();
        let (_, device, queue) = pollster::block_on(request_device(&instance, None));
        let texture = offscreen_texture(&device, width, height);
        Self::build(
            device,
            queue,
            Target::Offscreen {
                texture,
                width,
                height,
            },
            OFFSCREEN,
            assets,
        )
    }

    fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        target: Target,
        format: wgpu::TextureFormat,
        assets: Option<&Path>,
    ) -> Renderer {
        let mut source = assets.and_then(AssetSource::open);
        let atlas = source.as_mut().map_or_else(Atlas::flat, Atlas::from_assets);
        let font = source.as_mut().and_then(Font::load);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas_view = upload_texture(&device, &queue, atlas.width, atlas.height, &atlas.rgba);
        let (fw, fh, frgba) = font_texture(font.as_ref());
        let font_view = upload_texture(&device, &queue, fw, fh, &frgba);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout_bg = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                bgl_entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                bgl_entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout_bg,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: globals.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        let world_group = group(&atlas_view);
        let hud_group = group(&font_view);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout_bg],
            push_constant_ranges: &[],
        });
        let world_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
        let hud_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let pipe = |vs: &str,
                    stride: usize,
                    attrs: &[wgpu::VertexAttribute],
                    blend: bool,
                    depth_write: bool,
                    depth: bool,
                    cull: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: stride as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: attrs,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_world"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull.then_some(wgpu::Face::Back),
                    front_face: wgpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: depth_write,
                    depth_compare: if depth {
                        wgpu::CompareFunction::Less
                    } else {
                        wgpu::CompareFunction::Always
                    },
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let opaque_pipe = pipe(
            "vs_world",
            std::mem::size_of::<Vertex>(),
            &world_attrs,
            false,
            true,
            true,
            true,
        );
        let water_pipe = pipe(
            "vs_world",
            std::mem::size_of::<Vertex>(),
            &world_attrs,
            true,
            false,
            true,
            false,
        );
        let hud_pipe = pipe(
            "vs_hud",
            std::mem::size_of::<HudVertex>(),
            &hud_attrs,
            true,
            false,
            false,
            false,
        );

        let (w, h) = target_size(&target);
        let depth = depth_view(&device, w, h);
        Renderer {
            device,
            queue,
            target,
            depth,
            globals,
            world_group,
            hud_group,
            opaque_pipe,
            water_pipe,
            hud_pipe,
            atlas,
            font,
            blocks: HashMap::new(),
            meshes: HashMap::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        match &mut self.target {
            Target::Window { surface, config } => {
                config.width = w;
                config.height = h;
                surface.configure(&self.device, config);
            }
            Target::Offscreen {
                texture,
                width,
                height,
            } => {
                *texture = offscreen_texture(&self.device, w, h);
                (*width, *height) = (w, h);
            }
        }
        self.depth = depth_view(&self.device, w, h);
    }

    pub fn upload_chunk(&mut self, chunk: &impl ChunkBlocks) {
        let mut data = vec![0u16; 16 * 16 * HEIGHT];
        for y in 0..HEIGHT {
            for z in 0..16 {
                for x in 0..16 {
                    data[index(x, y, z)] = chunk.block(x, y, z);
                }
            }
        }
        let key = (chunk.chunk_x(), chunk.chunk_z());
        self.blocks.insert(key, data);
        self.remesh_around(key);
    }

    pub fn remove_chunk(&mut self, x: i32, z: i32) {
        self.blocks.remove(&(x, z));
        self.meshes.remove(&(x, z));
        self.remesh_neighbors((x, z));
    }

    /// Apply a single block change at world coordinates.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state: u16) {
        let key = (x.div_euclid(16), z.div_euclid(16));
        let Some(data) = self.blocks.get_mut(&key) else {
            return;
        };
        if !(0..HEIGHT as i32).contains(&y) {
            return;
        }
        data[index(
            x.rem_euclid(16) as usize,
            y as usize,
            z.rem_euclid(16) as usize,
        )] = state;
        self.remesh_around(key);
    }

    fn remesh_around(&mut self, key: (i32, i32)) {
        self.remesh(key);
        self.remesh_neighbors(key);
    }

    fn remesh_neighbors(&mut self, (cx, cz): (i32, i32)) {
        for k in [(cx - 1, cz), (cx + 1, cz), (cx, cz - 1), (cx, cz + 1)] {
            self.remesh(k);
        }
    }

    fn remesh(&mut self, (cx, cz): (i32, i32)) {
        let Some(center) = self.blocks.get(&(cx, cz)) else {
            return;
        };
        let get = |k| self.blocks.get(&k).map(Vec::as_slice);
        let hood = Neighborhood {
            center,
            neighbors: [
                get((cx - 1, cz)),
                get((cx + 1, cz)),
                get((cx, cz - 1)),
                get((cx, cz + 1)),
            ],
        };
        let m = mesh_chunk(&hood, &self.atlas);
        let origin = [cx as f32 * 16.0, 0.0, cz as f32 * 16.0];
        let mesh = ChunkMesh {
            opaque: self.upload_mesh(m.opaque, origin),
            water: self.upload_mesh(m.water, origin),
        };
        self.meshes.insert((cx, cz), mesh);
    }

    fn upload_mesh(&self, mut data: MeshData, origin: [f32; 3]) -> Option<GpuMesh> {
        if data.is_empty() {
            return None;
        }
        for v in &mut data.vertices {
            for (p, o) in v.pos.iter_mut().zip(origin) {
                *p += o;
            }
        }
        let buf = |contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents,
                    usage,
                })
        };
        Some(GpuMesh {
            vertices: buf(
                bytemuck::cast_slice(&data.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buf(
                bytemuck::cast_slice(&data.indices),
                wgpu::BufferUsages::INDEX,
            ),
            count: data.indices.len() as u32,
        })
    }

    /// Draw one frame to the window. Offscreen renderers draw into their texture.
    pub fn render(&mut self, cam: &Camera, hud: &Hud) {
        let frame = match &self.target {
            Target::Window { surface, .. } => match surface.get_current_texture() {
                Ok(f) => Some(f),
                Err(_) => {
                    self.resize(self.size().0, self.size().1);
                    return;
                }
            },
            Target::Offscreen { .. } => None,
        };
        let view = match (&frame, &self.target) {
            (Some(f), _) => f.texture.create_view(&Default::default()),
            (None, Target::Offscreen { texture, .. }) => texture.create_view(&Default::default()),
            _ => return,
        };
        self.draw(&view, cam, hud);
        if let Some(f) = frame {
            f.present();
        }
    }

    fn size(&self) -> (u32, u32) {
        target_size(&self.target)
    }

    fn draw(&mut self, view: &wgpu::TextureView, cam: &Camera, hud: &Hud) {
        let (w, h) = self.size();
        let globals = Globals {
            view_proj: view_proj(cam, w as f32 / h as f32),
            sky: [0.0; 4],
            screen: [w as f32, h as f32, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let verts = layout(hud, self.font.as_ref(), w, h);
        let hud_buf = (!verts.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });

        // Chunk positions are baked in world space as f32; chunks come from the camera's own area, so precision holds.
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: SKY[0],
                            g: SKY[1],
                            b: SKY[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.world_group, &[]);
            pass.set_pipeline(&self.opaque_pipe);
            for m in self.meshes.values().filter_map(|m| m.opaque.as_ref()) {
                draw_mesh(&mut pass, m);
            }
            pass.set_pipeline(&self.water_pipe);
            for m in self.meshes.values().filter_map(|m| m.water.as_ref()) {
                draw_mesh(&mut pass, m);
            }
            if let Some(buf) = &hud_buf {
                pass.set_bind_group(0, &self.hud_group, &[]);
                pass.set_pipeline(&self.hud_pipe);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..verts.len() as u32, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);
    }

    /// Render one frame offscreen and return `(width, height, rgba8)`. Only valid for offscreen renderers.
    pub fn render_to_rgba(&mut self, cam: &Camera, hud: &Hud) -> (u32, u32, Vec<u8>) {
        self.render(cam, hud);
        let Target::Offscreen {
            texture,
            width,
            height,
        } = &self.target
        else {
            panic!("render_to_rgba needs an offscreen renderer")
        };
        let (w, h) = (*width, *height);
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device.poll(wgpu::Maintain::Wait);
        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            out.extend_from_slice(&mapped[y * row as usize..y * row as usize + (w * 4) as usize]);
        }
        (w, h, out)
    }
}

fn draw_mesh<'a>(pass: &mut wgpu::RenderPass<'a>, m: &'a GpuMesh) {
    pass.set_vertex_buffer(0, m.vertices.slice(..));
    pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..m.count, 0, 0..1);
}

async fn request_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: surface,
            ..Default::default()
        })
        .await
        .expect("no wgpu adapter found");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .expect("request wgpu device");
    (adapter, device, queue)
}

fn bgl_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty,
        count: None,
    }
}

fn target_size(t: &Target) -> (u32, u32) {
    match t {
        Target::Window { config, .. } => (config.width, config.height),
        Target::Offscreen { width, height, .. } => (*width, *height),
    }
}

fn offscreen_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let tex = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    );
    tex.create_view(&Default::default())
}

/// Font image with one opaque white row appended (solid rectangles sample it); 128x129 transparent when there is no font.
fn font_texture(font: Option<&Font>) -> (u32, u32, Vec<u8>) {
    let (side, mut rgba) = font.map_or((128, vec![0u8; 128 * 128 * 4]), |f| {
        (f.image.width, f.image.rgba.clone())
    });
    rgba.extend(std::iter::repeat_n(255u8, side * 4));
    (side as u32, side as u32 + 1, rgba)
}

/// Column-major view-projection (OpenGL-style camera math, wgpu 0..1 depth).
fn view_proj(cam: &Camera, aspect: f32) -> [[f32; 4]; 4] {
    let (yaw, pitch) = (cam.yaw.to_radians(), cam.pitch.to_radians());
    let fwd = [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ];
    let right = norm(cross(fwd, [0.0, 1.0, 0.0]));
    let up = cross(right, fwd);
    let eye = [cam.pos[0] as f32, cam.pos[1] as f32, cam.pos[2] as f32];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let view = [
        [right[0], up[0], -fwd[0], 0.0],
        [right[1], up[1], -fwd[1], 0.0],
        [right[2], up[2], -fwd[2], 0.0],
        [-dot(right, eye), -dot(up, eye), dot(fwd, eye), 1.0],
    ];
    let (near, far) = (0.05f32, 1024.0f32);
    let f = 1.0 / (cam.fov_deg.to_radians() / 2.0).tan();
    let proj = [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ];
    mul(proj, view)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}
