//! Renders wolves and ocelots offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `before.png` (the tinted box the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the mobs stand on it at y = 63).
struct Flat(i32, i32);

impl ChunkBlocks for Flat {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn block(&self, _x: usize, y: usize, _z: usize) -> u16 {
        match y {
            0..=61 => 1 << 4,
            62 => 2 << 4,
            _ => 0,
        }
    }
}

/// A 0.6 x 0.8 mob facing the camera; `flags` is metadata 16 (1 sitting, 2 angry, 4 tamed).
fn pet(id: u8, x: f64, flags: u8, collar: u8, child: bool) -> EntityDraw {
    let size = if child { (0.3, 0.4) } else { (0.6, 0.8) };
    EntityDraw {
        class: Class::Mob(id),
        pos: [x, 63.0, 3.0],
        yaw: 180.0 - 25.0,
        head_yaw: 180.0 - 25.0,
        pitch: 10.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 7.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        size,
        held: 0,
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
        eye_height: size.1 * 0.8,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            variant: flags,
            collar,
            health: 20.0,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn wolves_and_ocelots_draw_their_models_and_textures() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 64.2, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // Wild, angry, tame sitting with a red collar and a baby wolf; a wild ocelot, a tame black cat
    // sitting and a tame red cat (cat type in `collar`).
    let draws = [
        pet(95, -2.7, 0, 14, false),
        pet(95, -1.8, 2, 14, false),
        pet(95, -0.9, 1 | 4, 1, false),
        pet(95, 0.0, 0, 14, true),
        pet(98, 0.9, 0, 0, false),
        pet(98, 1.8, 1 | 4, 1, false),
        pet(98, 2.7, 4, 2, false),
    ];
    let empty = r.render_to_rgba(&cam, &[]).2;
    // What the client drew before: one tinted box of the hitbox size.
    let boxes: Vec<EntityDraw> = draws
        .iter()
        .map(|d| EntityDraw {
            class: Class::Object(99),
            ..d.clone()
        })
        .collect();
    r.set_entities(&boxes);
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&draws);
    let after = r.render_to_rgba(&cam, &[]).2;

    let colors = |img: &[u8]| {
        img.chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .map(|(a, _)| [a[0] >> 3, a[1] >> 3, a[2] >> 3])
            .collect::<std::collections::HashSet<_>>()
            .len()
    };
    let (flat, textured) = (colors(&before), colors(&after));
    assert!(
        textured > flat * 3,
        "a textured model has many more colors than a box: {textured} vs {flat}"
    );
    // Each mob changes pixels on its own: none of them is missing from the frame.
    for d in &draws {
        r.set_entities(std::slice::from_ref(d));
        let one = r.render_to_rgba(&cam, &[]).2;
        let changed = one
            .chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 100, "mob {:?} drew {changed} pixels", d.class);
    }
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn shaking_wolf_differs_from_a_still_wolf() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 64.2, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let still = pet(95, 0.0, 4, 14, false);
    let mut shaking = still.clone();
    // `getShakeAngle` for head, mane, body and tail, a quarter into the shake.
    shaking.look.wolf_shake = [0.158_727_78, 0.426_234_28, -0.116_751_96, -0.353_422_64];
    r.set_entities(std::slice::from_ref(&still));
    // The skins load on first use; draw once with the roll, then back to the still wolf.
    r.set_entities(std::slice::from_ref(&shaking));
    r.render_to_rgba(&cam, &[]);
    r.set_entities(std::slice::from_ref(&still));
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(std::slice::from_ref(&shaking));
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after, "the shake rolls the wolf");
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("shake_before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("shake_after.png"), 800, 450, &after).unwrap();
    }
}
