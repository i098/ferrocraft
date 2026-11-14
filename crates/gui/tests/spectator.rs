//! Draws the spectator bar offscreen and writes a PNG (`SPECTATOR_PNG` overrides the path).
//! `FERROCRAFT_CLIENT_JAR` gives the 1.8.9 client jar for the font and the spectator textures.

use gui::scoreboard::Scoreboard;
use gui::spectator::SpectatorGui;
use gui::tablist::{TabEntry, TabList};
use gui::{Gui, GuiScale};
use render::{encode_png, Camera, Renderer};

#[test]
fn the_open_bar_draws_slots_with_key_names_and_the_prompt() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    let mut tab = TabList::default();
    for (uuid, name) in [(2, "Alex"), (4, "Steve")] {
        tab.add(TabEntry {
            uuid,
            name: name.into(),
            ..TabEntry::default()
        });
    }
    let mut menu = SpectatorGui::default();
    menu.set_keys(std::array::from_fn(|i| (i + 1).to_string()));
    let sb = Scoreboard::default();
    menu.select_slot(0, 1000, &tab, &sb);
    menu.scroll(1, 1000);
    let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
    menu.draw_hotbar(&mut g, 1000);
    menu.draw_tooltip(&mut g, 1000);
    let quads = g.into_quads();
    // Bar and cursor, then four icons: players, teams, the disabled next page arrow and close.
    let from = |tex: &str| quads.iter().filter(|q| &*q.tex == tex).count();
    assert_eq!(from("textures/gui/widgets.png"), 2);
    assert_eq!(from("textures/gui/spectator_widgets.png"), 4);
    let (w, h, rgba) = r.render_to_rgba(&Camera::default(), &quads);
    let path = std::env::var("SPECTATOR_PNG")
        .unwrap_or_else(|_| concat!(env!("CARGO_TARGET_TMPDIR"), "/spectator.png").into());
    encode_png(std::path::Path::new(&path), w, h, &rgba).expect("write png");
}
