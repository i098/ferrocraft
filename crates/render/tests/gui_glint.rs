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

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn inventory_glint_is_masked_by_the_model_texels() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(400, 500, Some(std::path::Path::new(&jar)));
    let cam = Camera::default();
    let mut plain = player();
    plain.armor_glint = [false; 4];
    plain.held_glint = false;
    let mut quads = vec![backdrop()];
    quads.extend(r.gui_entity([200.0, 460.0], 200.0, [0.0, -60.0], &plain));
    let base = r.render_to_rgba(&cam, &quads).2;
    quads = vec![backdrop()];
    quads.extend(r.gui_entity([200.0, 460.0], 200.0, [0.0, -60.0], &player()));
    let (w, h, glint) = r.render_to_rgba(&cam, &quads);
    let changed = base
        .chunks(4)
        .zip(glint.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 500, "the glint changed {changed} pixels");
    if let Some(dir) = std::env::var_os("GLINT_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("glint.png"), w, h, &glint).unwrap();
        encode_png(&dir.join("plain.png"), w, h, &base).unwrap();
    }
    // Opaque armor/sword coverage = pixels where the plain armored render differs from a bare player.
    // Outside it (backdrop and skin seen through chain holes) the glint must not change anything.
    let mut bare = player();
    bare.armor = [0; 4];
    bare.held = 0;
    bare.armor_glint = [false; 4];
    bare.held_glint = false;
    quads = vec![backdrop()];
    quads.extend(r.gui_entity([200.0, 460.0], 200.0, [0.0, -60.0], &bare));
    let bare = r.render_to_rgba(&cam, &quads).2;
    let leaked = bare
        .chunks(4)
        .zip(base.chunks(4))
        .zip(glint.chunks(4))
        .filter(|((u, p), g)| u == p && p != g)
        .count();
    assert_eq!(
        leaked, 0,
        "glint drawn over {leaked} pixels outside the opaque texels"
    );
}
