//! `PostChain`, `PostPass` and `Effect` (MCP `ShaderGroup`, `Shader`, `ShaderManager`): a
//! `shaders/post/*.json` effect runs its passes over the world and hand before the GUI is drawn.

use crate::assets::{decode_png, AssetSource};
use crate::glsl;
use serde_json::Value;
use std::borrow::Cow;

const MAIN: &str = "minecraft:main";

/// `GameRenderer.SHADERS`: the "Super Secret Settings..." cycle, in order.
pub const SHADERS: [&str; 24] = [
    "notch",
    "fxaa",
    "art",
    "bumpy",
    "blobs2",
    "pencil",
    "color_convolve",
    "deconverge",
    "flip",
    "invert",
    "ntsc",
    "outline",
    "phosphor",
    "scan_pincushion",
    "sobel",
    "bits",
    "desaturate",
    "green",
    "blur",
    "wobble",
    "blobs",
    "antialias",
    "creeper",
    "spider",
];

/// One loaded effect and its render targets; target 0 is `minecraft:main`, where the world draws.
pub(crate) struct Post {
    format: wgpu::TextureFormat,
    /// Target names, `minecraft:main` first.
    names: Vec<String>,
    targets: Vec<wgpu::TextureView>,
    /// `textures/effect/*.png` read by aux samplers.
    textures: Vec<(wgpu::TextureView, wgpu::Sampler)>,
    passes: Vec<Pass>,
    /// The pass quad in pixels: `Position` of the vertex stage.
    quad: wgpu::Buffer,
    nearest: wgpu::Sampler,
    blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    blit_group: Option<wgpu::BindGroup>,
    size: (u32, u32),
    /// `PostChain.time` in ticks, wrapped at 20, and the last partial tick it saw.
    time: f32,
    last_stamp: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Read {
    Target(usize),
    Texture(usize),
}

/// A member of the uniform block.
struct Slot {
    name: String,
    offset: usize,
    /// Float count, or 16 for a `mat4`.
    floats: usize,
    /// In the program JSON: only those get values from JSON and from `PostPass.process`.
    listed: bool,
}

struct Pass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    output: usize,
    /// The program's samplers, in binding order, and what each reads.
    samplers: Vec<String>,
    reads: Vec<Read>,
    /// `AuxSize<i>` values.
    aux_sizes: Vec<[f32; 2]>,
    slots: Vec<Slot>,
    values: Vec<u8>,
    buffer: Option<wgpu::Buffer>,
    group: Option<wgpu::BindGroup>,
}

impl Pass {
    fn set(&mut self, name: &str, values: &[f32]) {
        put(&self.slots, &mut self.values, name, values);
    }
}

/// `Uniform.set`: writes `values` into the block bytes of uniform `name`, if the block has it.
fn put(slots: &[Slot], bytes: &mut [u8], name: &str, values: &[f32]) {
    let Some(s) = slots.iter().find(|s| s.name == name) else {
        return;
    };
    let mut put = |i: usize, v: f32| {
        let at = s.offset + 4 * i;
        if let Some(b) = bytes.get_mut(at..at + 4) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    };
    if s.floats == 16 {
        // Vanilla uploads matrices transposed: JSON and `Matrix4f` values are rows.
        for (i, &v) in values.iter().take(16).enumerate() {
            put((i % 4) * 4 + i / 4, v);
        }
    } else {
        for (i, &v) in values.iter().take(s.floats).enumerate() {
            put(i, v);
        }
    }
}

impl Post {
    /// `new PostChain(..)` and `resize`: `name` is a path such as `shaders/post/notch.json`.
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &mut AssetSource,
        name: &str,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Result<Post, String> {
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let built = Self::build(device, queue, source, name, format, depth, size);
        let error = pollster::block_on(device.pop_error_scope());
        let mut post = built?;
        if let Some(e) = error {
            return Err(e.to_string());
        }
        post.resize(device, queue, size.0, size.1);
        Ok(post)
    }

    fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &mut AssetSource,
        name: &str,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Result<Post, String> {
        let chain = read_json(source, &format!("assets/minecraft/{name}"))?;
        let mut names = vec![MAIN.to_owned()];
        for t in array(&chain, "targets") {
            let n = t.as_str().or_else(|| t.get("name")?.as_str());
            names.push(n.ok_or("target without a name")?.to_owned());
        }
        let mut textures = Vec::new();
        let mut passes = Vec::new();
        for p in array(&chain, "passes") {
            let target = |key: &str| {
                let n = p.get(key).and_then(Value::as_str).unwrap_or_default();
                names
                    .iter()
                    .position(|t| t == n)
                    .ok_or(format!("{key} '{n}' does not exist"))
            };
            let (input, output) = (target("intarget")?, target("outtarget")?);
            let program = p.get("name").and_then(Value::as_str).unwrap_or_default();
            let mut pass = load_program(device, source, program, format, input, output)?;
            let mut aux = vec![("DiffuseSampler".to_owned(), Read::Target(input))];
            for a in array(p, "auxtargets") {
                let (name, read, aux_size) =
                    aux_target((device, queue), source, a, &names, size, &mut textures)?;
                pass.aux_sizes.push(aux_size);
                aux.push((name, read));
            }
            pass.bind_reads(&aux, output, program)?;
            pass.set_uniforms(p)?;
            passes.push(pass);
        }
        let blit = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let (blit, blit_layout) = pipeline(
            device,
            &blit,
            &blit,
            &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: texture_binding(),
                count: None,
            }],
            format,
            None,
            Some(depth),
        );
        Ok(Post {
            format,
            names,
            targets: Vec::new(),
            textures,
            passes,
            quad: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            nearest: device.create_sampler(&Default::default()),
            blit,
            blit_layout,
            blit_group: None,
            size,
            time: 0.0,
            last_stamp: 0.0,
        })
    }
}

/// One `auxtargets` entry: a chain target, or else `textures/effect/<id>.png` at its own size.
fn aux_target(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    source: &mut AssetSource,
    a: &Value,
    names: &[String],
    size: (u32, u32),
    textures: &mut Vec<(wgpu::TextureView, wgpu::Sampler)>,
) -> Result<(String, Read, [f32; 2]), String> {
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
    let name = s("name").to_owned();
    if let Some(i) = names.iter().position(|t| t == s("id")) {
        return Ok((name, Read::Target(i), [size.0 as f32, size.1 as f32]));
    }
    let int = |k| a.get(k).and_then(Value::as_u64).ok_or(format!("aux {k}"));
    let aux_size = [int("width")? as f32, int("height")? as f32];
    let bilinear = a.get("bilinear").and_then(Value::as_bool) == Some(true);
    let path = format!("assets/minecraft/textures/effect/{}.png", s("id"));
    textures.push(effect_texture(device, queue, source, &path, bilinear)?);
    Ok((name, Read::Texture(textures.len() - 1), aux_size))
}

impl Pass {
    /// A sampler that the pass does not set keeps reading the input (`DiffuseSampler`, the
    /// first sampler of every vanilla program).
    fn bind_reads(
        &mut self,
        aux: &[(String, Read)],
        output: usize,
        program: &str,
    ) -> Result<(), String> {
        for (i, s) in self.samplers.iter().enumerate() {
            if let Some((_, r)) = aux.iter().find(|(n, _)| n == s) {
                self.reads[i] = *r;
            }
        }
        if self.reads.contains(&Read::Target(output)) {
            return Err(format!("pass {program} reads its own output"));
        }
        Ok(())
    }

    /// The pass `uniforms` of the chain JSON; each must be one the program JSON lists.
    fn set_uniforms(&mut self, p: &Value) -> Result<(), String> {
        for u in array(p, "uniforms") {
            let n = u.get("name").and_then(Value::as_str).unwrap_or_default();
            if !self.slots.iter().any(|s| s.name == n && s.listed) {
                return Err(format!("Uniform '{n}' does not exist"));
            }
            self.set(n, &floats(u.get("values")));
        }
        Ok(())
    }
}

impl Post {
    /// Where the world and the hand draw while the effect is on.
    pub fn main(&self) -> &wgpu::TextureView {
        &self.targets[0]
    }

