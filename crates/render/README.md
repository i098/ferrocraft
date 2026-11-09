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
`set_hand` takes `HeldKind::Block(state)` or `HeldKind::Item { id, damage }`. A `builtin/generated` item model
draws as the extruded sprite with its `firstperson` transform.
Particles: call `Particles::vertices(renderer.atlas(), yaw, pitch, cam.pos, partial)` and then `set_particles` before
each `render`. The quads are relative to that frame's camera.
Third person: the engine pushes the local player through `set_entities`. The renderer culls no entity.
Chunk meshing runs on 2 worker threads (`mesher.rs`, as vanilla `ChunkRenderDispatcher`), nearest chunk first, at most 100 outstanding jobs.
`render` uploads finished meshes for about 2 ms per frame. A `set_block` remeshes the changed chunk (and an edge neighbour) on the render thread in that frame; the eight surrounding chunks remesh on the workers because light crosses chunk edges and corners.
Offscreen renderers wait for every mesh, so `render_to_rgba` shows the whole world.
