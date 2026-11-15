//! Renders the wither and the giant zombie offscreen with the real 1.8.9 assets, a zombie beside them for scale.
//! `MOBS_PROOF_DIR` keeps `before.png` (the tinted boxes the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (mobs stand on it at y = 63).
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

/// A mob facing the camera, standing at `x`.
fn mob(id: u8, x: f64, size: (f32, f32), health: f32) -> EntityDraw {
    let mut d = EntityDraw {
        class: Class::Mob(id),
        pos: [x, 63.0, 14.0],
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
        eye_height: size.1 * 0.85,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look::default(),
    };
    d.look.health = health;
    d
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn the_wither_and_the_giant_draw_their_models_beside_a_zombie() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 68.0, -6.0],
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -2..=2 {
        for cz in -2..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // A zombie for scale, a wither below half health (armor aura) and a giant.
    let draws = [
        mob(54, -9.0, (0.6, 1.95), 20.0),
        mob(64, 3.0, (0.9, 3.5), 100.0),
        mob(64, -3.0, (0.9, 3.5), 300.0),
        mob(53, 11.0, (3.6, 10.8), 100.0),
    ];
    let empty = r.render_to_rgba(&cam, &[]).2;
    // What the client drew before: one tinted box of the hitbox size.
    let boxes: Vec<EntityDraw> = draws
        .iter()
        .filter(|d| d.class != Class::Mob(54))
        .map(|d| EntityDraw {
            class: Class::Mob(63),
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
        "textured models have many more colors than boxes: {textured} vs {flat}"
    );
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
        // A close look at the two withers: armor aura (below half health) on the left, full health on the right.
        let near = Camera {
            pos: [0.0, 66.0, 6.0],
            ..cam
        };
        let close = r.render_to_rgba(&near, &[]).2;
        encode_png(&dir.join("close.png"), 800, 450, &close).unwrap();
    }
}
