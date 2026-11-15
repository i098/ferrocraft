//! `WorldRenderer.entityOutline` and `entityOutlineRenderTarget`: the spectator outline pass. While
//! the spectating player holds `key.spectatorOutlines`, the silhouettes of the players draw into the
//! `final` target of `shaders/post/entity_outline.json` with no depth test, the chain (Sobel edge,
//! two blurs, blit) runs, and `renderEntityOutlines` blends the target over the finished world.

use super::*;

const CHAIN: &str = "shaders/post/entity_outline.json";
const FINAL: &str = "final";

/// `RenderTarget.draw` of the outline target: one full-screen triangle that reads the texel.
const COMPOSITE: &str = "
@group(0) @binding(0) var outline: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(outline, vec2<i32>(at.xy), 0);
}
";

/// `loadEntityOutline`: the chain loads once; a failed load leaves the field null for good.
pub(super) enum Slot {
    Idle,
    Failed,
    Ready(Box<Outline>),
}

pub(super) struct Outline {
    post: Post,
    silhouette: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    /// The silhouettes of this frame; `None` while the outlines are off.
    mesh: Option<GpuMesh>,
}

impl Outline {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &mut AssetSource,
        layout_bg: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Result<Outline, String> {
        let post = Post::load(device, queue, source, CHAIN, format, DEPTH, size)?;
        let world = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2];
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[layout_bg],
            push_constant_ranges: &[],
        });
        // `depthFunc(GL_ALWAYS)` and no depth buffer: a later entity overwrites an earlier one.
        let silhouette = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&world_layout),
            vertex: wgpu::VertexState {
                module: &world,
                entry_point: Some("vs_world"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &world,
                entry_point: Some("fs_flat"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let screen = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(COMPOSITE.into()),
        });
        let screen_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        // `blendFuncSeparate(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, ZERO, ONE)`: the world keeps its alpha.
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&screen_layout),
            vertex: wgpu::VertexState {
                module: &screen,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &screen,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            // The overlay pass has the renderer's depth buffer attached.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let group = Self::group(device, &layout, &post)?;
        Ok(Outline {
            post,
            silhouette,
            composite,
            layout,
            group,
            mesh: None,
        })
    }

    fn group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        post: &Post,
    ) -> Result<wgpu::BindGroup, String> {
        let view = post
            .target(FINAL)
            .ok_or_else(|| format!("{CHAIN} has no target '{FINAL}'"))?;
        Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        }))
    }
}

impl Renderer {
    /// `WorldRenderer.shouldRenderEntityOutlines` is the caller's: the spectating player holds
    /// `key.spectatorOutlines`. `Some(draws)` outlines the players among them for the next frames;
    /// `None` (the key up, or not a spectator) draws the frame as without the pass.
    pub fn set_entity_outlines(&mut self, draws: Option<&[EntityDraw]>) {
        let Some(draws) = draws else {
            if let Slot::Ready(o) = &mut self.outline {
                o.mesh = None;
            }
            return;
        };
        if matches!(self.outline, Slot::Idle) {
            self.outline = self.load_outline();
        }
        let mesh = entity::outline::build(draws, &self.entity_skins);
        let mesh = self.upload_mesh(mesh, [0.0; 3]);
        if let Slot::Ready(o) = &mut self.outline {
            o.mesh = mesh;
        }
    }

    fn load_outline(&mut self) -> Slot {
        let format = self.color_format();
        let size = self.size();
        let Some(source) = &mut self.source else {
            return Slot::Failed;
        };
        let loaded = Outline::new(
            &self.device,
            &self.queue,
            source,
            &self.layout_bg,
            format,
            size,
        );
        match loaded {
            Ok(o) => Slot::Ready(Box::new(o)),
            Err(e) => {
                eprintln!("ferrocraft-render: Failed to load shader: {CHAIN}: {e}");
                Slot::Failed
            }
        }
    }

    /// The outline pass of a frame that has silhouettes.
    fn active_outline(&self) -> Option<&Outline> {
        match &self.outline {
            Slot::Ready(o) if o.mesh.is_some() => Some(o),
            _ => None,
        }
    }

