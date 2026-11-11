# render

Owner: fc-render

wgpu renderer for the 1.8.9 client: face-culled chunk meshes, block textures from the client jar
(flat colors without assets), the sky clear color, and a 2D overlay of textured `Quad`s that the
`gui` crate lays out.

Entry points: `Renderer::new` (window), `Renderer::new_offscreen` (tests, screenshots),
`upload_chunk`, `remove_chunk`, `set_block`, `set_selection`, `resize`, `render`, `render_to_rgba`, `font`.
Camera yaw and pitch use vanilla degrees; `Camera::pos` is the eye position.
A `Quad` names its texture by jar path (such as `textures/gui/widgets.png`); `""` is solid white.
`Blend` gives the vanilla blend modes (alpha, crosshair invert, vignette darken, glint add).
`register_texture(name, png)` adds a runtime texture (server icons) that quads name by `name`.
`set_player_skin(key, img)` stores the decoded skin of one player for the entity renderer. `has_player_skin(key)` tells whether it is still stored; the renderer replaces the least recently used skin when its slots are full.
`set_hand` takes `HeldKind::Block(state)` or `HeldKind::Item { id, damage }`. A `builtin/generated` item model
draws as the extruded sprite with its `firstperson` transform.
Particles: call `Particles::vertices(renderer.atlas(), yaw, pitch, cam.pos, partial, |p| renderer.light_at(p))` and then
`set_particles` before each `render`. The quads are relative to that frame's camera; each particle is lit by the world
light at its position like the 1.8.9 particle classes.
Lightning: `Bolts` holds the client lightning entities (spawn from Spawn Global Entity, `tick` at 20 Hz); `set_lightning(bolts.shapes())` draws them, and `Environment::flash` is `Bolts::flash(partial)`.
Rain splashes: `rain_particles` returns the droplets (and lava smoke) one tick adds; the engine spawns them as particles.
Third person: the engine pushes the local player through `set_entities`. The renderer culls no entity.
Burning entities use animated, camera-facing fire layers, including when their bodies are invisible.
`set_on_fire` draws the local fire overlay after the hand and before the HUD.
The engine disables that overlay in third-person views and spectator mode.
Chunk meshing runs on 2 worker threads (`mesher.rs`, as vanilla `ChunkRenderDispatcher`), nearest chunk first, at most 100 outstanding jobs.
`render` uploads finished meshes for about 2 ms per frame. A `set_block` remeshes the changed chunk (and an edge neighbour) on the render thread in that frame; the eight surrounding chunks remesh on the workers because light crosses chunk edges and corners.
Offscreen renderers wait for every mesh, so `render_to_rgba` shows the whole world.
