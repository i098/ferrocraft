//! The renderer side of `entity/shadow.rs`: settings and world lookups in, one shadow mesh out.

use super::*;

impl Renderer {
    /// Replaces the shadow setting (`GameSettings.entityShadows`).
    pub fn set_entity_shadows(&mut self, on: bool) {
        self.entity_shadows = on;
    }

    /// The interpolated view entity's feet, before the third-person camera offset (shadows and end portals).
    pub fn set_shadow_view(&mut self, pos: Option<[f64; 3]>) {
        self.shadow_view = pos;
    }

    /// The view entity's feet: the engine's value, else the first-person camera below the eye.
    pub(super) fn view_feet(&self, cam: &Camera) -> [f64; 3] {
        self.shadow_view
            .unwrap_or([cam.pos[0], cam.pos[1] - EYE_HEIGHT, cam.pos[2]])
    }

    /// Block state at a world position; air outside the world and in unloaded chunks.
    fn block_state(&self, x: i32, y: i32, z: i32) -> u16 {
        if !(0..HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.blocks.get(&(x >> 4, z >> 4)).map_or(0, |b| {
            b[index((x & 15) as usize, y as usize, (z & 15) as usize)]
        })
    }

    /// `[sky, block]` light levels `0..=15` of a block.
    pub(super) fn light_levels(&self, x: i32, y: i32, z: i32) -> [i32; 2] {
        self.light_at([x as f64, y as f64, z as f64])
            .map(|l| (l * 15.0).round() as i32)
    }

    /// `World.skylightSubtracted` of this frame.
    pub(super) fn skylight_subtracted(&self) -> i32 {
        let env = self.environment;
        entity::shadow::skylight_subtracted(env.world_time, env.rain, env.thunder)
    }

    /// The shadow blobs of this frame's entities (`Render.renderShadow`); `None` when off or none are cast.
    pub(super) fn prepare_shadows(&mut self, cam: &Camera) -> Option<GpuMesh> {
        if !self.entity_shadows || self.shadows.is_empty() {
            return None;
        }
        self.load_texture(entity::shadow::TEXTURE);
        let subtracted = self.skylight_subtracted();
        let view = self.view_feet(cam);
        let mesh = entity::shadow::build(
            &self.shadows,
            view,
            |x, y, z| self.block_state(x, y, z),
            |x, y, z| self.light_levels(x, y, z),
            subtracted,
        );
        self.upload_mesh(mesh, [0.0; 3])
    }
}
