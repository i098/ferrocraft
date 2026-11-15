//! The survival inventory screen with the player model, offscreen with the real 1.8.9 assets.
//! `INVENTORY_PROOF_DIR` keeps `before.png` (the screen without the model, as the client drew it
//! until now) and `after.png`; `held_before.png` and `held_after.png` show the model empty-handed
//! and with an iron sword in hand (`LayerHeldItem`).

use gui::inventory::icons::Icons;
use gui::inventory::{ContainerScreen, InventoryView};
use gui::{Gui, GuiCanvas, GuiScale};
use render::{encode_png, Camera, Class, EntityDraw, Look, Renderer};
use world::effects::Effects;
use world::inventory::Inventory;

fn player(held: i16) -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [0.0; 3],
        yaw: 0.0,
        head_yaw: 0.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 0.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        size: (0.6, 1.8),
        held,
        armor: [0; 4],
        armor_dye: [None; 4],
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        held_dye: None,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look::default(),
    }
}

/// The quads of one frame of the screen, with the model quads spliced in at their draw position.
fn frame(r: &mut Renderer, icons: &Icons, with_model: bool, held: i16) -> Vec<render::Quad> {
    let inv = Inventory::default();
    let mut screen = ContainerScreen::new(&inv, 427, 240);
    let effects = Effects::default();
    let me = player(held);
    let view = InventoryView {
        inv: &inv,
        icons,
        advanced_tooltips: false,
        effects: &effects,
        player: Some(&me),
    };
    let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
    screen.draw(&mut GuiCanvas::new(&mut g), &view, 300, 60);
    let entities = g.take_entities();
    assert_eq!(entities.len(), 1, "one player model per inventory screen");
    let mut quads = g.into_quads();
    if with_model {
        for e in entities.iter().rev() {
            quads.splice(
                e.at..e.at,
                r.gui_entity(e.origin, e.scale, e.mouse, &e.draw),
            );
        }
    }
    quads
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn inventory_screen_draws_the_player_model() {
    let jar =
        std::path::PathBuf::from(std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client jar"));
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
    let icons = Icons::new(read);
    let mut r = Renderer::new_offscreen(854, 480, Some(&jar));
    let quads = frame(&mut r, &icons, false, 0);
    let before = r.render_to_rgba(&Camera::default(), &quads);
    let quads = frame(&mut r, &icons, true, 0);
    let after = r.render_to_rgba(&Camera::default(), &quads);
    // The 26..75 x 8..77 box of the window: GUI scale 2, window at (116, 52) in a 427x240 screen.
    let (x0, y0, x1, y1) = (2 * (116 + 26), 2 * (52 + 8), 2 * (116 + 75), 2 * (52 + 77));
    let changed = |x: u32, y: u32| {
        let i = ((y * before.0 + x) * 4) as usize;
        before.2[i..i + 4] != after.2[i..i + 4]
    };
    let inside = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y)));
    let n = inside.filter(|&(x, y)| changed(x, y)).count();
    assert!(n > 2000, "only {n} pixels of the player box changed");
    if let Some(dir) = std::env::var_os("INVENTORY_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        encode_png(&dir.join("before.png"), before.0, before.1, &before.2).unwrap();
        encode_png(&dir.join("after.png"), after.0, after.1, &after.2).unwrap();
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn inventory_model_shows_the_held_item() {
    let jar =
        std::path::PathBuf::from(std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client jar"));
    let icons = Icons::new(|p: &str| -> Option<Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(&jar).ok()?).ok()?;
        let mut out = Vec::new();
        std::io::Read::read_to_end(
            &mut z.by_name(&format!("assets/minecraft/{p}")).ok()?,
            &mut out,
        )
        .ok()?;
        Some(out)
    });
    let mut r = Renderer::new_offscreen(854, 480, Some(&jar));
    const IRON_SWORD: i16 = 267;
    let quads = frame(&mut r, &icons, true, 0);
    let empty = r.render_to_rgba(&Camera::default(), &quads);
    let quads = frame(&mut r, &icons, true, IRON_SWORD);
    let armed = r.render_to_rgba(&Camera::default(), &quads);
    let n = empty
        .2
        .chunks(4)
        .zip(armed.2.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(n > 200, "only {n} pixels changed with a sword in hand");
    if let Some(dir) = std::env::var_os("INVENTORY_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        encode_png(&dir.join("held_before.png"), empty.0, empty.1, &empty.2).unwrap();
        encode_png(&dir.join("held_after.png"), armed.0, armed.1, &armed.2).unwrap();
    }
}
