//! Renders endermites offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `before.png` (the tinted box the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the endermites stand on it at y = 63).
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

/// A 0.4 x 0.3 endermite at `x`, turned `yaw` degrees, `age` ticks old.
fn endermite(x: f64, yaw: f32, age: f32) -> EntityDraw {
    let size = (0.4, 0.3);
    EntityDraw {
        class: Class::Mob(67),
        pos: [x, 63.0, 1.5],
        yaw,
        head_yaw: yaw,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age,
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
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look::default(),
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn endermites_draw_their_model_and_texture() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 63.8, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // Facing the camera, side on, away, and at three ages of the wiggle.
    let draws = [
        endermite(-0.9, 180.0, 0.0),
        endermite(-0.45, 90.0, 0.0),
        endermite(0.0, 0.0, 0.0),
        endermite(0.45, 180.0, 10.0),
        endermite(0.9, 180.0, 20.0),
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
    for d in &draws {
        r.set_entities(std::slice::from_ref(d));
        let one = r.render_to_rgba(&cam, &[]).2;
        let changed = one
            .chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 50,
            "endermite at {:?} drew {changed} pixels",
            d.pos
        );
    }
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
        // A 0.9 block pig beside the endermites shows their scale.
        let pig = EntityDraw {
            class: Class::Mob(90),
            size: (0.9, 0.9),
            ..endermite(1.5, 180.0, 0.0)
        };
        let mut scaled = draws.to_vec();
        scaled.push(pig);
        r.set_entities(&scaled);
        let scale = r.render_to_rgba(&cam, &[]).2;
        encode_png(&dir.join("after_scale.png"), 800, 450, &scale).unwrap();
    }
}
