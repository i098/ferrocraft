# render

Owner: fc-render

wgpu renderer for the 1.8.9 client: face-culled chunk meshes, block textures from the client jar
(flat colors without assets), sky clear color, crosshair, block selection outline, chat lines, chat input and debug lines
with the vanilla `ascii.png` font.

Entry points: `Renderer::new` (window), `Renderer::new_offscreen` (tests, screenshots),
`upload_chunk`, `remove_chunk`, `set_block`, `set_selection`, `resize`, `render`, `render_to_rgba`.
Camera yaw and pitch use vanilla degrees; `Camera::pos` is the eye position.
Text needs assets: without them the HUD draws no glyphs.
