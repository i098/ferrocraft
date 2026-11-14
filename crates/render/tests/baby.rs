//! Renders adult and baby zombies, pigmen, pigs, cows, sheep and chickens offscreen with the real
//! 1.8.9 assets. `MOBS_PROOF_DIR` keeps `before.png` (babies at adult size, as the client drew them
//! until now) and `after.png` (babies with the `isChild` model branch).

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

fn mob(id: u8, x: f64, z: f64, size: (f32, f32), child: bool) -> EntityDraw {
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
        child,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look { ..Look::default() },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn baby_mobs_draw_at_the_child_model_size() {
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
    // Hitboxes of 1.8.9 (adult): zombie and pigman 0.6 x 1.8, pig 0.9 x 0.9, cow 0.9 x 1.3,
    // sheep 0.9 x 1.3, chicken 0.4 x 0.7. Adults stand in the back row, babies in front.
    let kinds = [
        (54, (0.6, 1.8)),
        (57, (0.6, 1.8)),
        (90, (0.9, 0.9)),
        (92, (0.9, 1.3)),
        (91, (0.9, 1.3)),
        (93, (0.4, 0.7)),
    ];
    let x = |i: usize| -4.5 + 1.8 * i as f64;
    let scene = |child: bool| -> Vec<EntityDraw> {
        let mut v = Vec::new();
        for (i, (id, size)) in kinds.into_iter().enumerate() {
            v.push(mob(id, x(i), 9.0, size, false));
            let half = (size.0 / 2.0, size.1 / 2.0);
            v.push(mob(id, x(i), 5.0, if child { half } else { size }, child));
        }
        v
    };
    let empty = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&scene(false));
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&scene(true));
    let after = r.render_to_rgba(&cam, &[]).2;
    let drawn = |img: &[u8]| {
        img.chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .count()
    };
    // The front row halves, so the frame changes and its covered area shrinks.
    assert!(
        drawn(&after) < drawn(&before),
        "babies must cover fewer pixels"
    );
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}
