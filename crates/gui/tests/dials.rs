//! The compass and clock icons follow the player's turn and the time of day.
//! `FERROCRAFT_CLIENT_JAR` gives the 1.8.9 client jar; without it the test does nothing.
//! `DIALS_DIR` gets `before.png` (no tick yet, frame 0) and `after.png` (turned, midnight).

use gui::canvas::GuiCanvas;
use gui::inventory::dials::DialWorld;
use gui::inventory::icons::Icons;
use gui::{Gui, GuiScale};
use render::{encode_png, Camera, Quad, Renderer};
use world::inventory::ItemStack;

const COMPASS: i16 = 345;
const CLOCK: i16 = 347;

fn stack(id: i16) -> ItemStack {
    ItemStack {
        id,
        count: 1,
        damage: 0,
        nbt: None,
    }
}

fn draw(r: &Renderer, icons: &Icons) -> Vec<Quad> {
    let mut g = Gui::new(GuiScale::new(256, 96, 0, false), r.font(), 0);
    let mut c = GuiCanvas::new(&mut g);
    icons.draw(&mut c, &stack(COMPASS), 8, 8);
    icons.draw(&mut c, &stack(CLOCK), 40, 8);
    g.into_quads()
}

/// The top texture coordinate of the quad that draws `tex`.
fn top(quads: &[Quad], tex: &str) -> f32 {
    quads
        .iter()
        .find(|q| q.tex == tex)
        .map(|q| q.uv[0][1])
        .expect("sprite drawn")
}

#[test]
fn compass_and_clock_follow_the_world() {
    let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from) else {
        return;
    };
    let read = |p: &str| -> Option<Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(&jar).ok()?).ok()?;
        let mut out = Vec::new();
        std::io::Read::read_to_end(
            &mut z.by_name(&format!("assets/minecraft/{p}")).ok()?,
            &mut out,
        )
        .ok()?;
        Some(out)
    };
    let mut r = Renderer::new_offscreen(256, 96, Some(&jar));
    let mut icons = Icons::new(read);
    let world = |yaw: f32, time: i64| DialWorld {
        spawn: [0, 100],
        pos: [0.0, 0.0],
        yaw,
        time,
    };
    let before = draw(&r, &icons);
    let compass = "textures/items/compass.png";
    let clock = "textures/items/clock.png";
    assert_eq!(top(&before, compass), 0.0, "no tick yet: frame 0");
    for _ in 0..400 {
        icons.dials.tick(Some(world(0.0, 6000)));
    }
    let south = draw(&r, &icons);
    for _ in 0..400 {
        icons.dials.tick(Some(world(90.0, 18000)));
    }
    let west = draw(&r, &icons);
    // The strip has 32 frames: yaw 0 settles on frame 16 and yaw 90 on frame 8.
    assert_eq!(top(&south, compass), 16.0 / 32.0);
    assert_eq!(top(&west, compass), 8.0 / 32.0);
    // Noon (time 6000) is celestial angle 0, midnight is 0.5.
    assert_eq!(top(&south, clock), 0.0);
    assert_eq!(top(&west, clock), 0.5);
    if let Some(dir) = std::env::var_os("DIALS_DIR") {
        for (name, quads) in [("before", &before), ("after", &west)] {
            let (w, h, rgba) = r.render_to_rgba(&Camera::default(), quads);
            let path = std::path::Path::new(&dir).join(format!("{name}.png"));
            encode_png(&path, w, h, &rgba).expect("write png");
        }
    }
}
