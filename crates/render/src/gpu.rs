//! wgpu device, pipelines and frame rendering.

use crate::assets::{decode_png, missing_tile, AssetSource, Atlas, Font, Image};
use crate::entity::{self, EntityDraw, Skins};
use crate::hand::{self, Hand, HeldKind};
use crate::mesh::{index, mesh_chunk, MeshData, Neighborhood, Vertex, HEIGHT};
use crate::particles::Quads;
use crate::sky::{build_clouds, build_sky, EnvTexture, Environment, SkyGeometry, ENV_H, ENV_W};
use crate::weather::{build_rain, precipitation_height};
use crate::{Camera, ChunkBlocks, Quad};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use wgpu::util::DeviceExt;

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CLEAR_COLOR: [f64; 3] = [0.0; 3];
/// Eye height of the standing player; rain is centred on the feet.
const EYE_HEIGHT: f64 = 1.62;
const OFFSCREEN: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Vertices of the 12 edges of the block selection outline.
const SELECTION_VERTICES: u32 = 24;
/// Vanilla draws the outline black at 40% alpha with 2 px lines; wgpu lines are 1 px, so the
/// alpha is raised to keep it as visible.
const SELECTION_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.8];
const PARTICLE_SHEET: &str = "textures/particle/particles.png";

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    fog_color: [f32; 4],
    /// Fog start, fog end, sun brightness, torch flicker factor.
    fog: [f32; 4],
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

/// The resolved model of the held item.
enum Held {
    Block(MeshData),
    /// Extruded sprite and its item texture, bound with the hand globals.
    Flat(hand::Flat, wgpu::BindGroup),
    /// No model (unknown item, entity models such as skulls): nothing is drawn.
    Nothing,
}

