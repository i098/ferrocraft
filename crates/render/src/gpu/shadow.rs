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
        self.shadow_view.unwrap_or_else(|| {
            let eye = cam.eye();
            [eye[0], eye[1] - EYE_HEIGHT, eye[2]]
        })
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

    /// The fog light of this frame: the light at the viewer's block, smoothed over ticks.
    pub(super) fn fog_light_now(&mut self, cam: &Camera, env: &Environment) -> f32 {
        let eye = cam.eye();
        let [x, z] = [eye[0], eye[2]].map(|v| v.floor() as i32);
        // Vanilla EmptyChunk light is full sky light, not 0.
        let lit = self
            .meshes
            .get(&(x >> 4, z >> 4))
            .is_some_and(|m| m.light.is_some());
        let [sky, block] = if lit {
            self.light_levels(x, env.view_y.floor() as i32, z)
        } else {
            [15, 0]
        };
        // `WorldClient` computes `skylightSubtracted` once at creation, so the sky level is never reduced.
        let level = sky.max(block).clamp(0, 15);
        self.fog_light.advance(
            env.ticks,
            env.dimension.light_brightness(level as u8),
            env.render_distance_chunks,
        );
        self.fog_light.at(env.partial)
    }

    /// `[sky, block]` light levels `0..=15` of a block.
    fn light_levels(&self, x: i32, y: i32, z: i32) -> [i32; 2] {
        self.light_at([x as f64, y as f64, z as f64])
            .map(|l| (l * 15.0).round() as i32)
    }

    /// `Entity.getBrightness`: the light table value `0..=1` of the block at the eye `eye`. The client world never
    /// subtracts time or weather from the sky light. 0 light in unloaded chunks, like vanilla.
    pub fn eye_brightness(&self, eye: [f64; 3]) -> f32 {
        let light = self.light_levels(
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        );
        entity::shadow::eye_light_brightness(light)
    }

    /// The shadow blobs of this frame's entities (`Render.renderShadow`); `None` when off or none are cast.
    pub(super) fn prepare_shadows(&mut self, cam: &Camera) -> Option<GpuMesh> {
        if !self.entity_shadows || self.shadows.is_empty() {
            return None;
        }
        self.load_texture(entity::shadow::TEXTURE);
        let view = self.view_feet(cam);
        let mesh = entity::shadow::build(
            &self.shadows,
            view,
            |x, y, z| self.block_state(x, y, z),
            |x, y, z| self.light_levels(x, y, z),
        );
        self.upload_mesh(mesh, [0.0; 3])
    }
}
