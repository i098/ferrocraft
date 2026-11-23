# render

Owner: fc-render

wgpu renderer for the 1.8.9 client: face-culled chunk meshes, block textures from the client jar
(flat colors without assets), the sky clear color, and a 2D overlay of textured `Quad`s that the
`gui` crate lays out.

Clear glass panes use the 1.8.9 blockstate models and their separate edge texture.
Without model assets, the renderer uses its box shapes.

Entry points: `Renderer::new` (window), `Renderer::new_offscreen` (tests, screenshots),
`upload_chunk`, `remove_chunk`, `set_block`, `set_selection`, `resize`, `render`, `render_to_rgba`, `font`.
Camera yaw and pitch use vanilla degrees; `Camera::pos` is the eye position.
A `Quad` names its texture by jar path (such as `textures/gui/widgets.png`); `""` is solid white.
`Blend` gives the vanilla blend modes (alpha, crosshair invert, vignette darken, glint add).
`register_texture(name, png)` adds a runtime texture (server icons) that quads name by `name`.
`set_player_skin(key, img)` stores the decoded skin of one player for the entity renderer. `has_player_skin(key)` tells whether it is still stored; the renderer replaces the least recently used skin when its slots are full.
`set_player_cape(key, img)` and `has_player_cape(key)` do the same for the cape.
`set_hand` takes `HeldKind::Block(state)` or `HeldKind::Item { id, damage }`. A `builtin/generated` item model
draws as the extruded sprite with its `firstperson` transform.
Particles: call `Particles::vertices(renderer.atlas(), yaw, pitch, cam.pos, partial, |p| renderer.light_at(p))` and then
`set_particles` before each `render`. The quads are relative to that frame's camera; each particle is lit by the world
light at its position like the 1.8.9 particle classes.
Fireworks: the engine calls `Particles::firework(pos, vel, explosions)` for a blast and `firework_trail(pos, vel)` each tick of rocket flight. It also calls `set_viewer(eye_pos)` each tick and plays the `take_sounds()` entries (blast, twinkle) itself.
Lightning: `Bolts` holds the client lightning entities (spawn from Spawn Global Entity, `tick` at 20 Hz); `set_lightning(bolts.shapes())` draws them, and `Environment::flash` is `Bolts::flash(partial)`.
Rain splashes: `rain_particles` returns the droplets (and lava smoke) one tick adds, plus the `ambient.weather.rain` sound it may play as a `WeatherSound` (event, position, volume, pitch), as vanilla `EntityRenderer.addRainParticles`; `RainClock` carries the client tick and the rain sound counter. The engine spawns the particles and plays the sound.
Food crumbs: the engine calls `Particles::eat_crumbs(count, feet, eye, (yaw, pitch), item)` for the local player's eating ticks and finish. For each remote player that holds food with the use flag set, it calls it on the ticks where `remote_crumbs_due(ticks_in_use)` is true.
Weather: precipitation falls as rain, snow, or nothing per column biome and height, as vanilla 1.8.9 `renderRainSnow` (radius 10 columns, 5 on Fast graphics); sky and fog colors follow the biome.
Fog: `Environment::sky_state(temperature, look)` blends the sunrise/sunset glow into the fog toward the sun, as vanilla `updateFogColor` (render distance 4 or more), then rain and thunder darken it.
Third person: the engine pushes the local player through `set_entities`. The renderer culls no entity.
Creeper bodies and charge shells share the 1.8.9 `ModelCreeper` pivots.
Entity shadows use `textures/misc/shadow.png`, each renderer's shadow size, and vanilla light, height, and distance fades.
`set_fancy_graphics` applies the Graphics option: Fast makes leaves solid cubes that hide shared faces.
`set_entity_shadows` applies the Entity Shadows option; vanilla does not gate these shadows on Fancy Graphics.
`set_shadow_view` supplies the view entity's feet, so the F5 camera offset does not change the distance fade.
Burning entities use animated, camera-facing fire layers, including when their bodies are invisible.
Invisible entities draw their bodies at alpha 0.15 (`Look::ghost`) when the viewer is a spectator or a team mate with see-friendly-invisibles; otherwise bodies are skipped.
`set_on_fire` draws the local fire overlay after the hand and before the HUD.
`set_debug_axes(on)` draws the F3 axes (`drawDebugAxes`) at the screen centre after the hand, on a cleared depth buffer; the engine sets it for the full F3 screen with the HUD shown.
The engine disables that overlay in third-person views and spectator mode.
Post effects: `set_post(Some("shaders/post/<name>.json"))` loads a `shaders/post` chain from the jar, as vanilla `PostChain`; the GLSL 1.20 programs are rewritten to 4.50 for naga (`glsl.rs`). The effect runs after the hand and before the GUI. A failed load returns `Err` and switches effects off; `toggle_post` is F4. `SHADERS` is the "Super Secret Settings..." cycle.
Chunk meshing runs on 2 worker threads (`mesher.rs`, as vanilla `ChunkRenderDispatcher`), nearest chunk first, at most 100 outstanding jobs.
With Smooth Lighting on, the sides of upper and lower slabs receive light and ambient occlusion per corner.
The top and bottom faces of these slabs still use flat lighting.
Chunk lighting uses no sky light in the Nether or the End, even when Smooth Lighting is off.
`render` uploads finished meshes for about 2 ms per frame. A `set_block` remeshes the changed chunk (and an edge neighbour) on the render thread in that frame; the eight surrounding chunks remesh on the workers because light crosses chunk edges and corners.
Plant models and their block damage overlays use the same offsets as the vanilla standard renderer.
These offsets depend on world X and Z, not block height.
Offscreen renderers wait for every mesh, so `render_to_rgba` shows the whole world.
Translucent quads (water) draw far to near, as vanilla `sortVertexData`; once the camera moves a block, the 15 nearest chunks are sorted again (`sort.rs`).
End portals draw the 16-layer star field of `TileEntityEndPortalRenderer` (`blockent/portal.rs`); the texture transform is applied per corner on the CPU and the camera is assumed to be at the view entity.
Item frames, paintings and dropped items draw through `set_entities` (`entity/hanging.rs`, `entity/item.rs`). A framed filled map shows the texture `map/{id}`.
Maps: S34 goes to `map_update(id, area, data)`, which merges the colours and uploads `map/{id}`. `clear_maps` runs on connect (map ids restart per server). Across a renderer rebuild, `take_maps` on the old renderer feeds `restore_maps` on the new one.