/// Which texture the hand geometry samples.
#[derive(Clone, Copy)]
enum HandTex {
    Skin,
    Atlas,
    Item,
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
    layout_bg: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    world_group: wgpu::BindGroup,
    env_group: wgpu::BindGroup,
    entity_group: wgpu::BindGroup,
    entity_skins: Skins,
    entity_mesh: Option<GpuMesh>,
    opaque_pipe: wgpu::RenderPipeline,
    water_pipe: wgpu::RenderPipeline,
    sky_pipe: wgpu::RenderPipeline,
    sky_add_pipe: wgpu::RenderPipeline,
    cloud_pipe: wgpu::RenderPipeline,
    hand_pipe: wgpu::RenderPipeline,
    line_pipe: wgpu::RenderPipeline,
    selection: Option<[f64; 6]>,
    hand_globals: wgpu::Buffer,
    hand_atlas_group: wgpu::BindGroup,
    /// Player skin and its height in pixels; `None` without assets.
    hand_skin: Option<(wgpu::BindGroup, u32)>,
    rain_group: Option<wgpu::BindGroup>,
    hand: Option<Hand>,
    held: Option<(HeldKind, Held)>,
    /// Camera-relative particle quads for the next frame.
    particles: Quads,
    /// Indexed by `Blend as usize`.
    gui_pipes: [wgpu::RenderPipeline; 4],
    /// GUI textures by path, loaded on first use.
    textures: HashMap<String, wgpu::BindGroup>,
    source: Option<AssetSource>,
    atlas: Atlas,
    font: Option<Font>,
    env_tex: EnvTexture,
    environment: Environment,
    blocks: HashMap<(i32, i32), Vec<u16>>,
    meshes: HashMap<(i32, i32), ChunkMesh>,
    dirty: HashSet<(i32, i32)>,
    screenshot_requested: bool,
    screenshot: Option<(u32, u32, Vec<u8>)>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GuiVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
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
        if surface
            .get_capabilities(&adapter)
            .usages
            .contains(wgpu::TextureUsages::COPY_SRC)
        {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        // `FERROCRAFT_VSYNC=0` picks Immediate or Mailbox where the surface has them (vanilla: VSync off).
        config.present_mode =
            present_mode(!std::env::var("FERROCRAFT_VSYNC").is_ok_and(|v| v == "0"));
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
        let env_tex = EnvTexture::load(source.as_mut());
        let entity_skins = source.as_mut().map_or_else(Skins::flat, Skins::from_assets);
        if font.is_none() {
            eprintln!(
                "ferrocraft-render: no textures/font/ascii.png in {assets:?}; text is not drawn"
            );
        }

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let hand_globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hand globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut load_png = |name: &str| {
            source
                .as_mut()
                .and_then(|s| s.read(&format!("assets/minecraft/textures/{name}.png")))
                .and_then(|b| decode_png(&b))
        };
        let rain_img = load_png("environment/rain");
        let skin_img = load_png("entity/steve");
        let atlas_view = upload_texture(&device, &queue, atlas.width, atlas.height, &atlas.rgba);
        let env_view = upload_texture(&device, &queue, ENV_W, ENV_H, &env_tex.rgba);
        let entity_view = upload_texture(
            &device,
            &queue,
            Skins::width(),
            Skins::height(),
            &entity_skins.rgba,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            // GL_REPEAT: `options_background.png` tiles with texture coordinates past 1.
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
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
        let world_group = bind_group(&device, &layout_bg, &globals, &sampler, &atlas_view);
        let env_group = bind_group(&device, &layout_bg, &globals, &sampler, &env_view);
        let view_of = |img: &Image| {
            upload_texture(
                &device,
                &queue,
                img.width as u32,
                img.height as u32,
                &img.rgba,
            )
        };
        let hand_atlas_group =
            bind_group(&device, &layout_bg, &hand_globals, &sampler, &atlas_view);
        let hand_skin = skin_img.map(|img| {
            let view = view_of(&img);
            (
                bind_group(&device, &layout_bg, &hand_globals, &sampler, &view),
                img.height as u32,
            )
        });
        let rain_group =
            rain_img.map(|img| bind_group(&device, &layout_bg, &globals, &sampler, &view_of(&img)));
        let entity_group = bind_group(&device, &layout_bg, &globals, &sampler, &entity_view);
        let [opaque_pipe, water_pipe, sky_pipe, sky_add_pipe, cloud_pipe, hand_pipe, line_pipe] =
            build_pipelines(&device, &layout_bg, format);
        let gui_pipes = gui_pipelines(&device, &layout_bg, format);

        let (w, h) = target_size(&target);
        let depth = depth_view(&device, w, h);
        Renderer {
            device,
            queue,
            target,
            depth,
            globals,
            layout_bg,
            sampler,
            world_group,
            env_group,
            entity_group,
            entity_skins,
            entity_mesh: None,
            opaque_pipe,
            water_pipe,
            sky_pipe,
            sky_add_pipe,
            cloud_pipe,
            hand_pipe,
            line_pipe,
            selection: None,
            hand_globals,
            hand_atlas_group,
            hand_skin,
            rain_group,
            hand: None,
            held: None,
            particles: Quads::default(),
            gui_pipes,
            textures: HashMap::new(),
            source,
            atlas,
            font,
            env_tex,
            environment: Environment::default(),
            blocks: HashMap::new(),
            meshes: HashMap::new(),
            dirty: HashSet::new(),
            screenshot_requested: false,
            screenshot: None,
        }
    }

    /// Glyph widths of the vanilla font; `None` without assets.
    pub fn font(&self) -> Option<&Font> {
        self.font.as_ref()
    }

    /// Block outline for the next frames: a world-space box `[x0, y0, z0, x1, y1, z1]`.
    pub fn set_selection(&mut self, selection: Option<[f64; 6]>) {
        self.selection = selection;
    }

    /// Switches the window between vsync and no vsync; offscreen renderers ignore it.
    pub fn set_vsync(&mut self, on: bool) {
        if let Target::Window { surface, config } = &mut self.target {
            config.present_mode = present_mode(on);
            surface.configure(&self.device, config);
        }
    }

    /// Captures the next rendered frame for [`Renderer::take_screenshot`].
    pub fn request_screenshot(&mut self) {
        self.screenshot_requested = true;
    }

    /// The captured frame as `(width, height, rgba8)`; `None` when none was captured or the
    /// surface cannot be read back.
    pub fn take_screenshot(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.screenshot.take()
    }

    /// Target size in pixels.
    pub fn size(&self) -> (u32, u32) {
        target_size(&self.target)
    }

    /// Upload `tex` the first time a quad names it: white for `""`, the missing tile when absent.
    fn load_texture(&mut self, tex: &str) {
        if self.textures.contains_key(tex) {
            return;
        }
        let image = if tex.is_empty() {
            Image {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            }
        } else {
            let path = format!("assets/minecraft/{tex}");
            let image = self
                .source
                .as_mut()
                .and_then(|s| s.read(&path))
                .and_then(|b| decode_png(&b));
            image.unwrap_or_else(|| {
                eprintln!("ferrocraft-render: missing {path}");
                missing_tile()
            })
        };
        let view = upload_texture(
            &self.device,
            &self.queue,
            image.width as u32,
            image.height as u32,
            &image.rgba,
        );
        let group = bind_group(
            &self.device,
            &self.layout_bg,
            &self.globals,
            &self.sampler,
            &view,
        );
        self.textures.insert(tex.to_owned(), group);
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
        self.mark_around(key);
    }

    pub fn remove_chunk(&mut self, x: i32, z: i32) {
        self.blocks.remove(&(x, z));
        self.meshes.remove(&(x, z));
        self.mark_neighbors((x, z));
    }

    pub fn clear_chunks(&mut self) {
        self.blocks.clear();
        self.meshes.clear();
        self.dirty.clear();
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
        self.mark_around(key);
    }

    fn mark_around(&mut self, key: (i32, i32)) {
        self.dirty.insert(key);
        self.mark_neighbors(key);
    }

    fn mark_neighbors(&mut self, (cx, cz): (i32, i32)) {
        self.dirty
            .extend([(cx - 1, cz), (cx + 1, cz), (cx, cz - 1), (cx, cz + 1)]);
    }

    /// Remesh every chunk touched since the last frame once, however many changes hit it.
    fn flush_meshes(&mut self) {
        for key in std::mem::take(&mut self.dirty) {
            self.remesh(key);
        }
    }

    fn remesh(&mut self, (cx, cz): (i32, i32)) {
        let Some(center) = self.blocks.get(&(cx, cz)) else {
            return;
        };
        let get = |k| self.blocks.get(&k).map(Vec::as_slice);
        let hood = Neighborhood {
            origin: (cx, cz),
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

    /// Replaces the entities drawn each frame: models are meshed here and live until the next call.
    pub fn set_entities(&mut self, draws: &[EntityDraw]) {
        let mesh = entity::build(draws, &self.entity_skins);
        self.entity_mesh = self.upload_mesh(mesh, [0.0; 3]);
    }

    /// Draw one frame to the window: the world, then `overlay` in order. Offscreen renderers draw into their texture.
    pub fn render(&mut self, cam: &Camera, overlay: &[Quad]) {
        self.flush_meshes();
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
        self.draw(&view, cam, overlay);
        if std::mem::take(&mut self.screenshot_requested) {
            self.screenshot = match (&frame, &self.target) {
                (Some(f), Target::Window { config, .. })
                    if config.usage.contains(wgpu::TextureUsages::COPY_SRC) =>
                {
                    Some(self.read_texture(&f.texture))
                }
                (None, Target::Offscreen { texture, .. }) => Some(self.read_texture(texture)),
                _ => None,
            };
        }
        if let Some(f) = frame {
            f.present();
        }
    }

    /// Time of day, weather and render distance for sky, fog and the light curve.
    pub fn set_environment(&mut self, environment: Environment) {
        self.environment = environment;
    }

    /// First-person hand for the next frames; `None` hides it (F1, third person).
    pub fn set_hand(&mut self, hand: Option<Hand>) {
        self.hand = hand;
    }

    /// Particle quads for the next frame only, from [`crate::Particles::vertices`] with
    /// [`Renderer::atlas`] and the camera of that frame (the quads are relative to `Camera.pos`).
    pub fn set_particles(&mut self, quads: Quads) {
        self.particles = quads;
    }

    /// The block atlas that [`crate::Particles::vertices`] samples.
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// Registers (or replaces) a GUI texture that quads name by `name` in `Quad.tex`, such as a
    /// server icon. A PNG that does not decode shows the missing tile.
    pub fn register_texture(&mut self, name: &str, png: &[u8]) {
        let image = decode_png(png).unwrap_or_else(|| {
            eprintln!("ferrocraft-render: {name} is not a PNG");
            missing_tile()
        });
        let view = upload_texture(
            &self.device,
            &self.queue,
            image.width as u32,
            image.height as u32,
            &image.rgba,
        );
        let group = bind_group(
            &self.device,
            &self.layout_bg,
            &self.globals,
            &self.sampler,
            &view,
        );
        self.textures.insert(name.to_owned(), group);
    }

    /// Highest block that stops rain in a loaded column, as `World.getPrecipitationHeight`.
    fn rain_top(&self, x: i32, z: i32) -> Option<i32> {
        let blocks = self.blocks.get(&(x >> 4, z >> 4))?;
        Some(precipitation_height(
            blocks,
            x.rem_euclid(16) as usize,
            z.rem_euclid(16) as usize,
        ))
    }

    /// Hand geometry for this frame and the texture it samples; `None` when no hand or texture exists.
    fn hand_draw(&mut self) -> Option<(hand::Draw, HandTex)> {
        let hand = self.hand?;
        let Some(kind) = hand.held else {
            let (_, height) = self.hand_skin.as_ref()?;
            return Some((hand::arm(&hand, *height), HandTex::Skin));
        };
        if self.held.as_ref().map(|h| h.0) != Some(kind) {
            let held = self.resolve_held(kind);
            self.held = Some((kind, held));
        }
        match &self.held.as_ref()?.1 {
            Held::Block(mesh) => Some((hand::block(&hand, mesh), HandTex::Atlas)),
            Held::Flat(flat, _) => Some((hand::flat(&hand, flat), HandTex::Item)),
            Held::Nothing => None,
        }
    }

    fn resolve_held(&mut self, kind: HeldKind) -> Held {
        let state = match kind {
            HeldKind::Block(state) => state,
            HeldKind::Item { id, damage } => {
                if let Some(flat) = self.flat_item(id, damage) {
                    return flat;
                }
                if !(1..256).contains(&id) {
                    return Held::Nothing;
                }
                ((id as u16) << 4) | (damage as u16 & 15)
            }
        };
        Held::Block(hand::held_mesh(state, &self.atlas))
    }

    /// The extruded sprite of a `builtin/generated` item model, with its `layer0` texture uploaded.
    /// shortcut: only `layer0`, untinted (leather armor, potions, spawn eggs lose color and overlay).
    fn flat_item(&mut self, id: i16, damage: i16) -> Option<Held> {
        let name = world::inventory::items::model(id, damage)?;
        let source = self.source.as_mut()?;
        let mut read = |p: &str| source.read(&format!("assets/minecraft/{p}"));
        let (texture, display) = hand::flat_model(&mut read, name)?;
        let image = decode_png(&read(&texture)?)?;
        let view = upload_texture(
            &self.device,
            &self.queue,
            image.width as u32,
            image.height as u32,
            &image.rgba,
        );
        let group = bind_group(
            &self.device,
            &self.layout_bg,
            &self.hand_globals,
            &self.sampler,
            &view,
        );
        Some(Held::Flat(hand::Flat::new(&image, display), group))
    }

    fn hand_group(&self, tex: HandTex) -> &wgpu::BindGroup {
        match (tex, &self.hand_skin, &self.held) {
            (HandTex::Skin, Some((group, _)), _) => group,
            (HandTex::Item, _, Some((_, Held::Flat(_, group)))) => group,
            _ => &self.hand_atlas_group,
        }
    }

    fn rain_mesh(&self, env: &Environment, cam: &Camera) -> Option<GpuMesh> {
        self.rain_group.as_ref()?;
        let feet = [cam.pos[0], cam.pos[1] - EYE_HEIGHT, cam.pos[2]];
        let rain = build_rain(env, feet, |x, z| self.rain_top(x, z));
        self.upload_geometry(&rain)
    }

    /// Uploads the hand and its own projection, with the texture it samples.
    fn hand_mesh(
        &mut self,
        cam: &Camera,
        world: &Globals,
        sun_brightness: f32,
    ) -> Option<(GpuMesh, HandTex)> {
        let (mut draw, tex) = self.hand_draw()?;
        // A block above the eye keeps the hand dark.
        let covered = self
            .rain_top(cam.pos[0].floor() as i32, cam.pos[2].floor() as i32)
            .is_some_and(|top| top as f64 > cam.pos[1]);
        let light = if covered { [0.0, 0.0] } else { [1.0, 0.0] };
        for v in &mut draw.vertices {
            v.light = light;
        }
        let [w, h, ..] = world.screen;
        let globals = Globals {
            view_proj: mul(
                projection(cam.fov_deg, w / h),
                mul(bob_matrix(cam.bob), draw.model),
            ),
            eye: [0.0, 0.0, 0.0, self.environment.gamma],
            fog_color: world.fog_color,
            fog: [1.0e6, 2.0e6, sun_brightness, 1.5],
            screen: world.screen,
        };
        self.queue
            .write_buffer(&self.hand_globals, 0, bytemuck::bytes_of(&globals));
        let geometry = SkyGeometry {
            vertices: draw.vertices,
            indices: draw.indices,
        };
        Some((self.upload_geometry(&geometry)?, tex))
    }

    fn upload_geometry(&self, g: &SkyGeometry) -> Option<GpuMesh> {
        if g.indices.is_empty() {
            return None;
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
                bytemuck::cast_slice(&g.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buf(bytemuck::cast_slice(&g.indices), wgpu::BufferUsages::INDEX),
            count: g.indices.len() as u32,
        })
    }

    fn draw(&mut self, view: &wgpu::TextureView, cam: &Camera, overlay: &[Quad]) {
        let (w, h) = self.size();
        let env = self.environment;
        let sky = env.sky_state();
        let globals = Globals {
            view_proj: view_proj(cam, w as f32 / h as f32),
            eye: [
                cam.pos[0] as f32,
                cam.pos[1] as f32,
                cam.pos[2] as f32,
                env.gamma,
            ],
            fog_color: [sky.fog[0], sky.fog[1], sky.fog[2], 1.0],
            // Torch flicker is a slow random walk around 0 in vanilla, so the factor stays near 1.5.
            fog: [sky.fog_start, sky.fog_end, sky.sun_brightness, 1.5],
            screen: [w as f32, h as f32, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let layers = build_sky(&env, &sky, cam.pos);
        let sky_solid = self.upload_geometry(&layers.solid);
        let sky_blend = self.upload_geometry(&layers.blend);
        let sky_add = self.upload_geometry(&layers.additive);
        let clouds = self
            .env_tex
            .has_clouds
            .then(|| build_clouds(&env, &sky, cam.pos, &self.env_tex.clouds))
            .and_then(|g| self.upload_geometry(&g));

        let rain = self.rain_mesh(&env, cam);
        let hand = self.hand_mesh(cam, &globals, sky.sun_brightness);
        let offset = cam.pos.map(|c| c as f32);
        let quads = std::mem::take(&mut self.particles);
        let particle_block = self.upload_mesh(quad_mesh(quads.block), offset);
        let particle_sheet = self.upload_mesh(quad_mesh(quads.sheet), offset);
        if particle_sheet.is_some() {
            self.load_texture(PARTICLE_SHEET);
        }

        for q in overlay {
            self.load_texture(&q.tex);
        }
        let verts: Vec<GuiVertex> = overlay.iter().flat_map(quad_vertices).collect();
        let gui_buf = (!verts.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });

        let sel_buf = self.selection.map(|b| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&selection_lines(b)),
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
                            r: sky.fog[0] as f64 + CLEAR_COLOR[0],
                            g: sky.fog[1] as f64 + CLEAR_COLOR[1],
                            b: sky.fog[2] as f64 + CLEAR_COLOR[2],
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
            pass.set_bind_group(0, &self.env_group, &[]);
            pass.set_pipeline(&self.sky_pipe);
            for m in [&sky_solid, &sky_blend].into_iter().flatten() {
                draw_mesh(&mut pass, m);
            }
            pass.set_pipeline(&self.sky_add_pipe);
            if let Some(m) = &sky_add {
                draw_mesh(&mut pass, m);
            }
            pass.set_bind_group(0, &self.world_group, &[]);
            pass.set_pipeline(&self.opaque_pipe);
            for m in self.meshes.values().filter_map(|m| m.opaque.as_ref()) {
                draw_mesh(&mut pass, m);
            }
            if let Some(m) = &self.entity_mesh {
                pass.set_bind_group(0, &self.entity_group, &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            // `EffectRenderer.renderParticles` runs after entities and before the translucent layer.
            pass.set_pipeline(&self.water_pipe);
            if let Some(m) = &particle_block {
                draw_mesh(&mut pass, m);
            }
            if let Some(m) = &particle_sheet {
                pass.set_bind_group(0, &self.textures[PARTICLE_SHEET], &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            for m in self.meshes.values().filter_map(|m| m.water.as_ref()) {
                draw_mesh(&mut pass, m);
            }
            if let Some(buf) = &sel_buf {
                pass.set_bind_group(0, &self.world_group, &[]);
                pass.set_pipeline(&self.line_pipe);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..SELECTION_VERTICES, 0..1);
            }
            if let Some(m) = &clouds {
                pass.set_bind_group(0, &self.env_group, &[]);
                pass.set_pipeline(&self.cloud_pipe);
                draw_mesh(&mut pass, m);
            }
            if let (Some(m), Some(group)) = (&rain, &self.rain_group) {
                pass.set_bind_group(0, group, &[]);
                pass.set_pipeline(&self.water_pipe);
                draw_mesh(&mut pass, m);
            }
        }
        {
            // Vanilla draws the hand on a cleared depth buffer, then the GUI.
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
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
            if let Some((m, tex)) = &hand {
                pass.set_bind_group(0, self.hand_group(*tex), &[]);
                pass.set_pipeline(&self.hand_pipe);
                draw_mesh(&mut pass, m);
            }
            if let Some(buf) = &gui_buf {
                pass.set_vertex_buffer(0, buf.slice(..));
                // Consecutive quads with the same texture and blend share one draw; order is kept.
                let mut start = 0;
                for run in overlay.chunk_by(|a, b| a.tex == b.tex && a.blend == b.blend) {
                    pass.set_bind_group(0, &self.textures[&*run[0].tex], &[]);
                    pass.set_pipeline(&self.gui_pipes[run[0].blend as usize]);
                    let end = start + run.len() as u32 * 6;
                    pass.draw(start..end, 0..1);
                    start = end;
                }
            }
        }
        self.queue.submit([enc.finish()]);
    }

    /// Render one frame offscreen and return `(width, height, rgba8)`. Only valid for offscreen renderers.
    pub fn render_to_rgba(&mut self, cam: &Camera, overlay: &[Quad]) -> (u32, u32, Vec<u8>) {
        self.render(cam, overlay);
        let Target::Offscreen { texture, .. } = &self.target else {
            panic!("render_to_rgba needs an offscreen renderer")
        };
        self.read_texture(texture)
    }

    /// Copies a rendered texture back as `(width, height, rgba8)`, swapping BGRA surfaces to RGBA.
    fn read_texture(&self, texture: &wgpu::Texture) -> (u32, u32, Vec<u8>) {
        let (w, h) = (texture.width(), texture.height());
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
        if matches!(
            texture.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for px in out.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        (w, h, out)
    }
}

fn argb(c: u32) -> [f32; 4] {
    let ch = |s: u32| ((c >> s) & 255) as f32 / 255.0;
    [ch(16), ch(8), ch(0), ch(24)]
}

/// Two triangles over the corners top-left, top-right, bottom-right, bottom-left.
fn quad_vertices(q: &Quad) -> [GuiVertex; 6] {
    let v = |i: usize| GuiVertex {
        pos: q.pos[i],
        uv: q.uv[i],
        color: argb(q.color[i]),
    };
    [v(0), v(3), v(2), v(0), v(2), v(1)]
}

fn gui_pipelines(
    device: &wgpu::Device,
    layout_bg: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> [wgpu::RenderPipeline; 4] {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("gui.wgsl").into()),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[layout_bg],
        push_constant_ranges: &[],
    });
    let attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
    use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation::Add};
    let comp = |src, dst| C {
        src_factor: src,
        dst_factor: dst,
        operation: Add,
    };
    let alpha = comp(F::One, F::OneMinusSrcAlpha);
    [
        wgpu::BlendState {
            color: comp(F::SrcAlpha, F::OneMinusSrcAlpha),
            alpha,
        },
        wgpu::BlendState {
            color: comp(F::OneMinusDst, F::OneMinusSrc),
            alpha,
        },
        wgpu::BlendState {
            color: comp(F::Zero, F::OneMinusSrc),
            alpha,
        },
        wgpu::BlendState {
            color: comp(F::Src, F::One),
            alpha,
        },
    ]
    .map(|blend| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_gui"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GuiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_gui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
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
        })
    })
}

fn build_pipelines(
    device: &wgpu::Device,
    layout_bg: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> [wgpu::RenderPipeline; 7] {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[layout_bg],
        push_constant_ranges: &[],
    });
    let world_attrs =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2];
    let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
    // Vanilla draws the sun and moon with `SRC_ALPHA, ONE`.
    let additive = Some(wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent::OVER,
    });
    let pipe = |vs: &str,
                fs: &str,
                stride: usize,
                attrs: &[wgpu::VertexAttribute],
                blend: Option<wgpu::BlendState>,
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
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: if fs == "fs_flat" {
                    wgpu::PrimitiveTopology::LineList
                } else {
                    wgpu::PrimitiveTopology::TriangleList
                },
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
    let stride = std::mem::size_of::<Vertex>();
    let opaque_pipe = pipe(
        "vs_world",
        "fs_world",
        stride,
        &world_attrs,
        None,
        true,
        true,
        true,
    );
    let water_pipe = pipe(
        "vs_world",
        "fs_world",
        stride,
        &world_attrs,
        alpha,
        false,
        true,
        false,
    );
    let sky_pipe = pipe(
        "vs_world",
        "fs_sky",
        stride,
        &world_attrs,
        alpha,
        false,
        false,
        false,
    );
    let sky_add_pipe = pipe(
        "vs_world",
        "fs_sky",
        stride,
        &world_attrs,
        additive,
        false,
        false,
        false,
    );
    let cloud_pipe = pipe(
        "vs_world",
        "fs_sky",
        stride,
        &world_attrs,
        alpha,
        false,
        true,
        false,
    );
    // The hand draws opaque on a fresh depth buffer; its quads have no fixed winding.
    let hand_pipe = pipe(
        "vs_world",
        "fs_world",
        stride,
        &world_attrs,
        None,
        true,
        true,
        false,
    );
    let line_pipe = pipe(
        "vs_world",
        "fs_flat",
        stride,
        &world_attrs,
        alpha,
        false,
        true,
        false,
    );
    [
        opaque_pipe,
        water_pipe,
        sky_pipe,
        sky_add_pipe,
        cloud_pipe,
        hand_pipe,
        line_pipe,
    ]
}