    /// `renderEntities` with `shouldRenderEntityOutlines`: clear the target, draw the silhouettes, run the chain.
    pub(super) fn record_outlines(&self, enc: &mut wgpu::CommandEncoder) {
        let Some(o) = self.active_outline() else {
            return;
        };
        let (Some(mesh), Some(target)) = (&o.mesh, o.post.target(FINAL)) else {
            return;
        };
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&o.silhouette);
            pass.set_bind_group(0, &self.world_group, &[]);
            draw_mesh(&mut pass, mesh);
        }
        o.post.process(&self.queue, enc);
    }

    /// `renderEntityOutlines`: the finished outlines over the world, after the hand and before any effect.
    pub(super) fn composite_outlines<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if let Some(o) = self.active_outline() {
            pass.set_pipeline(&o.composite);
            pass.set_bind_group(0, &o.group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// `onResolutionChanged`: the chain targets take the new size.
    pub(super) fn resize_outline(&mut self, w: u32, h: u32) {
        if let Slot::Ready(o) = &mut self.outline {
            o.post.resize(&self.device, &self.queue, w, h);
            match Outline::group(&self.device, &o.layout, &o.post) {
                Ok(group) => o.group = group,
                Err(_) => self.outline = Slot::Failed,
            }
        }
    }

    /// The color format of the window or of the offscreen texture.
    pub(super) fn color_format(&self) -> wgpu::TextureFormat {
        match &self.target {
            Target::Window { config, .. } => config.format,
            Target::Offscreen { .. } => OFFSCREEN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Class, TeamTag};
    use crate::Look;

    fn player(x: f64, outline: Option<u32>) -> EntityDraw {
        EntityDraw {
            class: Class::Player { slim: false },
            pos: [x, 64.0, 12.0],
            yaw: 180.0,
            head_yaw: 180.0,
            pitch: 0.0,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            swing: 0.0,
            age: 0.0,
            sneaking: false,
            hurt: false,
            death_time: 0.0,
            invisible: false,
            size: (0.6, 1.8),
            held: 0,
            armor: [0; 4],
            armor_dye: [None; 4],
            armor_glint: [false; 4],
            held_glint: false,
            held_damage: 0,
            held_dye: None,
            stack: None,
            hover_start: 0.0,
            hang: Default::default(),
            name: None,
            skin: None,
            burning: false,
            eye_height: 1.62,
            always_name: false,
            team_tag: TeamTag {
                outline,
                ..Default::default()
            },
            pointed: false,
            ridden: false,
            child: false,
            using: None,
            view: [0.0; 2],
            line: None,
            look: Look::default(),
        }
    }

    /// Pixels of `frame` that differ from `base`, as `(x, rgb)`.
    fn changed(base: &[u8], frame: &[u8], w: usize) -> Vec<(usize, [u8; 3])> {
        base.chunks(4)
            .zip(frame.chunks(4))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (_, b))| (i % w, [b[0], b[1], b[2]]))
            .collect()
    }

    /// Acceptance of the issue: with the key held two players get an outline in their team colors; with
    /// the key up the frame is the plain one. The chain is the real `entity_outline.json` of the jar.
    #[test]
    fn key_held_outlines_two_players_and_key_up_changes_nothing() {
        let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR") else {
            eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
            return;
        };
        let (w, h) = (400usize, 240u32);
        let mut r = Renderer::new_offscreen(w as u32, h, Some(Path::new(&jar)));
        let cam = Camera {
            pos: [8.0, 65.6, 4.0],
            yaw: 0.0,
            pitch: 0.0,
            fov_deg: 70.0,
            ..Camera::default()
        };
        let draws = [player(9.5, Some(0xFF_55_55)), player(6.5, None)];
        r.set_entities(&draws);
        let plain = r.render_to_rgba(&cam, &[]).2;

        r.set_entity_outlines(Some(&draws));
        let outlined = r.render_to_rgba(&cam, &[]).2;
        if let Some(dir) = std::env::var_os("OUTLINE_PNG_DIR") {
            let dir = Path::new(&dir);
            crate::assets::encode_png(&dir.join("before.png"), w as u32, h, &plain).unwrap();
            crate::assets::encode_png(&dir.join("after.png"), w as u32, h, &outlined).unwrap();
        }
        let diff = changed(&plain, &outlined, w);
        assert!(!diff.is_empty(), "the key held draws outlines");
        let half = |left: bool| diff.iter().filter(move |(x, _)| (*x < w / 2) == left);
        let red = half(true).filter(|(_, [r, g, b])| r > g && r > b).count();
        let white = half(false)
            .filter(|(_, [r, g, b])| r == g && g == b)
            .count();
        assert!(
            red > 20,
            "the red team's player has a red outline ({red} px)"
        );
        assert!(
            white > 20,
            "the teamless player has a white outline ({white} px)"
        );

        r.set_entity_outlines(None);
        assert_eq!(
            r.render_to_rgba(&cam, &[]).2,
            plain,
            "the key up leaves the frame as it was"
        );
    }
}
