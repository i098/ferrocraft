# gui

Owner: fc-p-hud

Vanilla 1.8.9 GUI for the client. It draws into a `toolkit::Gui`, and `render` draws the quads over the world.

- `toolkit`: `ScaledResolution` (`GuiScale`), `Gui` drawing (`drawRect`, `drawGradientRect`, `drawTexturedModalRect`, a matrix stack), `FontRenderer` text with `§` codes, widths, trim and wrap (`Metrics`), and `GuiButton` (`Button`).
- `hud`: `GuiIngame.renderGameOverlay`: vignette, hotbar and selected slot, crosshair, F3 screen, action bar, chat.
- `chat`: `GuiNewChat` (wrap, fade, 10 lines closed and 20 open, scroll, sent history) and the `GuiChat` input line.
- `debug`: the F3 left column (`GuiOverlayDebug`).

`tests/hud.rs` draws the HUD offscreen to a PNG. Set `FERROCRAFT_CLIENT_JAR` to the 1.8.9 client jar for real textures and the font.
