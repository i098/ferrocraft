//! Uploading and drawing the meshes of one `particles::Layers`.

use super::*;
use crate::particles::Layers;

/// The meshes of one `particles::Layers`.
pub(super) struct ParticleMeshes {
    block: Option<GpuMesh>,
    sheet: Option<GpuMesh>,
    explosion: Option<GpuMesh>,
    textured: Vec<(String, GpuMesh)>,
}

/// Draws the meshes of one particle pass with the pipeline already set.
pub(super) fn draw_particles<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    textures: &'a HashMap<String, wgpu::BindGroup>,
    world_group: &'a wgpu::BindGroup,
    set: &'a ParticleMeshes,
) {
    set.block.iter().for_each(|m| draw_mesh(pass, m));
    let sheets = [
        (PARTICLE_SHEET, set.sheet.as_ref()),
        (EXPLOSION_SHEET, set.explosion.as_ref()),
    ]
    .into_iter()
    .filter_map(|(tex, m)| m.map(|m| (tex, m)));
    for (tex, m) in sheets.chain(set.textured.iter().map(|(tex, m)| (tex.as_str(), m))) {
        pass.set_bind_group(0, &textures[tex], &[]);
        draw_mesh(pass, m);
        pass.set_bind_group(0, world_group, &[]);
    }
}

impl Renderer {
    /// Meshes of one `Layers` with their textures uploaded.
    pub(super) fn upload_particle_layers(
        &mut self,
        layers: Layers,
        offset: [f32; 3],
    ) -> ParticleMeshes {
        let block = self.upload_mesh(quad_mesh(layers.block), offset);
        let sheet = self.upload_mesh(quad_mesh(layers.sheet), offset);
        let explosion = self.upload_mesh(quad_mesh(layers.explosion), offset);
        if explosion.is_some() {
            self.load_texture(EXPLOSION_SHEET);
        }
        if sheet.is_some() {
            self.load_texture(PARTICLE_SHEET);
        }
        let textured = self.textured_particles(layers.footprint, layers.items, offset);
        ParticleMeshes {
            block,
            sheet,
            explosion,
            textured,
        }
    }
}
