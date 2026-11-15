//! The renderer side of `blockent.rs`: server data in, per-frame meshes out.

use super::*;
use crate::blockent::{
    banner_image, Banner, Beams, Skull, View, WornHead, BEAM_TEXTURE, PORTAL_TEXTURE, SKY_TEXTURE,
};

impl Renderer {
    /// `S24 BlockAction` (`event` 1 and `value` = viewers) opens or closes the lid of a chest.
    pub fn block_action(&mut self, pos: [i32; 3], block: u16, event: u8, value: u8) {
        self.block_entities.block_action(pos, block, event, value);
    }

    /// The four lines of a sign (`S33 UpdateSign`, `S35 UpdateBlockEntity` action 9) as plain
    /// text with `§` codes.
    pub fn set_sign_text(&mut self, pos: [i32; 3], lines: [String; 4]) {
        self.block_entities.set_sign(pos, lines);
    }

    /// `SkullType`, `Rot` and the owner skin key of a skull (`S35` action 4).
    pub fn set_skull(&mut self, pos: [i32; 3], skull: Skull) {
        self.block_entities.set_skull(pos, skull);
    }

    /// `Base` and `Patterns` of a banner (`S35` action 6).
    pub fn set_banner(&mut self, pos: [i32; 3], banner: Banner) {
        self.block_entities.set_banner(pos, banner);
    }

    /// The entity, `Delay` and `MinSpawnDelay` of a mob spawner (`S35` action 1).
    pub fn set_spawner(&mut self, pos: [i32; 3], entity: &str, delay: i16, min_delay: Option<i32>) {
        self.block_entities
            .set_spawner(pos, entity, delay, min_delay);
    }

    /// One client tick (20 Hz) of the chest lids and enchanting table books; `players` are all player positions.
    pub fn tick_block_entities(&mut self, players: &[[f64; 3]]) {
        self.block_entities.tick(players);
    }

    /// One client tick of every mob spawner (`MobSpawnerBaseLogic.updateSpawner`): turns the
    /// display mob of the activated ones and returns their smoke and flame positions.
    pub fn spawner_particles(
        &mut self,
        players: &[[f64; 3]],
        rng: &mut crate::jrandom::JavaRandom,
    ) -> Vec<[f64; 3]> {
        self.block_entities.tick_spawners(players, rng)
    }

    /// Meshes the block entities in range for the next frames. `world_time` is the total world
    /// time in ticks, `partial` the tick fraction.
    pub fn set_block_entities(&mut self, eye: [f64; 3], world_time: i64, partial: f32) {
        let missing: Vec<(String, Banner)> = self
            .block_entities
            .banners_in_range(eye)
            .filter(|(key, _)| !self.textures.contains_key(key))
            .map(|(key, b)| (key, b.clone()))
            .collect();
        for (key, banner) in missing {
            let image = self.source.as_mut().and_then(|source| {
                banner_image(
                    &mut |name| {
                        let bytes =
                            source.read(&format!("assets/minecraft/textures/entity/{name}.png"))?;
                        decode_png(&bytes)
                    },
                    &banner,
                )
            });
            let white = Image {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            };
            self.register_image(&key, image.as_ref().unwrap_or(&white));
        }
        let view = View {
            eye,
            world_time,
            partial,
        };
        let batches =
            self.block_entities
                .build(&view, &self.entity_skins, self.font.as_ref(), |p| {
                    self.light_at(p)
                });
        self.tile_meshes = self.upload_batches(batches);
        let blocks = &self.blocks;
        let beams = self.block_entities.beams(&view, |x, y, z| {
            let at = blocks.get(&(x.div_euclid(16), z.div_euclid(16)));
            let in_world = (0..HEIGHT as i32).contains(&y);
            at.filter(|_| in_world).map_or(0, |data| {
                data[index(
                    x.rem_euclid(16) as usize,
                    y as usize,
                    z.rem_euclid(16) as usize,
                )]
            })
        });
        self.set_beams(beams);
    }