    /// `PostChain.resize`: every target takes the window size, as vanilla resizes them all.
    pub fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32) {
        self.size = (w, h);
        let format = self.format;
        self.targets = self
            .names
            .iter()
            .map(|_| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size: wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            })
            .collect();
        let (x, y) = (w as f32, h as f32);
        let quad: [[f32; 4]; 4] = [
            [0.0, 0.0, 500.0, 1.0],
            [x, 0.0, 500.0, 1.0],
            [0.0, y, 500.0, 1.0],
            [x, y, 500.0, 1.0],
        ];
        queue.write_buffer(&self.quad, 0, bytemuck::cast_slice(&quad));
        let view = |r: Read| match r {
            Read::Target(i) => (&self.targets[i], &self.nearest),
            Read::Texture(i) => (&self.textures[i].0, &self.textures[i].1),
        };
        for pass in &mut self.passes {
            let mut entries: Vec<wgpu::BindGroupEntry> = pass
                .buffer
                .iter()
                .map(|b| wgpu::BindGroupEntry {
                    binding: 0,
                    resource: b.as_entire_binding(),
                })
                .collect();
            for (i, &r) in pass.reads.iter().enumerate() {
                let (tex, smp) = view(r);
                let b = 1 + 2 * i as u32;
                entries.push(wgpu::BindGroupEntry {
                    binding: b,
                    resource: wgpu::BindingResource::TextureView(tex),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: b + 1,
                    resource: wgpu::BindingResource::Sampler(smp),
                });
            }
            pass.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pass.layout,
                entries: &entries,
            }));
        }
        self.blit_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.blit_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.targets[0]),
            }],
        }));
    }

    /// The `time` step of `PostChain.process(partialTicks)`, once per frame.
    pub fn advance(&mut self, partial: f32) {
        if partial < self.last_stamp {
            self.time += 1.0 - self.last_stamp + partial;
        } else {
            self.time += partial - self.last_stamp;
        }
        self.last_stamp = partial;
        while self.time > 20.0 {
            self.time -= 20.0;
        }
    }

    /// `PostChain.process`: every pass, in order, each into a cleared target.
    pub fn process(&self, queue: &wgpu::Queue, enc: &mut wgpu::CommandEncoder) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        // `updateOrthoMatrix`: rows of `ortho(0, w, h, 0, 0.1, 1000)`.
        #[rustfmt::skip]
        let proj = [
            2.0 / w, 0.0, 0.0, -1.0,
            0.0, -2.0 / h, 0.0, 1.0,
            0.0, 0.0, -0.0020001999, -1.0001999,
            0.0, 0.0, 0.0, 1.0,
        ];
        for pass in &self.passes {
            let mut values = pass.values.clone();
            // `Effect.safeGetUniform(name).set(..)`: a no-op for a uniform the program JSON lacks.
            let mut set = |name: &str, v: &[f32]| {
                if pass.slots.iter().any(|s| s.name == name && s.listed) {
                    put(&pass.slots, &mut values, name, v);
                }
            };
            set("ProjMat", &proj);
            set("InSize", &[w, h]);
            set("OutSize", &[w, h]);
            set("Time", &[self.time / 20.0]);
            set("ScreenSize", &[w, h]);
            for (i, size) in pass.aux_sizes.iter().enumerate() {
                set(&format!("AuxSize{i}"), size);
            }
            if let Some(b) = &pass.buffer {
                queue.write_buffer(b, 0, &values);
            }
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets[pass.output],
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
            rp.set_pipeline(&pass.pipeline);
            rp.set_bind_group(0, pass.group.as_ref(), &[]);
            rp.set_vertex_buffer(0, self.quad.slice(..));
            rp.draw(0..4, 0..1);
        }
    }

    /// `Framebuffer.framebufferRender`: `minecraft:main` onto the window, alpha ignored. `pass`
    /// has the window's color target and the renderer's depth buffer.
    pub fn blit<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.blit);
        pass.set_bind_group(0, self.blit_group.as_ref(), &[]);
        pass.draw(0..3, 0..1);
    }
}

const BLIT: &str = "
@group(0) @binding(0) var main_: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(main_, vec2<i32>(at.xy), 0).rgb, 1.0);
}
";

fn read_json(source: &mut AssetSource, path: &str) -> Result<Value, String> {
    let bytes = source.read(path).ok_or(format!("{path} not found"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))
}

fn array<'a>(j: &'a Value, key: &str) -> &'a [Value] {
    j.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn floats(j: Option<&Value>) -> Vec<f32> {
    j.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_f64)
                .map(|v| v as f32)
                .collect()
        })
        .unwrap_or_default()
}

fn texture_binding() -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    }
}

