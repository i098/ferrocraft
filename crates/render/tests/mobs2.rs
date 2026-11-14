//! Renders a slime, magma cube, blaze, ghast and bat offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `before.png` (the tinted box the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the stands stand on it at y = 63).
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

fn mob(id: u8, x: f64, z: f64, size: (f32, f32), meta: u8) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(id),
        pos: [x, 63.0, z],
        yaw: 180.0,
        head_yaw: 180.0,
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
        eye_height: size.1 * 0.8,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            variant: meta,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn slime_magma_cube_blaze_ghast_and_bat_draw_their_models() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // Hitboxes of 1.8.9: slime 0.51 * size, blaze 0.6 x 1.8, ghast 4 x 4, bat 0.5 x 0.9. The ghast
    // charges a fireball (metadata 1) and the second bat hangs from a roost.
    let draws = [
        mob(55, 4.0, 6.0, (2.04, 2.04), 4),
        mob(62, 1.0, 6.0, (1.53, 1.53), 3),
        mob(61, -1.5, 6.0, (0.6, 1.8), 0),
        mob(65, -3.2, 6.0, (0.5, 0.9), 0),
        mob(65, -4.2, 6.0, (0.5, 0.9), 1),
        mob(56, 0.0, 15.0, (4.0, 4.0), 1),
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

/// Rows and columns that differ from the empty frame: `(height, width)` in pixels.
fn extent(img: &[u8], empty: &[u8]) -> (usize, usize) {
    let (mut y0, mut y1, mut x0, mut x1) = (usize::MAX, 0, usize::MAX, 0);
    for (i, (a, b)) in img.chunks(4).zip(empty.chunks(4)).enumerate() {
        if a != b {
            let (x, y) = (i % 800, i / 800);
            (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
        }
    }
    (y1 + 1 - y0, x1 + 1 - x0)
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn landing_squish_flattens_a_slime_and_a_magma_cube() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let empty = r.render_to_rgba(&cam, &[]).2;
    for (id, x) in [(55, 2.0), (62, -2.0)] {
        let mut d = mob(id, x, 6.0, (1.02, 1.02), 2);
        r.set_entities(std::slice::from_ref(&d));
        let rest = r.render_to_rgba(&cam, &[]).2;
        // The squish a slime has on the tick after it lands (`targetStretch` -0.5, eased by one half).
        d.look.squish = -0.25;
        r.set_entities(std::slice::from_ref(&d));
        let flat = r.render_to_rgba(&cam, &[]).2;
        let ((h0, w0), (h1, w1)) = (extent(&rest, &empty), extent(&flat, &empty));
        assert!(
            h1 < h0 && w1 > w0,
            "mob {id}: {h0}x{w0} at rest, {h1}x{w1} squished"
        );
        if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
            let dir = std::path::PathBuf::from(dir);
            encode_png(&dir.join(format!("rest-{id}.png")), 800, 450, &rest).unwrap();
            encode_png(&dir.join(format!("squished-{id}.png")), 800, 450, &flat).unwrap();
        }
    }
}
