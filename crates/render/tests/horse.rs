//! Renders horses offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `before.png` (the tinted box the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, HorseLook, Look, Renderer};

/// Flat stone and grass up to y = 62 (the horses stand on it at y = 63).
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

/// A 1.4 x 1.6 horse (halved for a baby) at `x`, `z` blocks; `horse` is its metadata and animation.
fn horse(x: f64, z: f64, horse: HorseLook, child: bool, swing: f32) -> EntityDraw {
    let size = if child { (0.7, 0.8) } else { (1.4, 1.6) };
    EntityDraw {
        class: Class::Mob(100),
        pos: [x, 63.0, z],
        yaw: 180.0 - 25.0,
        head_yaw: 180.0 - 25.0,
        pitch: 10.0,
        limb_swing: swing * 3.0,
        limb_swing_amount: swing,
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
            horse,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn horses_draw_their_model_and_textures() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(1000, 500, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, -1.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let h = |kind, flags, variant, armor| HorseLook {
        kind,
        flags,
        variant,
        armor,
        ..HorseLook::default()
    };
    // Back row: coats and markings, a saddled and armored horse, a donkey with a chest, a mule, the undead.
    let back = [
        horse(-5.0, 6.0, h(0, 0, 0, 0), false, 0.0),
        horse(-3.5, 6.0, h(0, 0, 3 | (2 << 8), 0), false, 0.0),
        horse(-2.0, 6.0, h(0, 4, 5 | (1 << 8), 3), false, 0.0),
        horse(-0.5, 6.0, h(1, 8, 0, 0), false, 0.0),
        horse(1.0, 6.0, h(2, 4, 0, 0), false, 0.0),
        horse(2.5, 6.0, h(3, 0, 0, 0), false, 0.0),
        horse(4.0, 6.0, h(4, 0, 0, 0), false, 0.0),
    ];
    // Front row: walking, rearing, grazing, mouth open and a baby.
    let rear = HorseLook {
        angry: 1.0,
        ..h(0, 4, 1, 0)
    };
    let graze = HorseLook {
        grass: 1.0,
        ..h(0, 0, 2, 0)
    };
    let open = HorseLook {
        mouth: 1.0,
        ..h(0, 0, 4, 0)
    };
    let front = [
        horse(-3.0, 3.5, h(0, 4, 0, 1), false, 0.8),
        horse(-1.5, 3.5, rear, false, 0.0),
        horse(0.0, 3.5, graze, false, 0.0),
        horse(1.5, 3.5, open, false, 0.0),
        horse(2.8, 3.5, h(0, 0, 6, 0), true, 0.0),
    ];
    let draws: Vec<EntityDraw> = back.into_iter().chain(front).collect();
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
    for d in &draws {
        r.set_entities(std::slice::from_ref(d));
        let one = r.render_to_rgba(&cam, &[]).2;
        let changed = one
            .chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 50, "horse at {:?} drew {changed} pixels", d.pos);
    }
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 1000, 500, &before).unwrap();
        encode_png(&dir.join("after.png"), 1000, 500, &after).unwrap();
    }
}

/// A player (`Class::Player`) of 0.6 x 1.8 at `pos`, in the riding pose when `riding`.
fn player(pos: [f64; 3], riding: bool) -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos,
        size: (0.6, 1.8),
        eye_height: 1.62,
        look: Look {
            riding,
            ..Look::default()
        },
        ..horse(0.0, 0.0, HorseLook::default(), false, 0.0)
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn riders_sit_on_standing_and_rearing_horses() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, -1.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let saddled = |angry| HorseLook {
        flags: 4,
        angry,
        ..HorseLook::default()
    };
    // The horse yaw is 155: it faces the camera. Rest seat 0.75 * 1.6 - 0.35 above its feet; a rearing horse
    // (`prevRearingAmount` 1) adds 0.15 up and 0.7 along (sin, -cos) of the yaw.
    let (s, c) = (155.0f64.to_radians().sin(), 155.0f64.to_radians().cos());
    let seat =
        |x: f64, z: f64, rear: f64| [x + 0.7 * rear * s, 63.85 + 0.15 * rear, z - 0.7 * rear * c];
    let scene = |riding: bool| {
        vec![
            horse(-1.8, 4.0, saddled(0.0), false, 0.0),
            player(seat(-1.8, 4.0, 0.0), riding),
            horse(1.0, 4.0, saddled(1.0), false, 0.0),
            player(seat(1.0, 4.0, 1.0), riding),
            // A player on foot for scale.
            player([3.2, 63.0, 3.0], false),
        ]
    };
    // The first frames load the textures.
    for _ in 0..3 {
        r.set_entities(&scene(false));
        r.render_to_rgba(&cam, &[]);
    }
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&scene(true));
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after, "the riding pose changes the picture");
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("rider_before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("rider_after.png"), 800, 450, &after).unwrap();
    }
}