/// `textures/effect/<id>.png`: rows flipped so the GL-to-wgpu `v` flip of every sample reads
/// them as GL does, `GL_REPEAT`, and `bilinear` picks the filter.
fn effect_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &mut AssetSource,
    path: &str,
    bilinear: bool,
) -> Result<(wgpu::TextureView, wgpu::Sampler), String> {
    let img = source
        .read(path)
        .and_then(|b| decode_png(&b))
        .ok_or(format!("{path} not found"))?;
    let rows: Vec<u8> = img
        .rgba
        .chunks(img.width * 4)
        .rev()
        .flatten()
        .copied()
        .collect();
    let view =
        crate::gpu::upload_texture(device, queue, img.width as u32, img.height as u32, &rows);
    let filter = if bilinear {
        wgpu::FilterMode::Linear
    } else {
        wgpu::FilterMode::Nearest
    };
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        ..Default::default()
    });
    Ok((view, sampler))
}

/// `Effect`: `shaders/program/<name>.json` with its two stages, its uniforms and its blend.
fn load_program(
    device: &wgpu::Device,
    source: &mut AssetSource,
    name: &str,
    format: wgpu::TextureFormat,
    input: usize,
    output: usize,
) -> Result<Pass, String> {
    let dir = "assets/minecraft/shaders/program";
    let json = read_json(source, &format!("{dir}/{name}.json"))?;
    let mut stage = |key: &str, ext: &str| {
        let file = json.get(key).and_then(Value::as_str).unwrap_or_default();
        let path = format!("{dir}/{file}.{ext}");
        let bytes = source.read(&path).ok_or(format!("{path} not found"))?;
        Ok::<_, String>(String::from_utf8_lossy(&bytes).into_owned())
    };
    let program = glsl::translate(&stage("vertex", "vsh")?, &stage("fragment", "fsh")?);
    let parse = |src: &str, stage| {
        naga::front::glsl::Frontend::default()
            .parse(&naga::front::glsl::Options::from(stage), src)
            .map_err(|e| format!("{name}: {}", e.emit_to_string(src)))
    };
    let vertex = parse(&program.vertex, naga::ShaderStage::Vertex)?;
    let fragment = parse(&program.fragment, naga::ShaderStage::Fragment)?;

    let listed: Vec<&str> = array(&json, "uniforms")
        .iter()
        .filter_map(|u| u.get("name")?.as_str())
        .collect();
    let (slots, block_size) = block(&vertex, &listed);
    let entries = layout_entries(block_size > 0, program.samplers.len() as u32);
    let module = |m| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(name),
            source: wgpu::ShaderSource::Naga(Cow::Owned(m)),
        })
    };
    let (pipeline, layout) = pipeline(
        device,
        &module(vertex),
        &module(fragment),
        &entries,
        format,
        blend(json.get("blend"))?,
        None,
    );
    let mut pass = Pass {
        pipeline,
        layout,
        output,
        reads: vec![Read::Target(input); program.samplers.len()],
        aux_sizes: Vec::new(),
        values: vec![0; block_size],
        buffer: (block_size > 0).then(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: block_size as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        }),
        slots,
        group: None,
        samplers: program.samplers,
    };
    for (n, v) in &program.inits {
        pass.set(n, v);
    }
    for u in array(&json, "uniforms") {
        let n = u.get("name").and_then(Value::as_str).unwrap_or_default();
        pass.set(n, &floats(u.get("values")));
    }
    Ok(pass)
}

/// The uniform block at binding 0 when the program has one, then a texture and a sampler for
/// each sampler at bindings `1 + 2i` and `2 + 2i`.
fn layout_entries(has_block: bool, samplers: u32) -> Vec<wgpu::BindGroupLayoutEntry> {
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty,
        count: None,
    };
    let block = wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let sampler = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
    has_block
        .then(|| entry(0, block))
        .into_iter()
        .chain((0..samplers).flat_map(|i| {
            [
                entry(1 + 2 * i, texture_binding()),
                entry(2 + 2 * i, sampler),
            ]
        }))
        .collect()
}

/// The members of the uniform block with naga's std140 offsets, and the block size.
fn block(m: &naga::Module, listed: &[&str]) -> (Vec<Slot>, usize) {
    let found = m
        .global_variables
        .iter()
        .find_map(|(_, g)| (g.space == naga::AddressSpace::Uniform).then(|| &m.types[g.ty].inner));
    let Some(naga::TypeInner::Struct { members, span }) = found else {
        return (Vec::new(), 0);
    };
    let slots = members
        .iter()
        .map(|mem| {
            let name = mem.name.clone().unwrap_or_default();
            let floats = match m.types[mem.ty].inner {
                naga::TypeInner::Vector { size, .. } => size as usize,
                naga::TypeInner::Matrix { .. } => 16,
                _ => 1,
            };
            Slot {
                listed: listed.contains(&name.as_str()),
                name,
                offset: mem.offset as usize,
                floats,
            }
        })
        .collect();
    (slots, *span as usize)
}

