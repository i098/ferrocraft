//! wgpu renderer for the 1.8.9 client: face-culled chunk meshes, sky color, crosshair, chat and debug text.

mod assets;
mod blocks;
mod gpu;
mod hand;
mod light;
mod mesh;
mod particles;
mod shapes;
mod sky;
mod text;
mod weather;

pub use hand::Hand;
pub use sky::Environment;

pub use assets::encode_png;
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

#[derive(Clone, Debug, Default)]
pub struct Hud {
    /// Chat lines, oldest first; `§` color codes are honored.
    pub chat: Vec<String>,
    /// `Some` while the chat box is open.
    pub chat_input: Option<String>,
    pub crosshair: bool,
    pub debug: Vec<String>,
}

/// One 16x16 column of blocks as the renderer reads it.
/// `world::chunk::Chunk` implements this once that module lands (block state id = id << 4 | meta).
pub trait ChunkBlocks {
    fn chunk_x(&self) -> i32;
    fn chunk_z(&self) -> i32;
    /// Block state id at chunk-local `x`, `z` in `0..16` and `y` in `0..256`.
    fn block(&self, x: usize, y: usize, z: usize) -> u16;
}
