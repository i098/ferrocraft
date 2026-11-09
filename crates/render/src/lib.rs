//! wgpu renderer for the 1.8.9 client: face-culled chunk meshes, sky color and a 2D overlay of
//! textured quads that the `gui` crate lays out.

use std::borrow::Cow;

mod assets;
mod blocks;
mod entity;
mod gpu;
mod hand;
mod light;
mod mesh;
mod particles;
mod shapes;
mod sky;
mod weather;

pub use hand::{Hand, HeldKind};
pub use sky::Environment;

pub use assets::{encode_png, AssetSource, Font};
pub use entity::{nametags, Class, EntityDraw};
pub use gpu::Renderer;
pub use particles::{Kind as ParticleKind, Particles, Quads as ParticleQuads};

/// Whether a block state is drawn as a full cube (lava included); everything the mesher skips
/// or blends (plants, doors, panes, water) is not.
pub fn is_solid(state: u16) -> bool {
    matches!(
        blocks::look(state).kind,
        blocks::Kind::Opaque | blocks::Kind::Cutout
    )
}

/// Camera at the eye position; yaw and pitch are vanilla degrees (yaw 0 looks toward +z, pitch > 0 looks down).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov_deg: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            pos: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            fov_deg: 70.0,
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
}