/// `BlendMode.fromJson`: the defaults (`add`, `1`, `0`) mean blending stays off; `srcalpha` or
/// `dstalpha` makes the alpha factors separate.
fn blend(j: Option<&Value>) -> Result<Option<wgpu::BlendState>, String> {
    use wgpu::{BlendFactor as F, BlendOperation as Op};
    let Some(j) = j else { return Ok(None) };
    let get = |k: &str| j.get(k).and_then(Value::as_str);
    let op = blend_op(get("func"));
    let factor = |k: &str, default| get(k).map_or(Ok(default), blend_factor);
    let (src, dst) = (factor("srcrgb", F::One)?, factor("dstrgb", F::Zero)?);
    let (src_a, dst_a) = (factor("srcalpha", F::One)?, factor("dstalpha", F::Zero)?);
    if op == Op::Add && (src, dst, src_a, dst_a) == (F::One, F::Zero, F::One, F::Zero) {
        return Ok(None);
    }
    let separate = get("srcalpha").is_some() || get("dstalpha").is_some();
    let comp = |s, d| {
        // GL ignores the factors of `min` and `max`; wgpu wants them `One`.
        let (s, d) = if matches!(op, Op::Min | Op::Max) {
            (F::One, F::One)
        } else {
            (s, d)
        };
        wgpu::BlendComponent {
            src_factor: s,
            dst_factor: d,
            operation: op,
        }
    };
    let color = comp(src, dst);
    Ok(Some(wgpu::BlendState {
        color,
        alpha: if separate { comp(src_a, dst_a) } else { color },
    }))
}

/// `BlendMode.stringToBlendFunc`; an unknown name is `add`.
fn blend_op(name: Option<&str>) -> wgpu::BlendOperation {
    use wgpu::BlendOperation as Op;
    match name.map(str::to_lowercase).as_deref() {
        Some("subtract") => Op::Subtract,
        Some("reversesubtract" | "reverse_subtract") => Op::ReverseSubtract,
        Some("min") => Op::Min,
        Some("max") => Op::Max,
        _ => Op::Add,
    }
}

/// `BlendMode.stringToBlendFactor`: `one_minus_src_alpha` and `1-srcalpha` are the same factor.
fn blend_factor(name: &str) -> Result<wgpu::BlendFactor, String> {
    use wgpu::BlendFactor as F;
    let s = name
        .to_lowercase()
        .replace('_', "")
        .replace("one", "1")
        .replace("zero", "0")
        .replace("minus", "-");
    Ok(match s.as_str() {
        "0" => F::Zero,
        "1" => F::One,
        "srccolor" => F::Src,
        "1-srccolor" => F::OneMinusSrc,
        "dstcolor" => F::Dst,
        "1-dstcolor" => F::OneMinusDst,
        "srcalpha" => F::SrcAlpha,
        "1-srcalpha" => F::OneMinusSrcAlpha,
        "dstalpha" => F::DstAlpha,
        "1-dstalpha" => F::OneMinusDstAlpha,
        _ => return Err(format!("blend factor '{s}'")),
    })
}

/// A pass or blit pipeline; the post vertex stage reads `Position` as a pixel-space `vec4`.
fn pipeline(
    device: &wgpu::Device,
    vertex: &wgpu::ShaderModule,
    fragment: &wgpu::ShaderModule,
    entries: &[wgpu::BindGroupLayoutEntry],
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    depth: Option<wgpu::TextureFormat>,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries,
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let attrs = wgpu::vertex_attr_array![0 => Float32x4];
    let quad = [wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &attrs,
    }];
    let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: vertex,
            entry_point: None,
            compilation_options: Default::default(),
            // The blit draws a full-screen triangle from vertex indices.
            buffers: if depth.is_some() { &[] } else { &quad },
        },
        fragment: Some(wgpu::FragmentState {
            module: fragment,
            entry_point: None,
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: if depth.is_some() {
                wgpu::PrimitiveTopology::TriangleList
            } else {
                wgpu::PrimitiveTopology::TriangleStrip
            },
            ..Default::default()
        },
        depth_stencil: depth.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    (pipe, layout)
}
