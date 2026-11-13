# gui

Owner: fc-p-hud

Vanilla 1.8.9 GUI for the client. It draws into a `toolkit::Gui`, and `render` draws the quads over the world.

- `toolkit`: `ScaledResolution` (`GuiScale`), `Gui` drawing (`drawRect`, `drawGradientRect`, `drawTexturedModalRect`, a matrix stack), `FontRenderer` text with `§` codes, widths, trim and wrap (`Metrics`), and `GuiButton` (`Button`).
- `hud`: `GuiIngame.renderGameOverlay`: vignette, hotbar with items and the held-item name, crosshair, boss bar, player stats, XP or jump bar, F3 screen, action bar, title, sidebar, chat, tab list.
- `status`: `renderPlayerStats` (armor, hearts, food or mount hearts, air), `renderExpBar`, `renderHorseJumpBar`.
- `tablist`: `GuiPlayerTabOverlay`, including player heads. `scoreboard`: objectives, scores, teams and `renderScoreboard`. `title`: `displayTitle`. `bossbar`: `BossStatus`.
- `canvas`: `GuiCanvas`, the `screens::Canvas` over a `Gui`; textures from `Canvas::texture` wait in `pending_textures`.
- `chat`: `GuiNewChat` (wrap, fade, 10 lines closed and 20 open, scroll, sent history) and the `GuiChat` input line.
- `debug`: the F3 left column (`GuiOverlayDebug`).

`tests/hud.rs` draws the HUD offscreen to a PNG. Set `FERROCRAFT_CLIENT_JAR` to the 1.8.9 client jar for real textures and the font.
