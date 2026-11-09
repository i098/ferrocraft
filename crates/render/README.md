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
