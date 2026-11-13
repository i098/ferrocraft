//! Draws the tab list offscreen with a custom skin head and writes a PNG (`TAB_PNG` overrides the path).
//! `FERROCRAFT_CLIENT_JAR` gives the 1.8.9 client jar for the font and the default skins.

use gui::tablist::{Clock, Head, TabEntry};
use gui::{Gui, GuiScale, Hud};
use render::{encode_png, Camera, Image, Renderer};

/// A 64x64 skin: red face at (8, 8), green hat at (40, 8), blue elsewhere in the head.
fn skin() -> Image {
    let mut rgba = vec![0; 64 * 64 * 4];
    for y in 0..16usize {
        for x in 0..64usize {
            let px = match (x, y) {
                (8..16, 8..16) => [255, 0, 0, 255],
                (40..48, 8..16) => [0, 255, 0, 255],
                _ => [0, 0, 255, 255],
            };
            rgba[(y * 64 + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    Image {
        width: 64,
        height: 64,
        rgba,
    }
}

#[test]
fn tab_list_draws_the_custom_skin_face_and_hat() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    r.register_image("skin/custom", &skin());
    let mut hud = Hud::default();
    hud.tablist.encrypted = true;
    for (uuid, name, skin) in [(2, "Custom", true), (4, "Default", false)] {
        hud.tablist.add(TabEntry {
            uuid,
            name: name.into(),
            head: Head {
                skin: skin.then(|| "skin/custom".to_owned()),
                hat: skin,
                upside_down: false,
            },
            ..TabEntry::default()
        });
    }
    let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
    hud.tablist.set_visible(true, 0);
    hud.tablist.draw(&mut g, &hud.scoreboard, Clock::default());
    let (w, h, rgba) = r.render_to_rgba(&Camera::default(), &g.into_quads());
    let path = std::env::var("TAB_PNG")
        .unwrap_or_else(|_| concat!(env!("CARGO_TARGET_TMPDIR"), "/tablist.png").into());
    encode_png(std::path::Path::new(&path), w, h, &rgba).expect("write png");
    let count = |c: [u8; 3]| {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3] == c)
            .count()
    };
    // The hat over the face covers it: 8x8 GUI pixels at scale 2 is 16x16 screen pixels.
    assert_eq!(count([0, 255, 0]), 16 * 16);
    assert_eq!(count([255, 0, 0]), 0);
}