    fn set_beams(&mut self, beams: Beams) {
        if !beams.inner.is_empty() {
            self.load_texture(BEAM_TEXTURE);
        }
        self.beam_meshes = [beams.inner, beams.outer].map(|m| self.upload_mesh(m, [0.0; 3]));
    }

    /// Draws the beacon beams (`TileEntityBeaconRenderer`) and restores the opaque pipeline.
    pub(super) fn draw_beams<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let [Some(inner), outer] = &self.beam_meshes else {
            return;
        };
        pass.set_bind_group(0, &self.textures[BEAM_TEXTURE], &[]);
        pass.set_pipeline(&self.beam_pipe);
        draw_mesh(pass, inner);
        if let Some(outer) = outer {
            pass.set_pipeline(&self.beam_glow_pipe);
            draw_mesh(pass, outer);
        }
        pass.set_pipeline(&self.opaque_pipe);
        pass.set_bind_group(0, &self.world_group, &[]);
    }

    /// Builds the end portal star field for this frame (`TileEntityEndPortalRenderer`).
    pub(super) fn prepare_portals(&mut self, cam: &Camera) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let portals = self.block_entities.portals(cam, self.view_feet(cam), now);
        for name in [SKY_TEXTURE, PORTAL_TEXTURE] {
            if !portals.sky.is_empty() {
                self.load_texture(name);
            }
        }
        self.portal_meshes = [portals.sky, portals.layers].map(|m| self.upload_mesh(m, [0.0; 3]));
    }

    /// Draws the end portal star field and restores the opaque pipeline.
    pub(super) fn draw_portals<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let [Some(sky), layers] = &self.portal_meshes else {
            return;
        };
        pass.set_bind_group(0, &self.textures[SKY_TEXTURE], &[]);
        pass.set_pipeline(&self.portal_sky_pipe);
        draw_mesh(pass, sky);
        if let Some(layers) = layers {
            pass.set_bind_group(0, &self.textures[PORTAL_TEXTURE], &[]);
            pass.set_pipeline(&self.portal_add_pipe);
            draw_mesh(pass, layers);
        }
        pass.set_pipeline(&self.opaque_pipe);
        pass.set_bind_group(0, &self.world_group, &[]);
    }

    /// Skulls on the heads of mobs and players for the next frames (`LayerCustomHead`).
    pub fn set_worn_heads(&mut self, heads: &[WornHead]) {
        let batches =
            crate::blockent::build_worn_heads(heads, &self.entity_skins, |p| self.light_at(p));
        self.head_meshes = self.upload_batches(batches);
    }

    /// The chests that minecarts carry, for the next frames (`MinecartRenderer`).
    pub(super) fn set_cart_chests(&mut self, draws: &[EntityDraw]) {
        let batches = crate::blockent::build_cart_chests(draws, |p| self.light_at(p));
        self.cart_meshes = self.upload_batches(batches);
    }

    fn upload_batches(&mut self, batches: Vec<(Tex, MeshData)>) -> Vec<(Tex, GpuMesh)> {
        let mut out = Vec::new();
        for (tex, mesh) in batches {
            if let Tex::Named(name) = &tex {
                self.load_texture(name);
            }
            if let Some(mesh) = self.upload_mesh(mesh, [0.0; 3]) {
                out.push((tex, mesh));
            }
        }
        out
    }

    /// Draws the block entity and worn head meshes, then restores the world bind group.
    pub(super) fn draw_tiles<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        for (tex, m) in self
            .tile_meshes
            .iter()
            .chain(&self.head_meshes)
            .chain(&self.cart_meshes)
        {
            pass.set_bind_group(0, self.tile_group(tex), &[]);
            draw_mesh(pass, m);
        }
        pass.set_bind_group(0, &self.world_group, &[]);
    }

    pub(super) fn tile_group(&self, tex: &Tex) -> &wgpu::BindGroup {
        match tex {
            Tex::Atlas => &self.entity_group,
            Tex::Named(name) => &self.textures[name],
        }
    }
}
