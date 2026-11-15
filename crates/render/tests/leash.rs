//! Renders a leashed cow and a leash knot offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `before.png` (no rope, a tinted box for the knot) and `after.png`.

use render::leash::Holder;
use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the knot and the cow stand on it).
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

/// An entity of `class` at `pos` with the plain defaults.
fn draw(class: Class, pos: [f64; 3], size: (f32, f32), look: Look) -> EntityDraw {
    EntityDraw {
        class,
        pos,
        yaw: 270.0,
        pitch: 0.0,
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
        eye_height: 0.0,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        head_yaw: 0.0,
        look,
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn a_leashed_cow_draws_its_rope_and_the_knot_its_model() {
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
    let knot = [1.5, 63.5, 4.5];
    let cow_look = Look {
        leash: Some(Holder::new(knot, -0.0625, [0.0; 2], [0.0; 2], 0.0, true)),
        ..Look::default()
    };
    let leashed = [
        draw(Class::Mob(92), [-1.5, 63.0, 4.5], (0.9, 1.3), cow_look),
        draw(Class::Object(77), knot, (0.375, 0.5), Look::default()),
    ];
    let empty = r.render_to_rgba(&cam, &[]).2;
    let unleashed = [
        EntityDraw {
            look: Look::default(),
            ..leashed[0].clone()
        },
        EntityDraw {
            class: Class::Object(99),
            ..leashed[1].clone()
        },
    ];
    r.set_entities(&unleashed);
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&leashed);
    let after = r.render_to_rgba(&cam, &[]).2;

    // The rope is the flat brown 0.5, 0.4, 0.3 (or 0.7 of it), unlit.
    let rope = |img: &[u8]| {
        img.chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b && a[0] > a[1] && a[1] > a[2] && a[0] < 140)
            .count()
    };
    let (none, some) = (rope(&before), rope(&after));
    assert!(
        some > none + 100,
        "the rope adds brown pixels: {some} vs {none}"
    );
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}
