//! Renders the GUI player model of the inventory with enchanted chain armor and an enchanted sword.
//! `GLINT_PROOF_DIR` keeps `glint.png`. The glint must show on opaque texels only (`GL_EQUAL`).

use render::{encode_png, Blend, Camera, Class, EntityDraw, Look, Quad, Renderer};

/// A solid backdrop, so that an empty pixel is exactly the backdrop color.
fn backdrop() -> Quad {
    Quad {
        tex: "".into(),
        blend: Blend::Alpha,
        pos: [[0.0, 0.0], [400.0, 0.0], [400.0, 500.0], [0.0, 500.0]],
        uv: [[0.0; 2]; 4],
        color: [0xFF30_3030; 4],
    }
}

fn player() -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [0.0, 0.0, 0.0],
        yaw: 0.0,
        head_yaw: 0.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 7.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        size: (0.6, 1.8),
        held: 276,
        armor: [305, 304, 303, 302],
        armor_dye: [None; 4],
        armor_glint: [true; 4],
        held_glint: true,
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

/// The inventory model of `d` over the backdrop.
fn draw(r: &mut Renderer, d: &EntityDraw) -> (u32, u32, Vec<u8>) {
    let mut quads = vec![backdrop()];
    quads.extend(r.gui_entity([200.0, 460.0], 200.0, [0.0, -60.0], d));
    r.render_to_rgba(&Camera::default(), &quads)
}

fn without_glint(mut d: EntityDraw) -> EntityDraw {
    d.armor_glint = [false; 4];
    d.held_glint = false;
    d
}

/// Number of pixels where `keep(a, b, c)` holds, over the same pixel of three images.
fn count(a: &[u8], b: &[u8], c: &[u8], keep: impl Fn(&[u8], &[u8], &[u8]) -> bool) -> usize {
    let px = |v: &[u8]| v.chunks(4).map(<[u8]>::to_vec).collect::<Vec<_>>();
    let (a, b, c) = (px(a), px(b), px(c));
    (0..a.len()).filter(|&i| keep(&a[i], &b[i], &c[i])).count()
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn inventory_glint_is_masked_by_the_model_texels() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(400, 500, Some(std::path::Path::new(&jar)));
    let base = draw(&mut r, &without_glint(player())).2;
    let (w, h, glint) = draw(&mut r, &player());
    let changed = count(&base, &base, &glint, |a, _, g| a != g);
    assert!(changed > 500, "the glint changed {changed} pixels");
    if let Some(dir) = std::env::var_os("GLINT_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("glint.png"), w, h, &glint).unwrap();
        encode_png(&dir.join("plain.png"), w, h, &base).unwrap();
    }
    // Opaque armor/sword coverage = pixels where the plain armored render differs from a bare player.
    // Outside it (backdrop and skin seen through chain holes) the glint must not change anything.
    let mut bare = without_glint(player());
    (bare.armor, bare.held) = ([0; 4], 0);
    let bare = draw(&mut r, &bare).2;
    let leaked = count(&bare, &base, &glint, |u, p, g| u == p && p != g);
    assert_eq!(
        leaked, 0,
        "glint drawn over {leaked} pixels outside the opaque texels"
    );
}
