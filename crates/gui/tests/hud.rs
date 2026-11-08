//! Draws the HUD over an empty world offscreen and writes a PNG (`HUD_PNG` overrides the path).
//! `FERROCRAFT_CLIENT_JAR` gives the 1.8.9 client jar for real textures and the font.

use ferrocraft_net_client::styled;
use gui::{debug, Frame, Gui, GuiScale, Hud, Metrics};
use render::{encode_png, Camera, Renderer};

#[test]
fn draws_hud_to_png() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    let mut hud = Hud::default();
    for json in [
        r#"{"translate":"chat.type.text","with":["Steve","hello world"]}"#,
        r#"{"text":"Alex joined the game","color":"yellow"}"#,
    ] {
        hud.chat.print(styled(json), 0, 0, false, Metrics(r.font()));
    }
    hud.selected_slot = 2;
    hud.set_action_bar("Action bar".into());
    let left = debug::left_lines(&debug::DebugInfo {
        pos: [8.0, 68.0, -4.0],
        ..Default::default()
    });
    let mut g = Gui::new(GuiScale::new(854, 480, 0), r.font(), 0);
    hud.draw(
        &mut g,
        &Frame {
            chat_input: Some("/say hi"),
            debug: Some((&left, &[])),
            fancy: true,
            brightness: 1.0,
            player_view: true,
            ..Default::default()
        },
    );
    let quads = g.into_quads();
    let (w, h, rgba) = r.render_to_rgba(&Camera::default(), &quads);
    let path = std::env::var("HUD_PNG").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("ferrocraft-hud.png")
            .to_string_lossy()
            .into()
    });
    encode_png(path.as_ref(), w, h, &rgba).expect("write png");
}