/// The 12 edges of a world-space box `[x0, y0, z0, x1, y1, z1]` as line-list vertices.
fn selection_lines(b: [f64; 6]) -> [Vertex; SELECTION_VERTICES as usize] {
    let corner = |i: usize| {
        let pick = |axis: usize| b[axis + 3 * ((i >> axis) & 1)] as f32;
        Vertex {
            pos: [pick(0), pick(1), pick(2)],
            uv: [0.0; 2],
            color: SELECTION_COLOR,
            light: [1.0; 2],
        }
    };
    let mut v = [corner(0); SELECTION_VERTICES as usize];
    let mut n = 0;
    // Corner indices differ in exactly one bit along an edge.
    for a in 0..8usize {
        for axis in 0..3 {
            if a & (1 << axis) == 0 {
                v[n] = corner(a);
                v[n + 1] = corner(a | 1 << axis);
                n += 2;
            }
        }
    }
    v
}

/// Indexed triangles for vertices that come four per quad.
fn quad_mesh(vertices: Vec<Vertex>) -> MeshData {
    let indices = (0..vertices.len() as u32 / 4)
        .flat_map(|q| [0, 1, 2, 0, 2, 3].map(|i| q * 4 + i))
        .collect();
    MeshData { vertices, indices }
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
    let mut adapter = None;
    // A software adapter (lavapipe, WARP) is the fallback when no GPU exists, e.g. on CI runners.
    for force_fallback_adapter in [false, true] {
        adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: surface,
                force_fallback_adapter,
                ..Default::default()
            })
            .await;
        if adapter.is_some() {
            break;
        }
    }
    let adapter = adapter.expect("no wgpu adapter found");
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

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
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
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
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

