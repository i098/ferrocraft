//! wgpu device, pipelines and frame rendering.

use crate::assets::{decode_png, missing_tile, AssetSource, Atlas, Font, Image};
use crate::biome::{temperature, PLAINS_TEMPERATURE};
use crate::blockent::{BlockEntities, Tex};
use crate::crack;
use crate::entity::{self, EntityDraw, Skins};
use crate::hand::{self, Hand, HeldKind};
use crate::mesh::{index, MeshData, Vertex, AROUND, HEIGHT};
use crate::mesher::{Done, Job, Mesher, SharedBiomes, SharedBlocks};
use crate::particles::Quads;
use crate::post::Post;
use crate::sky::{
    build_clouds, build_sky, EnvTexture, Environment, SkyGeometry, SkyState, ENV_H, ENV_W,
};
use crate::weather::{build_precipitation, precipitation_height};
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
const EXPLOSION_SHEET: &str = "textures/entity/explosion.png";
const FOOTPRINT_SHEET: &str = "textures/particle/footprint.png";
/// Outstanding mesh tasks (queued, running or waiting for upload); `ChunkRenderDispatcher.pendingTasks` holds 100.
const MAX_PENDING_MESHES: usize = 100;
/// Time per frame for uploading finished meshes, at least one; vanilla `runTasksUntil` stops at the frame's finish time.
const UPLOAD_BUDGET: std::time::Duration = std::time::Duration::from_millis(2);

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
    light: Option<crate::light::Light>,
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
    entity_tex: wgpu::Texture,
    entity_mesh: Option<GpuMesh>,
    /// Chests, signs, skulls and banners, and worn heads, by texture (`block_entities.rs`).
    block_entities: BlockEntities,
    tile_meshes: Vec<(Tex, GpuMesh)>,
    head_meshes: Vec<(Tex, GpuMesh)>,
    glint_mesh: Option<GpuMesh>,
    /// The additive white flash of primed TNT.
    flash_mesh: Option<GpuMesh>,
    /// Item models for held and dropped items, by `(id, damage)`.
    item_models: entity::item::Models,
    /// Wood models of item frames, read the first time one is drawn.
    frames: Option<entity::hanging::Frames>,
    /// Colour index bytes of each received map, 128x128.
    maps: HashMap<i32, Vec<u8>>,
    /// Held and dropped item meshes: the block atlas (`None`) or the sprite texture they sample.
    item_meshes: Vec<(Option<String>, GpuMesh)>,
    fires: Vec<entity::fire::Fire>,
    on_fire: bool,
    fire_frames: [u32; 2],
    fire_pipe: wgpu::RenderPipeline,
    opaque_pipe: wgpu::RenderPipeline,
    water_pipe: wgpu::RenderPipeline,
    /// `TRANSLUCENT` chunk layer: blended like `water_pipe`, but back faces culled as vanilla does.
    translucent_pipe: wgpu::RenderPipeline,
    sky_pipe: wgpu::RenderPipeline,
    sky_add_pipe: wgpu::RenderPipeline,
    cloud_pipe: wgpu::RenderPipeline,
    /// `renderCloudsFancy` first pass: `colorMask(false, ...)`, depth only.
    cloud_depth_pipe: wgpu::RenderPipeline,
    hand_pipe: wgpu::RenderPipeline,
    line_pipe: wgpu::RenderPipeline,
    /// `renderMiningProgress` blend: source times destination, twice.
    crack_pipe: wgpu::RenderPipeline,
    selection: Option<[f64; 6]>,
    /// Blocks with a crack stage `0..=9` for the next frames.
    cracks: Vec<([i32; 3], u8)>,
    /// Lightning bolts `(position, seed)` for the next frames.
    bolts: Vec<([f64; 3], i64)>,
    bolt_pipe: wgpu::RenderPipeline,
    border_pipe: wgpu::RenderPipeline,
    /// The world border to draw, if any.
    border: Option<crate::Border>,
    hand_glint_pipe: wgpu::RenderPipeline,
    hand_globals: wgpu::Buffer,
    hand_atlas_group: wgpu::BindGroup,
    /// Player skin and its height in pixels; `None` without assets.
    hand_skin: Option<(wgpu::BindGroup, u32)>,
    rain_group: Option<wgpu::BindGroup>,
    snow_group: Option<wgpu::BindGroup>,
    hand: Option<Hand>,
    hand_glint_group: Option<wgpu::BindGroup>,
    held: Option<(HeldKind, Held)>,
    /// Camera-relative particle quads for the next frame.
    particles: Quads,
    /// Indexed by `Blend as usize`.
    gui_pipes: [wgpu::RenderPipeline; 4],
    /// GUI textures by path, loaded on first use.
    textures: HashMap<String, wgpu::BindGroup>,
    /// Icon texture path of item `(id, damage)`, `None` when its model is not a flat sprite.
    item_textures: HashMap<(i16, i16), Option<String>>,
    source: Option<AssetSource>,
    atlas: Arc<Atlas>,
    font: Option<Font>,
    env_tex: EnvTexture,
    environment: Environment,
    /// The loaded `shaders/post` effect and whether it runs (F4, `useShader`).
    post: Option<Post>,
    post_on: bool,
    blocks: HashMap<(i32, i32), SharedBlocks>,
    /// Biome ids per chunk column, `z << 4 | x`.
    biomes: HashMap<(i32, i32), SharedBiomes>,
    meshes: HashMap<(i32, i32), ChunkMesh>,
    mesher: Mesher,
    /// Chunks to remesh on the workers.
    dirty: HashSet<(i32, i32)>,
    /// Chunks to remesh on the render thread this frame (a block changed in them), as vanilla `rebuildSync`.
    urgent: HashSet<(i32, i32)>,
    /// Newest mesh job per chunk; a result with another generation is stale.
    pending: HashMap<(i32, i32), u64>,
    next_generation: u64,
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
    /// Window renderer. `assets` is the 1.8.9 client jar or an extracted assets directory, with
    /// any resource packs over it; `None` draws flat colors and no text.
    pub fn new(window: Arc<winit::window::Window>, assets: Option<AssetSource>) -> Renderer {
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
        Self::new_offscreen_from(width, height, assets.and_then(AssetSource::open))
    }

    /// [`Renderer::new_offscreen`] over an opened source, such as resource packs over the jar.
    pub fn new_offscreen_from(width: u32, height: u32, source: Option<AssetSource>) -> Renderer {
        // One process-wide instance, never dropped, and serialized device creation: parallel
        // tests that each create and destroy a Vulkan instance crash lavapipe (issue #129).
        static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
        static CREATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let instance = INSTANCE.get_or_init(wgpu::Instance::default);
        let (_, device, queue) = {
            let _guard = CREATE.lock().unwrap_or_else(|e| e.into_inner());
            pollster::block_on(request_device(instance, None))
        };
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
            source,
        )
    }

    fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        target: Target,
        format: wgpu::TextureFormat,
        mut source: Option<AssetSource>,
    ) -> Renderer {
        let atlas = Arc::new(source.as_mut().map_or_else(Atlas::flat, Atlas::from_assets));
        let font = source.as_mut().and_then(Font::load);
        let env_tex = EnvTexture::load(source.as_mut());
        let entity_skins = source.as_mut().map_or_else(Skins::flat, Skins::from_assets);
        if font.is_none() {
            eprintln!(
                "ferrocraft-render: no textures/font/ascii.png in the assets; text is not drawn"
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
        let snow_img = load_png("environment/snow");
        let skin_img = load_png("entity/steve");
        let atlas_view = upload_texture(&device, &queue, atlas.width, atlas.height, &atlas.rgba);
        let env_view = upload_texture(&device, &queue, ENV_W, ENV_H, &env_tex.rgba);
        let entity_tex = create_texture(
            &device,
            &queue,
            Skins::width(),
            Skins::height(),
            &entity_skins.rgba,
        );
        let entity_view = entity_tex.create_view(&Default::default());
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
        let snow_group =
            snow_img.map(|img| bind_group(&device, &layout_bg, &globals, &sampler, &view_of(&img)));
        let entity_group = bind_group(&device, &layout_bg, &globals, &sampler, &entity_view);
        let [opaque_pipe, water_pipe, translucent_pipe, sky_pipe, sky_add_pipe, cloud_pipe, cloud_depth_pipe, hand_pipe, line_pipe, crack_pipe, bolt_pipe, fire_pipe, hand_glint_pipe, border_pipe] =
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
            entity_tex,
            entity_mesh: None,
            block_entities: BlockEntities::default(),
            tile_meshes: Vec::new(),
            head_meshes: Vec::new(),
            glint_mesh: None,
            flash_mesh: None,
            item_models: HashMap::new(),
            frames: None,
            maps: HashMap::new(),
            item_meshes: Vec::new(),
            fires: Vec::new(),
            on_fire: false,
            fire_frames: [0; 2],
            fire_pipe,
            opaque_pipe,
            water_pipe,
            translucent_pipe,
            sky_pipe,
            sky_add_pipe,
            cloud_pipe,
            cloud_depth_pipe,
            hand_pipe,
            line_pipe,
            crack_pipe,
            cracks: Vec::new(),
            bolt_pipe,
            border_pipe,
            border: None,
            hand_glint_pipe,
            bolts: Vec::new(),
            selection: None,
            hand_globals,
            hand_atlas_group,
            hand_skin,
            rain_group,
            snow_group,
            hand: None,
            hand_glint_group: None,
            held: None,
            particles: Quads::default(),
            gui_pipes,
            textures: HashMap::new(),
            item_textures: HashMap::new(),
            source,
            mesher: Mesher::new(atlas.clone()),
            atlas,
            font,
            env_tex,
            environment: Environment::default(),
            post: None,
            post_on: false,
            blocks: HashMap::new(),
            biomes: HashMap::new(),
            meshes: HashMap::new(),
            dirty: HashSet::new(),
            urgent: HashSet::new(),
            pending: HashMap::new(),
            next_generation: 0,
            screenshot_requested: false,
            screenshot: None,
        }
    }

    /// Glyph widths of the vanilla font; `None` without assets.
    pub fn font(&self) -> Option<&Font> {
        self.font.as_ref()
    }

    /// `FontRenderer.setUnicodeFlag`; returns whether it changed.
    pub fn set_force_unicode(&mut self, on: bool) -> bool {
        self.font
            .as_mut()
            .is_some_and(|f| std::mem::replace(&mut f.force_unicode, on) != on)
    }

    /// Lightning bolts `(position, seed)` to draw: see [`crate::Bolts::shapes`].
    pub fn set_lightning(&mut self, bolts: Vec<([f64; 3], i64)>) {
        self.bolts = bolts;
    }

    /// The world border walls for the next frames; `None` draws none.
    pub fn set_border(&mut self, border: Option<crate::Border>) {
        self.border = border;
    }

    /// Block outline for the next frames: a world-space box `[x0, y0, z0, x1, y1, z1]`.
    pub fn set_selection(&mut self, selection: Option<[f64; 6]>) {
        self.selection = selection;
    }

    /// Blocks being mined, with their destroy stage in `0..=9`: the local target and other players' `BlockBreakAnim`.
    pub fn set_cracks(&mut self, cracks: Vec<([i32; 3], u8)>) {
        self.cracks = cracks;
    }

    /// One mesh per destroy stage in use, with the stage texture's name.
    fn crack_meshes(&mut self) -> Vec<(String, GpuMesh)> {
        let mut out = Vec::new();
        for stage in 0..10u8 {
            let mut verts = Vec::new();
            for &(at, _) in self.cracks.iter().filter(|c| c.1 == stage) {
                let at_block = |x: i32, y: i32, z: i32| match self
                    .blocks
                    .get(&(x.div_euclid(16), z.div_euclid(16)))
                {
                    Some(d) if (0..HEIGHT as i32).contains(&y) => {
                        d[index(
                            x.rem_euclid(16) as usize,
                            y as usize,
                            z.rem_euclid(16) as usize,
                        )]
                    }
                    _ => 0,
                };
                verts.extend(crack::quads(at, at_block(at[0], at[1], at[2]), &at_block));
            }
            let tex = format!("textures/blocks/destroy_stage_{stage}.png");
            if let Some(m) = self.upload_mesh(quad_mesh(verts), [0.0; 3]) {
                self.load_texture(&tex);
                out.push((tex, m));
            }
        }
        out
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
        self.insert_texture(tex, &image);
    }

    /// Uploads `image` as texture `tex`, replacing an earlier one.
    fn insert_texture(&mut self, tex: &str, image: &Image) {
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
        if tex == entity::GLINT_TEXTURE {
            self.hand_glint_group = Some(bind_group(
                &self.device,
                &self.layout_bg,
                &self.hand_globals,
                &self.sampler,
                &view,
            ));
        }
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
        if let Some(post) = &mut self.post {
            post.resize(&self.device, &self.queue, w, h);
        }
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
        self.block_entities
            .load_chunk(key.0, key.1, |x, y, z| data[index(x, y, z)]);
        self.blocks.insert(key, Arc::new(data));
        self.biomes.insert(
            key,
            Arc::new(std::array::from_fn(|i| chunk.biome(i & 15, i >> 4))),
        );
        self.mark_around(key);
    }

    pub fn remove_chunk(&mut self, x: i32, z: i32) {
        self.blocks.remove(&(x, z));
        self.block_entities.remove_chunk(x, z);
        self.biomes.remove(&(x, z));
        self.meshes.remove(&(x, z));
        self.pending.remove(&(x, z));
        self.mark_neighbors((x, z));
    }

    pub fn clear_chunks(&mut self) {
        self.blocks.clear();
        self.block_entities.clear();
        self.biomes.clear();
        self.meshes.clear();
        self.dirty.clear();
        self.urgent.clear();
        self.pending.clear();
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
        let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
        // Copies only while a mesh job still reads the old data.
        Arc::make_mut(data)[index(lx as usize, y as usize, lz as usize)] = state;
        self.block_entities.set_state([x, y, z], state);
        self.mark_around(key);
        // The edited chunk, and a neighbour when the block touches its edge, show the change this frame.
        self.urgent.insert(key);
        for (edge, dx, dz) in [
            (lx == 0, -1, 0),
            (lx == 15, 1, 0),
            (lz == 0, 0, -1),
            (lz == 15, 0, 1),
        ] {
            if edge {
                self.urgent.insert((key.0 + dx, key.1 + dz));
            }
        }
    }

    fn mark_around(&mut self, key: (i32, i32)) {
        self.dirty.insert(key);
        self.mark_neighbors(key);
    }

    fn mark_neighbors(&mut self, (cx, cz): (i32, i32)) {
        // Light crosses chunk edges and corners, so all eight neighbours change.
        self.dirty.extend(AROUND.map(|(dx, dz)| (cx + dx, cz + dz)));
    }

    /// Mesh jobs for the chunks around `near`, nearest first, up to `MAX_PENDING_MESHES` outstanding.
    fn dispatch_meshes(&mut self, near: (i32, i32)) {
        let mut keys: Vec<_> = self.dirty.drain().collect();
        keys.retain(|k| self.blocks.contains_key(k));
        keys.sort_unstable_by_key(|&(x, z)| (x - near.0).pow(2) + (z - near.1).pow(2));
        let room = MAX_PENDING_MESHES.saturating_sub(self.pending.len());
        self.dirty.extend(keys.split_off(room.min(keys.len())));
        for key in keys {
            let job = self.mesh_job(key);
            self.pending.insert(key, job.generation);
            self.mesher.submit(job);
        }
    }

    fn mesh_job(&mut self, key @ (cx, cz): (i32, i32)) -> Job {
        self.next_generation += 1;
        let get = |k| self.blocks.get(&k).cloned();
        Job {
            key,
            generation: self.next_generation,
            center: self.blocks[&key].clone(),
            neighbors: AROUND.map(|(dx, dz)| get((cx + dx, cz + dz))),
            biomes: std::array::from_fn(|i| {
                let (dx, dz) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
                self.biomes.get(&(cx + dx, cz + dz)).cloned()
            }),
        }
    }

    /// Remesh the chunks of block changes now, dispatch the other dirty chunks to the workers and upload
    /// finished meshes. Offscreen renderers wait for every mesh, so a frame shows the whole world.
    fn flush_meshes(&mut self, cam: &Camera) {
        for key in std::mem::take(&mut self.urgent) {
            self.dirty.remove(&key);
            if self.blocks.contains_key(&key) {
                let job = self.mesh_job(key);
                self.pending.insert(key, job.generation);
                let done = job.run(&self.atlas);
                self.finish_mesh(done);
            }
        }
        let near = (
            (cam.pos[0] / 16.0).floor() as i32,
            (cam.pos[2] / 16.0).floor() as i32,
        );
        let wait = matches!(self.target, Target::Offscreen { .. });
        let start = std::time::Instant::now();
        let mut uploaded = false;
        loop {
            if !self.dirty.is_empty() {
                self.dispatch_meshes(near);
            }
            let done = if wait {
                if self.pending.is_empty() {
                    return;
                }
                self.mesher.recv()
            } else if !uploaded || start.elapsed() < UPLOAD_BUDGET {
                let Some(done) = self.mesher.try_recv() else {
                    return;
                };
                done
            } else {
                return;
            };
            uploaded = true;
            self.finish_mesh(done);
        }
    }

    /// Upload a finished mesh unless its chunk was removed or queued again since.
    fn finish_mesh(&mut self, done: Done) {
        if self.pending.get(&done.key) != Some(&done.generation) {
            return;
        }
        self.pending.remove(&done.key);
        let (cx, cz) = done.key;
        let m = done.meshes;
        let origin = [cx as f32 * 16.0, 0.0, cz as f32 * 16.0];
        let mesh = ChunkMesh {
            opaque: self.upload_mesh(m.opaque, origin),
            water: self.upload_mesh(m.water, origin),
            light: m.light,
        };
        self.meshes.insert(done.key, mesh);
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
        for key in draws.iter().filter_map(|d| d.skin) {
            self.entity_skins.touch(key);
        }
        let mesh = entity::build(draws, &self.entity_skins, |p| self.light_at(p));
        self.entity_mesh = self.upload_mesh(mesh, [0.0; 3]);
        let items = self.upload_items(draws);
        let mut glint = entity::build_glint(draws);
        let base = glint.vertices.len() as u32;
        glint.vertices.extend(items.vertices);
        glint.indices.extend(items.indices.iter().map(|i| i + base));
        if !glint.is_empty() {
            self.load_texture(entity::GLINT_TEXTURE);
        }
        self.glint_mesh = self.upload_mesh(glint, [0.0; 3]);
        self.fires.clear();
        self.fires.extend(
            draws
                .iter()
                .filter(|d| d.burning)
                .map(|d| entity::fire::Fire {
                    pos: d.pos,
                    size: d.size,
                }),
        );
    }

    /// Held and dropped items, each mesh with the texture it samples; leaves the world group bound.
    fn draw_items<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        for (tex, m) in &self.item_meshes {
            let group = tex
                .as_ref()
                .map_or(&self.world_group, |t| &self.textures[t]);
            pass.set_bind_group(0, group, &[]);
            draw_mesh(pass, m);
        }
        pass.set_bind_group(0, &self.world_group, &[]);
    }

    /// Meshes the held and dropped items into `item_meshes`; returns their glint layer.
    fn upload_items(&mut self, draws: &[EntityDraw]) -> MeshData {
        self.resolve_items(draws);
        self.load_frames(draws);
        let frames = self.frames.take().unwrap_or_default();
        let mut items = entity::item::build(draws, &self.item_models, |p| self.light_at(p));
        entity::hanging::build(&mut items, draws, &frames, &self.item_models, |p| {
            self.light_at(p)
        });
        self.frames = Some(frames);
        self.item_meshes.clear();
        let layers = std::iter::once((None, items.atlas)).chain(
            items
                .sprites
                .into_iter()
                .map(|((_, path), m)| (Some(path), m)),
        );
        for (tex, mesh) in layers {
            if let Some(tex) = &tex {
                // A map not yet received has no texture and is not drawn.
                if tex.starts_with("map/") && !self.textures.contains_key(tex) {
                    continue;
                }
                self.load_texture(tex);
            }
            if let Some(m) = self.upload_mesh(mesh, [0.0; 3]) {
                self.item_meshes.push((tex, m));
            }
        }
        self.flash_mesh = self.upload_mesh(items.flash, [0.0; 3]);
        items.glint
    }

    /// Reads the item frame models when a frame is first drawn.
    fn load_frames(&mut self, draws: &[EntityDraw]) {
        if self.frames.is_some() || !draws.iter().any(entity::hanging::is_frame) {
            return;
        }
        let frames = self.source.as_mut().map(|source| {
            let mut read = |p: &str| source.read(&format!("assets/minecraft/{p}"));
            entity::hanging::Frames::load(&mut read)
        });
        self.frames = Some(frames.unwrap_or_default());
    }

    /// `MapData.updateMapData` of S34: the `columns` by `rows` colour bytes at `(x, z)`, then
    /// `MapRenderer.updateTexture`.
    pub fn map_update(&mut self, id: i32, area: [usize; 4], data: &[u8]) {
        let [x, z, columns, rows] = area;
        let colors = self.maps.entry(id).or_insert_with(|| vec![0; 128 * 128]);
        for i in 0..columns {
            for j in 0..rows {
                if let (Some(&c), Some(slot)) = (
                    data.get(i + j * columns),
                    colors.get_mut(x + i + (z + j) * 128),
                ) {
                    *slot = c;
                }
            }
        }
        let rgba = colors
            .iter()
            .flat_map(|&c| entity::hanging::map_pixel(c))
            .collect();
        let image = Image {
            width: 128,
            height: 128,
            rgba,
        };
        self.insert_texture(&format!("map/{id}"), &image);
    }

    /// Forgets every received map, as a new server's map ids start again.
    pub fn clear_maps(&mut self) {
        self.maps.clear();
        self.textures.retain(|k, _| !k.starts_with("map/"));
    }

    /// Hands over the received map colours, for the renderer that replaces this one.
    pub fn take_maps(&mut self) -> HashMap<i32, Vec<u8>> {
        std::mem::take(&mut self.maps)
    }

    /// Uploads maps taken from an earlier renderer.
    pub fn restore_maps(&mut self, maps: HashMap<i32, Vec<u8>>) {
        for (id, colors) in maps {
            self.map_update(id, [0, 0, 128, 128], &colors);
        }
    }

    /// Resolves the model of every held and dropped item that is new.
    fn resolve_items(&mut self, draws: &[EntityDraw]) {
        for key in draws
            .iter()
            .flat_map(|d| entity::item::wanted(d).chain(entity::hanging::wanted(d)))
        {
            if self.item_models.contains_key(&key) {
                continue;
            }
            let model = self.source.as_mut().and_then(|source| {
                let mut read = |p: &str| source.read(&format!("assets/minecraft/{p}"));
                entity::item::resolve(&mut read, &self.atlas, key.0, key.1)
            });
            self.item_models.insert(key, model);
        }
    }

    pub fn set_on_fire(&mut self, on_fire: bool) {
        self.on_fire = on_fire;
    }

    /// Stores the decoded skin of player `key` and uploads its slot.
    pub fn set_player_skin(&mut self, key: u128, img: &Image) {
        let (row, rgba) = self.entity_skins.set_player(key, img);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.entity_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: row, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(Skins::width() * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: Skins::width(),
                height: entity::SLOT,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Stores the decoded cape of player `key` and uploads its slot.
    pub fn set_player_cape(&mut self, key: u128, img: &Image) {
        let (row, rgba) = self.entity_skins.set_cape(key, img);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.entity_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: row, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(Skins::width() * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: Skins::width(),
                height: entity::SLOT,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Whether the cape of player `key` is stored.
    pub fn has_player_cape(&self, key: u128) -> bool {
        self.entity_skins.cape(key).is_some()
    }

    /// Whether the skin of player `key` is stored; a replaced one has to be sent again.
    pub fn has_player_skin(&self, key: u128) -> bool {
        self.entity_skins.player(key).is_some()
    }

    /// `Entity.getBrightnessForRender`: `[sky, block]` light (`0..=1`) in the block at `p`, 0 when its chunk is not loaded.
    pub fn light_at(&self, p: [f64; 3]) -> [f32; 2] {
        let [x, y, z] = p.map(|v| v.floor() as i32);
        self.meshes
            .get(&(x >> 4, z >> 4))
            .and_then(|m| m.light.as_ref())
            .map_or([0.0; 2], |l| {
                let (s, b) = l.get(x & 15, y, z & 15);
                [f32::from(s) / 15.0, f32::from(b) / 15.0]
            })
    }

    /// Draw one frame to the window: the world, then `overlay` in order. Offscreen renderers draw into their texture.
    pub fn render(&mut self, cam: &Camera, overlay: &[Quad]) {
        self.flush_meshes(cam);
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

    /// `PostChain.close`, then `GameRenderer.loadShader` when `name` (a jar path such as
    /// `shaders/post/notch.json`) is given: a loaded effect turns effects on, a failed one off.
    pub fn set_post(&mut self, name: Option<&str>) -> Result<(), String> {
        self.post = None;
        let Some(name) = name else { return Ok(()) };
        let format = match &self.target {
            Target::Window { config, .. } => config.format,
            Target::Offscreen { .. } => OFFSCREEN,
        };
        let size = self.size();
        let post = match &mut self.source {
            Some(source) => {
                Post::load(&self.device, &self.queue, source, name, format, DEPTH, size)
            }
            None => Err("no client assets".to_owned()),
        };
        self.post_on = post.is_ok();
        self.post = Some(post?);
        Ok(())
    }

    /// F4, `GameRenderer.switchUseShader`: whether a loaded effect runs.
    pub fn toggle_post(&mut self) {
        self.post_on = !self.post_on;
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
        self.register_image(name, &image);
    }

    pub fn register_image(&mut self, name: &str, image: &Image) {
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

    /// Highest block that stops rain in a loaded column, as `World.getPrecipitationHeight`, and the column's biome id.
    fn rain_top(&self, x: i32, z: i32) -> Option<(i32, u8)> {
        let blocks = self.blocks.get(&(x >> 4, z >> 4))?;
        let ground =
            precipitation_height(blocks, x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
        Some((ground, self.biome_at(x, z)?))
    }

    /// Biome id of a loaded column.
    fn biome_at(&self, x: i32, z: i32) -> Option<u8> {
        let biomes = self.biomes.get(&(x >> 4, z >> 4))?;
        Some(biomes[(z.rem_euclid(16) as usize) << 4 | x.rem_euclid(16) as usize])
    }

    /// `World.getSkyColor` reads the temperature at the camera entity's block; plains when unloaded.
    fn sky_temperature(&self, cam: &Camera) -> f32 {
        let [x, y, z] = [cam.pos[0], cam.pos[1] - EYE_HEIGHT, cam.pos[2]].map(|c| c.floor() as i32);
        self.biome_at(x, z)
            .map_or(PLAINS_TEMPERATURE, |b| temperature(b, x, y, z))
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
            HeldKind::Item { id, damage, dye } => {
                if let Some(flat) = self.flat_item(id, damage, dye) {
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

    /// Meshes of the footprint quads and of each item's icon quads, with their textures uploaded.
    fn textured_particles(
        &mut self,
        footprint: Vec<Vertex>,
        items: Vec<((i16, i16), Vec<Vertex>)>,
        offset: [f32; 3],
    ) -> Vec<(String, GpuMesh)> {
        let mut out = Vec::new();
        let mut layers = vec![(Some(FOOTPRINT_SHEET.to_string()), footprint)];
        for ((id, damage), quads) in items {
            layers.push((self.item_texture(id, damage), quads));
        }
        for (tex, quads) in layers {
            let Some(tex) = tex else { continue };
            if let Some(m) = self.upload_mesh(quad_mesh(quads), offset) {
                self.load_texture(&tex);
                out.push((tex, m));
            }
        }
        out
    }

    /// The `layer0` texture path of a flat item model, cached.
    fn item_texture(&mut self, id: i16, damage: i16) -> Option<String> {
        if let Some(t) = self.item_textures.get(&(id, damage)) {
            return t.clone();
        }
        let name = world::inventory::items::model(id, damage);
        let source = self.source.as_mut();
        let found = name.zip(source).and_then(|(name, source)| {
            let mut read = |p: &str| source.read(&format!("assets/minecraft/{p}"));
            hand::flat_model(&mut read, name).and_then(|m| m.0.into_iter().next())
        });
        self.item_textures.insert((id, damage), found.clone());
        found
    }

    /// The extruded sprite of a `builtin/generated` item model: every `layerN` texture tinted by
    /// `ItemColors` (`dye` is the leather `display.color`) and drawn over the previous one.
    fn flat_item(&mut self, id: i16, damage: i16, dye: Option<i32>) -> Option<Held> {
        let name = world::inventory::items::model(id, damage)?;
        let source = self.source.as_mut()?;
        let mut read = |p: &str| source.read(&format!("assets/minecraft/{p}"));
        let (textures, display) = hand::flat_model(&mut read, name)?;
        let layers: Vec<_> = textures
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let tint = world::inventory::items::tint(id, damage, dye, i);
                Some((decode_png(&read(t)?)?, tint))
            })
            .collect();
        let image = hand::composite(&layers)?;
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

    /// Draws each precipitation mesh that has a texture.
    fn draw_precipitation<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        layers: [(&'a Option<GpuMesh>, &'a Option<wgpu::BindGroup>); 2],
    ) {
        for (mesh, group) in layers {
            if let (Some(m), Some(group)) = (mesh, group) {
                pass.set_bind_group(0, group, &[]);
                pass.set_pipeline(&self.water_pipe);
                draw_mesh(pass, m);
            }
        }
    }

    /// Rain and snow meshes for this frame.
    fn rain_mesh(&self, env: &Environment, cam: &Camera) -> (Option<GpuMesh>, Option<GpuMesh>) {
        let feet = [cam.pos[0], cam.pos[1] - EYE_HEIGHT, cam.pos[2]];
        let (rain, snow) = build_precipitation(env, feet, |x, z| self.rain_top(x, z));
        (self.upload_geometry(&rain), self.upload_geometry(&snow))
    }

    /// Uploads the hand and its own projection, with the texture it samples.
    fn hand_mesh(
        &mut self,
        cam: &Camera,
        world: &Globals,
        sun_brightness: f32,
    ) -> Option<(GpuMesh, HandTex, Option<GpuMesh>)> {
        let (mut draw, tex) = self.hand_draw()?;
        let glint = self.hand.is_some_and(|h| h.glint && h.held.is_some());
        if glint {
            self.load_texture(entity::GLINT_TEXTURE);
        }
        let age = self.environment.ticks as f32 + self.environment.partial;
        // `renderEffect` draws two passes; a flat item's sprite uv covers 16 px of the atlas.
        let uv_scale = if matches!(tex, HandTex::Item) {
            entity::ITEM_UV_SCALE
        } else {
            1.0
        };
        let glint_vertices: Vec<Vertex> = if glint {
            (0..2)
                .flat_map(|pass| {
                    draw.vertices.iter().map(move |v| Vertex {
                        uv: entity::glint_uv_item(v.uv.map(|c| c * uv_scale), age * 50.0, pass),
                        color: entity::ITEM_GLINT_COLOR,
                        light: [1.0; 2],
                        ..*v
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let n = draw.vertices.len() as u32;
        let glint_indices: Vec<u32> = if glint {
            let second = draw.indices.iter().map(|i| i + n);
            draw.indices.iter().copied().chain(second).collect()
        } else {
            Vec::new()
        };
        let eye = cam.pos.map(|p| p.floor() as i32);
        let light = self
            .meshes
            .get(&(eye[0] >> 4, eye[2] >> 4))
            .and_then(|m| m.light.as_ref())
            .map_or([1.0, 0.0], |l| {
                let (s, b) = l.get(eye[0] & 15, eye[1], eye[2] & 15);
                [s as f32 / 15.0, b as f32 / 15.0]
            });
        for v in &mut draw.vertices {
            v.light = light;
        }
        let [w, h, ..] = world.screen;
        let globals = Globals {
            view_proj: mul(
                projection(cam.hand_fov_deg, w / h),
                mul(
                    hurt_matrix(cam.hurt_roll),
                    mul(bob_matrix(cam.bob), draw.model),
                ),
            ),
            eye: [0.0, 0.0, 0.0, self.environment.gamma],
            fog_color: world.fog_color,
            fog: [1.0e6, 2.0e6, sun_brightness, 1.5],
            screen: world.screen,
        };
        self.queue
            .write_buffer(&self.hand_globals, 0, bytemuck::bytes_of(&globals));
        let glint = if glint {
            self.upload_geometry(&SkyGeometry {
                vertices: glint_vertices,
                indices: glint_indices,
            })
        } else {
            None
        };
        let geometry = SkyGeometry {
            vertices: draw.vertices,
            indices: draw.indices,
        };
        Some((self.upload_geometry(&geometry)?, tex, glint))
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

    fn draw_fire_world<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, fire: &'a FireFrame) {
        pass.set_pipeline(&self.fire_pipe);
        for (i, mesh) in fire.meshes.iter().enumerate() {
            if let Some(mesh) = mesh {
                pass.set_bind_group(0, &self.textures[entity::fire::TEXTURES[i]], &[]);
                draw_mesh(pass, mesh);
            }
        }
    }

    fn draw_fire_overlay<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, fire: &'a FireFrame) {
        if let (Some(buf), Some(quads)) = (&fire.buf, &fire.overlay) {
            self.draw_gui(pass, buf, quads);
        }
    }

    /// Fire meshes, first-person overlay quads and their vertex buffer for this frame.
    fn prepare_fire(&mut self, cam: &Camera, w: u32, h: u32) -> FireFrame {
        if (self.on_fire || !self.fires.is_empty()) && self.fire_frames[0] == 0 {
            self.load_fire_textures();
        }
        let fires = entity::fire::build(
            &self.fires,
            cam.yaw,
            self.environment.ticks,
            self.fire_frames,
        )
        .map(|m| self.upload_mesh(m, [0.0; 3]));
        let overlay = self.on_fire.then(|| {
            entity::fire::overlay(
                w as f32,
                h as f32,
                self.environment.ticks,
                self.fire_frames[1],
            )
        });
        let buf = overlay.as_ref().map(|quads| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("first-person fire"),
                    contents: bytemuck::cast_slice(
                        &quads.iter().flat_map(quad_vertices).collect::<Vec<_>>(),
                    ),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        FireFrame {
            meshes: fires,
            overlay,
            buf,
        }
    }

    /// Reads each fire texture's frame count (`height / width`) and uploads it.
    fn load_fire_textures(&mut self) {
        for (i, name) in entity::fire::TEXTURES.iter().enumerate() {
            let image = self
                .source
                .as_mut()
                .and_then(|s| s.read(&format!("assets/minecraft/{name}")))
                .and_then(|bytes| decode_png(&bytes));
            self.fire_frames[i] =
                image.map_or(1, |image| (image.height / image.width.max(1)).max(1) as u32);
            self.load_texture(name);
        }
    }

    fn draw(&mut self, view: &wgpu::TextureView, cam: &Camera, overlay: &[Quad]) {
        let (w, h) = self.size();
        let env = self.environment;
        self.advance_post(env.partial);
        let fire = self.prepare_fire(cam, w, h);
        let sky = env.sky_state(self.sky_temperature(cam), look_vector(cam));
        let globals = Self::globals(cam, &env, &sky, (w, h));
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let layers = build_sky(&env, &sky, cam.pos);
        let sky_solid = self.upload_geometry(&layers.solid);
        let sky_blend = self.upload_geometry(&layers.blend);
        let sky_add = self.upload_geometry(&layers.additive);
        let clouds = (self.env_tex.has_clouds && env.fluid.is_none())
            .then(|| build_clouds(&env, &sky, cam.pos, &self.env_tex.clouds))
            .and_then(|g| self.upload_geometry(&g));

        let bolt_mesh = (!self.bolts.is_empty())
            .then(|| crate::lightning::build(&self.bolts, cam.pos))
            .and_then(|d| self.upload_mesh(d, cam.pos.map(|c| c as f32)));
        let border_mesh = self
            .border
            .map(|b| crate::border::quads(&b, cam.pos))
            .and_then(|v| self.upload_mesh(quad_mesh(v), cam.pos.map(|c| c as f32)));
        if border_mesh.is_some() {
            self.load_texture(crate::border::TEXTURE);
        }
        let (rain, snow) = self.rain_mesh(&env, cam);
        let hand = self.hand_mesh(cam, &globals, sky.sun_brightness);
        let offset = cam.pos.map(|c| c as f32);
        let crack_meshes = self.crack_meshes();
        let quads = std::mem::take(&mut self.particles);
        let particle_block = self.upload_mesh(quad_mesh(quads.block), offset);
        let particle_sheet = self.upload_mesh(quad_mesh(quads.sheet), offset);
        let particle_explosion = self.upload_mesh(quad_mesh(quads.explosion), offset);
        if particle_explosion.is_some() {
            self.load_texture(EXPLOSION_SHEET);
        }
        if particle_sheet.is_some() {
            self.load_texture(PARTICLE_SHEET);
        }
        let particle_textured = self.textured_particles(quads.footprint, quads.items, offset);
        let curse = entity::guardian::build(&quads.curses, &self.entity_skins);
        let particle_curse = self.upload_mesh(curse, offset);

        overlay.iter().for_each(|q| self.load_texture(&q.tex));
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

        let world = self.running_post().map_or(view, Post::main);
        // Chunk positions are baked in world space as f32; chunks come from the camera's own area, so precision holds.
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: world,
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
            [&sky_solid, &sky_blend]
                .into_iter()
                .flatten()
                .for_each(|m| draw_mesh(&mut pass, m));
            pass.set_pipeline(&self.sky_add_pipe);
            sky_add.iter().for_each(|m| draw_mesh(&mut pass, m));
            pass.set_bind_group(0, &self.world_group, &[]);
            pass.set_pipeline(&self.opaque_pipe);
            self.meshes
                .values()
                .filter_map(|m| m.opaque.as_ref())
                .for_each(|m| draw_mesh(&mut pass, m));
            if let Some(m) = &self.entity_mesh {
                pass.set_bind_group(0, &self.entity_group, &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            self.draw_items(&mut pass);
            self.draw_tiles(&mut pass);
            if let Some(m) = &self.glint_mesh {
                pass.set_pipeline(&self.hand_glint_pipe);
                pass.set_bind_group(0, &self.textures[entity::GLINT_TEXTURE], &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
                pass.set_pipeline(&self.opaque_pipe);
            }
            if let Some(m) = &self.flash_mesh {
                pass.set_bind_group(0, &self.env_group, &[]);
                pass.set_pipeline(&self.bolt_pipe);
                draw_mesh(&mut pass, m);
                pass.set_pipeline(&self.opaque_pipe);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            self.draw_fire_world(&mut pass, &fire);
            // `renderMiningProgress` follows the entities.
            pass.set_pipeline(&self.crack_pipe);
            for (tex, m) in &crack_meshes {
                pass.set_bind_group(0, &self.textures[tex.as_str()], &[]);
                draw_mesh(&mut pass, m);
            }
            pass.set_bind_group(0, &self.world_group, &[]);
            if let Some(m) = &bolt_mesh {
                pass.set_bind_group(0, &self.env_group, &[]);
                pass.set_pipeline(&self.bolt_pipe);
                draw_mesh(&mut pass, m);
                pass.set_pipeline(&self.opaque_pipe);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            // `EffectRenderer.renderParticles` runs after entities and before the translucent layer.
            pass.set_pipeline(&self.water_pipe);
            particle_block.iter().for_each(|m| draw_mesh(&mut pass, m));
            if let Some(m) = &particle_sheet {
                pass.set_bind_group(0, &self.textures[PARTICLE_SHEET], &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            if let Some(m) = &particle_explosion {
                pass.set_bind_group(0, &self.textures[EXPLOSION_SHEET], &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            for (tex, m) in &particle_textured {
                pass.set_bind_group(0, &self.textures[tex], &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            if let Some(m) = &particle_curse {
                pass.set_bind_group(0, &self.entity_group, &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            pass.set_pipeline(&self.translucent_pipe);
            self.meshes
                .values()
                .filter_map(|m| m.water.as_ref())
                .for_each(|m| draw_mesh(&mut pass, m));
            // `renderWorldBorder`: additive, no depth write, no culling, polygon offset (the `fs_crack` pipeline).
            if let Some(m) = &border_mesh {
                pass.set_pipeline(&self.border_pipe);
                pass.set_bind_group(0, &self.textures[crate::border::TEXTURE], &[]);
                draw_mesh(&mut pass, m);
                pass.set_bind_group(0, &self.world_group, &[]);
            }
            if let Some(buf) = &sel_buf {
                pass.set_bind_group(0, &self.world_group, &[]);
                pass.set_pipeline(&self.line_pipe);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..SELECTION_VERTICES, 0..1);
            }
            if let Some(m) = &clouds {
                pass.set_bind_group(0, &self.env_group, &[]);
                // Fancy clouds draw twice: depth only, then color.
                let pipes = [&self.cloud_depth_pipe, &self.cloud_pipe];
                for pipe in &pipes[usize::from(env.clouds != 2)..] {
                    pass.set_pipeline(pipe);
                    draw_mesh(&mut pass, m);
                }
            }
            self.draw_precipitation(
                &mut pass,
                [(&rain, &self.rain_group), (&snow, &self.snow_group)],
            );
        }
        self.draw_hand_and_gui(
            &mut enc,
            [world, view],
            hand.as_ref(),
            &fire,
            gui_buf.as_ref(),
            overlay,
        );
        self.queue.submit([enc.finish()]);
    }

    fn globals(cam: &Camera, env: &Environment, sky: &SkyState, (w, h): (u32, u32)) -> Globals {
        Globals {
            view_proj: view_proj(cam, w as f32 / h as f32),
            eye: [
                cam.pos[0] as f32,
                cam.pos[1] as f32,
                cam.pos[2] as f32,
                env.gamma,
            ],
            fog_color: [sky.fog[0], sky.fog[1], sky.fog[2], 1.0],
            // Torch flicker is a slow random walk around 0 in vanilla, so the factor stays near 1.5.
            fog: match sky.fog_density {
                // A negative start tells `fs_world` to use `exp(-density * d)`.
                Some(d) => [-d, d, sky.sun_brightness, 1.5],
                None => [sky.fog_start, sky.fog_end, sky.sun_brightness, 1.5],
            },
            screen: [w as f32, h as f32, env.night_vision, 0.0],
        }
    }

    fn draw_hand_and_gui(
        &self,
        enc: &mut wgpu::CommandEncoder,
        // The world target (`minecraft:main` while an effect runs), then the window.
        [world, view]: [&wgpu::TextureView; 2],
        hand: Option<&(GpuMesh, HandTex, Option<GpuMesh>)>,
        fire: &FireFrame,
        gui_buf: Option<&wgpu::Buffer>,
        overlay: &[Quad],
    ) {
        // Vanilla draws the hand on a cleared depth buffer, then the GUI.
        let mut pass = overlay_pass(enc, world, &self.depth);
        if let Some(hand) = hand {
            self.draw_hand(&mut pass, hand);
        }
        self.draw_fire_overlay(&mut pass, fire);
        // `updateCameraAndRender`: the effect runs after the hand and before the GUI.
        if let Some(post) = self.running_post() {
            drop(pass);
            post.process(&self.queue, enc);
            pass = overlay_pass(enc, view, &self.depth);
            post.blit(&mut pass);
        }
        if let Some(buf) = gui_buf {
            self.draw_gui(&mut pass, buf, overlay);
        }
    }

    fn draw_hand<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        (m, tex, glint): &'a (GpuMesh, HandTex, Option<GpuMesh>),
    ) {
        pass.set_bind_group(0, self.hand_group(*tex), &[]);
        pass.set_pipeline(&self.hand_pipe);
        draw_mesh(pass, m);
        if let (Some(g), Some(group)) = (glint, &self.hand_glint_group) {
            pass.set_bind_group(0, group, &[]);
            pass.set_pipeline(&self.hand_glint_pipe);
            draw_mesh(pass, g);
        }
    }

    /// The loaded effect while it runs (`useShader`).
    fn running_post(&self) -> Option<&Post> {
        self.post.as_ref().filter(|_| self.post_on)
    }

    /// The effect's `Time` step, once per frame while it runs.
    fn advance_post(&mut self, partial: f32) {
        if let (Some(post), true) = (&mut self.post, self.post_on) {
            post.advance(partial);
        }
    }

    fn draw_gui<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        buf: &'a wgpu::Buffer,
        overlay: &[Quad],
    ) {
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
) -> [wgpu::RenderPipeline; 14] {
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
    // `LayerArmorBase.renderGlint` and `RenderItem.renderEffect`: `blendFunc(SRC_COLOR, ONE)`.
    let glint_blend = Some(wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Src,
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
                    // `fs_cloud` writing depth is the depth-only first pass of fancy clouds.
                    write_mask: if fs == "fs_cloud" && depth_write {
                        wgpu::ColorWrites::empty()
                    } else {
                        wgpu::ColorWrites::ALL
                    },
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
                depth_compare: if fs == "fs_glint" {
                    // `depthFunc(GL_EQUAL)`: the glint lands only where the held item drew.
                    wgpu::CompareFunction::Equal
                } else if depth {
                    wgpu::CompareFunction::LessEqual
                } else {
                    wgpu::CompareFunction::Always
                },
                stencil: Default::default(),
                // `glPolygonOffset(-3, -3)` keeps the crack overlay in front of the block face.
                bias: if fs == "fs_crack" {
                    wgpu::DepthBiasState {
                        constant: -3,
                        slope_scale: -3.0,
                        clamp: 0.0,
                    }
                } else {
                    Default::default()
                },
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
    let translucent_pipe = pipe(
        "vs_world",
        "fs_world",
        stride,
        &world_attrs,
        alpha,
        false,
        true,
        true,
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
    // `RenderLightningBolt`: additive, depth tested, no lighting.
    let bolt_pipe = pipe(
        "vs_world",
        "fs_sky",
        stride,
        &world_attrs,
        additive,
        false,
        true,
        false,
    );
    let hand_glint_pipe = pipe(
        "vs_world",
        "fs_glint",
        stride,
        &world_attrs,
        glint_blend,
        false,
        true,
        false,
    );
    let cloud_pipe = pipe(
        "vs_world",
        "fs_cloud",
        stride,
        &world_attrs,
        alpha,
        false,
        true,
        false,
    );
    let cloud_depth_pipe = pipe(
        "vs_world",
        "fs_cloud",
        stride,
        &world_attrs,
        None,
        true,
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
    let crack_pipe = pipe(
        "vs_world",
        "fs_crack",
        stride,
        &world_attrs,
        Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        false,
        true,
        true,
    );
    let fire_pipe = pipe(
        "vs_world",
        "fs_fire",
        stride,
        &world_attrs,
        None,
        true,
        true,
        false,
    );
    let border_pipe = pipe(
        "vs_world",
        "fs_crack",
        stride,
        &world_attrs,
        additive,
        false,
        true,
        false,
    );
    [
        opaque_pipe,
        water_pipe,
        translucent_pipe,
        sky_pipe,
        sky_add_pipe,
        cloud_pipe,
        cloud_depth_pipe,
        hand_pipe,
        line_pipe,
        crack_pipe,
        bolt_pipe,
        fire_pipe,
        hand_glint_pipe,
        border_pipe,
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

mod block_entities;

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

/// A pass that keeps the color of `view` and clears the depth buffer: the hand, then the GUI.
fn overlay_pass<'e>(
    enc: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    depth: &wgpu::TextureView,
) -> wgpu::RenderPass<'e> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}

pub(crate) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    create_texture(device, queue, w, h, rgba).create_view(&Default::default())
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    )
}

/// `Entity.getLook`: the unit vector the camera faces.
fn look_vector(cam: &Camera) -> [f32; 3] {
    let (yaw, pitch) = (cam.yaw.to_radians(), cam.pitch.to_radians());
    [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ]
}

/// Column-major view-projection (OpenGL-style camera math, wgpu 0..1 depth).
pub fn view_proj(cam: &Camera, aspect: f32) -> [[f32; 4]; 4] {
    let yaw = cam.yaw.to_radians();
    let fwd = look_vector(cam);
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
        mul(hurt_matrix(cam.hurt_roll), mul(bob_matrix(cam.bob), view)),
    )
}

/// `rotatef(roll, 0, 0, 1)` of `applyHurtCam`.
fn hurt_matrix(roll: f32) -> [[f32; 4]; 4] {
    let (s, c) = roll.to_radians().sin_cos();
    [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
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

/// Per-frame fire geometry: world billboards, first-person quads and their vertex buffer.
struct FireFrame {
    meshes: [Option<GpuMesh>; 2],
    overlay: Option<[Quad; 2]>,
    buf: Option<wgpu::Buffer>,
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
