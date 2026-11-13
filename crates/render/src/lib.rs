//! wgpu renderer for the 1.8.9 client: face-culled chunk meshes, sky color and a 2D overlay of
//! textured quads that the `gui` crate lays out.

use std::borrow::Cow;

mod animation;
mod assets;
mod biome;
mod blockent;
mod blocks;
mod border;
pub use border::Border;
mod crack;
mod entity;
mod gpu;
mod hand;
mod jrandom;
pub use jrandom::JavaRandom;
mod lightning;
pub use lightning::Bolts;
mod glsl;
mod light;
mod mesh;
mod mesher;
mod mip;
mod particles;
mod post;
mod shapes;
mod sky;
mod weather;

pub use hand::{Hand, HeldKind, UseAction, Using};
pub use sky::{celestial_angle, Environment, Fluid};

pub use assets::{decode_png, encode_png, AssetSource, Font, Image};
pub use blockent::{Banner, Skull, WornHead};
pub use entity::{hanging, item};
pub use entity::{
    nametags, CapeSway, Class, EntityDraw, Look, NameTag, StandLook, TeamTag, Thrower,
};
pub use gpu::{view_proj, Renderer};
pub use particles::{Kind as ParticleKind, Particles, Quads as ParticleQuads};
pub use post::SHADERS;
pub use weather::{rain_particles, stops_rain};

/// Whether a block state is drawn as a full cube (lava included); everything the mesher skips
/// or blends (plants, doors, panes, water) is not.
pub fn is_solid(state: u16) -> bool {
    matches!(
        blocks::look(state).kind,
        blocks::Kind::Opaque | blocks::Kind::Cutout
    )
}

/// `Block.isViewBlocking`: an opaque full cube hides the camera inside it.
pub fn is_view_blocking(state: u16) -> bool {
    blocks::is_opaque_cube(state)
}

/// The texture file of the block `state` that fills the screen when the camera is inside it
/// (`BlockModelShapes.getTexture`, the particle icon).
pub fn block_texture(state: u16) -> String {
    format!(
        "textures/blocks/{}.png",
        blocks::look(state).tex[blocks::NORTH]
    )
}

/// Camera at the eye position; yaw and pitch are vanilla degrees (yaw 0 looks toward +z, pitch > 0 looks down).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov_deg: f32,
    /// View bobbing of `EntityRenderer.setupViewBobbing`: translate x, translate y, roll and
    /// pitch in degrees, applied in view space before the camera rotation.
    pub bob: [f32; 4],
    /// Roll in degrees of `EntityRenderer.applyHurtCam` (hit tilt and death roll), applied
    /// before the bobbing.
    pub hurt_roll: f32,
    /// Field of view of the hand: `getFOVModifier(partial, false)`, so without the option.
    pub hand_fov_deg: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            pos: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            fov_deg: 70.0,
            bob: [0.0; 4],
            hurt_roll: 0.0,
            hand_fov_deg: 70.0,
        }
    }
}

/// How a [`Quad`] mixes with the pixels under it: the vanilla `blendFunc` pairs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Blend {
    /// 770, 771: source alpha over the destination.
    #[default]
    Alpha,
    /// 775, 769: inverts the destination (the crosshair).
    Invert,
    /// 0, 769: darkens the destination by the source color (the vignette).
    Darken,
    /// 768, 1: adds the source color (the enchantment glint).
    Add,
}

/// One textured quad in window pixels, origin top-left. Corners run top-left, top-right,
/// bottom-right, bottom-left, so a transformed (rotated, sheared) rectangle stays one quad.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    /// Path under `assets/minecraft/`, such as `textures/gui/icons.png`; `""` is solid white.
    pub tex: Cow<'static, str>,
    pub blend: Blend,
    pub pos: [[f32; 2]; 4],
    /// Texture coordinates in `0..=1`.
    pub uv: [[f32; 2]; 4],
    /// ARGB tint of each corner (`drawGradientRect` uses two).
    pub color: [u32; 4],
}

/// One 16x16 column of blocks as the renderer reads it.
/// `world::chunk::Chunk` implements this once that module lands (block state id = id << 4 | meta).
pub trait ChunkBlocks {
    fn chunk_x(&self) -> i32;
    fn chunk_z(&self) -> i32;
    /// Block state id at chunk-local `x`, `z` in `0..16` and `y` in `0..256`.
    fn block(&self, x: usize, y: usize, z: usize) -> u16;
    /// Biome id at chunk-local `x`, `z`; plains (1) unless the source knows better.
    fn biome(&self, _x: usize, _z: usize) -> u8 {
        1
    }
}