/// Column-major view-projection (OpenGL-style camera math, wgpu 0..1 depth).
fn view_proj(cam: &Camera, aspect: f32) -> [[f32; 4]; 4] {
    let (yaw, pitch) = (cam.yaw.to_radians(), cam.pitch.to_radians());
    let fwd = [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ];
    let right = [-yaw.cos(), 0.0, -yaw.sin()];
    let up = cross(right, fwd);
    let eye = [cam.pos[0] as f32, cam.pos[1] as f32, cam.pos[2] as f32];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let view = [
        [right[0], up[0], -fwd[0], 0.0],
        [right[1], up[1], -fwd[1], 0.0],
        [right[2], up[2], -fwd[2], 0.0],
        [-dot(right, eye), -dot(up, eye), dot(fwd, eye), 1.0],
    ];
    mul(
        projection(cam.fov_deg, aspect),
        mul(bob_matrix(cam.bob), view),
    )
}

/// `T(x, y, 0) * Rz(roll) * Rx(pitch)` of `EntityRenderer.setupViewBobbing`, column-major.
fn bob_matrix([x, y, roll, pitch]: [f32; 4]) -> [[f32; 4]; 4] {
    let (sr, cr) = roll.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let t = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, 0.0, 1.0],
    ];
    let rz = [
        [cr, sr, 0.0, 0.0],
        [-sr, cr, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let rx = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, cp, sp, 0.0],
        [0.0, -sp, cp, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    mul(t, mul(rz, rx))
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}

fn projection(fov_deg: f32, aspect: f32) -> [[f32; 4]; 4] {
    let (near, far) = (0.05f32, 1024.0f32);
    let f = 1.0 / (fov_deg.to_radians() / 2.0).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn clip_x(cam: &Camera, p: [f32; 3]) -> f32 {
        let m = view_proj(cam, 1.0);
        (0..3).map(|c| m[c][0] * p[c]).sum::<f32>() + m[3][0]
    }

    /// A point at the player's left stays left while the pitch clamps at +-90.
    #[test]
    fn view_does_not_flip_at_the_pitch_clamp() {
        let at = |pitch| Camera {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch,
            fov_deg: 70.0,
            ..Camera::default()
        };
        // Yaw 0 faces +z, so +x is on the left of the screen: clip x < 0.
        for pitch in [-90.0, -89.0, 0.0, 89.0, 90.0] {
            assert!(clip_x(&at(pitch), [1.0, 0.0, 0.0]) < 0.0, "pitch {pitch}");
        }
    }

    #[test]
    fn bob_translates_then_rolls_then_pitches_in_view_space() {
        let apply = |m: [[f32; 4]; 4], p: [f32; 3]| {
            [0, 1, 2].map(|r| (0..3).map(|c| m[c][r] * p[c]).sum::<f32>() + m[3][r])
        };
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5);
        let p = [1.0, 2.0, 3.0];
        assert!(near(apply(bob_matrix([0.0; 4]), p), p));
        assert!(near(
            apply(bob_matrix([0.5, -0.25, 0.0, 0.0]), p),
            [1.5, 1.75, 3.0]
        ));
        assert!(near(
            apply(bob_matrix([0.0, 0.0, 90.0, 0.0]), [1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0]
        ));
        assert!(near(
            apply(bob_matrix([0.0, 0.0, 0.0, 90.0]), [0.0, 1.0, 0.0]),
            [0.0, 0.0, 1.0]
        ));
        assert!(near(
            apply(bob_matrix([1.0, 0.0, 90.0, 0.0]), [1.0, 0.0, 0.0]),
            [1.0, 1.0, 0.0]
        ));
    }
}
