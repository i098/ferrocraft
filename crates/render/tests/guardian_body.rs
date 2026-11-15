//! Renders a guardian and an elder guardian beside a pig offscreen with the real 1.8.9 assets.
//! `GUARDIAN_PROOF_DIR` keeps `guardian.png`.

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

/// Mob `id` of hitbox `size` at `x`, facing the camera, metadata 16 `variant`.
fn mob(id: u8, size: (f32, f32), x: f64, z: f64, variant: u8) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(id),
        pos: [x, 63.0, z],
        yaw: 180.0,
        head_yaw: 180.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 0.5,
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
        eye_height: size.1 * 0.5,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            variant,
            viewer: [0.0, 64.62, 0.0],
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn guardians_draw_their_model_and_texture() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 64.62, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let guardian = mob(68, (0.85, 0.85), -1.2, 3.5, 0);
    let elder = mob(68, (1.9975, 1.9975), 2.2, 6.0, 4);
    let pig = mob(90, (0.9, 0.9), 0.3, 3.5, 0);
    let empty = r.render_to_rgba(&cam, &[]).2;
    let pixels = |r: &mut Renderer, d: &[EntityDraw]| {
        r.set_entities(d);
        r.render_to_rgba(&cam, &[]).2
    };
    let changed = |img: &[u8]| {
        img.chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .map(|(a, _)| [a[0] >> 3, a[1] >> 3, a[2] >> 3])
            .collect::<Vec<_>>()
    };
    let small = changed(&pixels(&mut r, std::slice::from_ref(&guardian)));
    let big = changed(&pixels(&mut r, std::slice::from_ref(&elder)));
    let colors = |p: &[[u8; 3]]| p.iter().collect::<std::collections::HashSet<_>>().len();
    // A textured model has many colors; a flat tinted box has a few shades.
    assert!(colors(&small) > 40, "guardian colors {}", colors(&small));
    assert!(colors(&big) > 40, "elder colors {}", colors(&big));
    // The elder is 2.35 times larger: at 6 / 3.5 the distance it still covers (2.35 * 3.5 / 6)^2 =
    // 1.88 times the area.
    assert!(
        big.len() * 2 > small.len() * 3,
        "{} vs {}",
        big.len(),
        small.len()
    );
    let all = pixels(&mut r, &[guardian, pig, elder]);
    if let Some(dir) = std::env::var_os("GUARDIAN_PROOF_DIR") {
        let path = std::path::PathBuf::from(dir).join("guardian.png");
        encode_png(&path, 800, 450, &all).unwrap();
    }
}
