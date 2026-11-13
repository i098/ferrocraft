//! A pumpkin on the head and a nether portal each draw a full-screen overlay over the HUD.
//! `FERROCRAFT_CLIENT_JAR` gives the 1.8.9 client jar; without it only the quads are checked.
//! `OVERLAY_DIR` gets `before.png` (plain view), `pumpkin.png` and `portal.png`.

use gui::{Frame, Gui, GuiScale, Hud};
use render::{encode_png, Camera, Quad, Renderer};

fn draw(r: &Renderer, hud: &mut Hud, frame: &Frame) -> Vec<Quad> {
    let mut g = Gui::new(GuiScale::new(427, 240, 0, false), r.font(), 0);
    hud.draw(&mut g, frame);
    g.into_quads()
}

fn mean_luma(rgba: &[u8]) -> f32 {
    let sum: u64 = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
        .sum();
    sum as f32 / (rgba.len() / 4) as f32 / 3.0
}

#[test]
fn pumpkin_and_portal_overlays_cover_the_screen() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(427, 240, jar.as_deref());
    let mut hud = Hud::default();
    let plain = draw(&r, &mut hud, &Frame::default());
    let pumpkin = draw(
        &r,
        &mut hud,
        &Frame {
            helmet_pumpkin: true,
            ..Frame::default()
        },
    );
    for _ in 0..40 {
        hud.portal.touch();
        hud.portal.tick(None);
    }
    let swirl = Frame {
        partial_ticks: 1.0,
        ..Frame::default()
    };
    let portal = draw(&r, &mut hud, &swirl);
    // Nausea replaces the swirl.
    let nauseous = draw(
        &r,
        &mut hud,
        &Frame {
            nausea: true,
            ..swirl
        },
    );
    let drawn = |q: &[Quad], tex: &str| q.iter().filter(|q| q.tex == tex).count();
    assert_eq!(drawn(&plain, gui::overlay::PUMPKIN), 0);
    assert_eq!(drawn(&pumpkin, gui::overlay::PUMPKIN), 1);
    assert_eq!(drawn(&portal, gui::overlay::PORTAL), 1);
    assert_eq!(drawn(&nauseous, gui::overlay::PORTAL), 0);

    if jar.is_none() {
        return;
    }
    let shots: Vec<_> = [&plain, &pumpkin, &portal]
        .map(|q| r.render_to_rgba(&Camera::default(), q))
        .into();
    assert!(
        mean_luma(&shots[1].2) < mean_luma(&shots[0].2),
        "a pumpkin helmet darkens the view"
    );
    assert_ne!(shots[2].2, shots[0].2, "the swirl changes the view");
    if let Some(dir) = std::env::var_os("OVERLAY_DIR") {
        for (name, (w, h, rgba)) in ["before", "pumpkin", "portal"].iter().zip(&shots) {
            let path = std::path::Path::new(&dir).join(format!("{name}.png"));
            encode_png(&path, *w, *h, rgba).expect("write png");
        }
    }
}
