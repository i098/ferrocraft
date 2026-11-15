//! Renders the ender dragon beside a zombie offscreen with the real 1.8.9 assets.
//! `DRAGON_PROOF_DIR` keeps `dragon.png`.

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

/// Mob `id` of hitbox `size` at `pos`, turned `yaw` degrees, a never-ticked entity as a spawner
/// draws it (age is the partial tick).
fn mob(id: u8, size: (f32, f32), pos: [f64; 3], yaw: f32) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(id),
        pos,
        yaw,
        head_yaw: yaw,
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
    }
}

const DRAGON: u8 = 63;

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn dragon_draws_its_model_texture_and_eyes() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 72.0, 0.0],
        pitch: 22.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -2..=1 {
        for cz in -1..=3 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let side = mob(DRAGON, (16.0, 8.0), [0.0, 63.0, 18.0], 135.0);
    let zombie = mob(54, (0.6, 1.95), [5.0, 63.0, 10.0], 180.0);
    let empty = r.render_to_rgba(&cam, &[]).2;
    let pixels = |r: &mut Renderer, d: &[EntityDraw]| {
        r.set_entities(d);
        r.render_to_rgba(&cam, &[]).2
    };
    // Yaw 0 turns the head to the camera: `rotateCorpse` turns by `-yaw`, so the dragon faces the
    // opposite way of other mobs.
    let front = mob(DRAGON, (16.0, 8.0), [-6.0, 63.0, 30.0], 0.0);
    let all = pixels(&mut r, &[side.clone(), front.clone(), zombie]);
    if let Some(dir) = std::env::var_os("DRAGON_PROOF_DIR") {
        let path = std::path::PathBuf::from(dir).join("dragon.png");
        encode_png(&path, 800, 450, &all).unwrap();
    }
    let changed = |img: &[u8]| {
        img.chunks(4)
            .zip(empty.chunks(4))
            .filter(|(a, b)| a != b)
            .map(|(a, _)| [a[0], a[1], a[2]])
            .collect::<Vec<_>>()
    };
    let dragon = changed(&pixels(&mut r, &[side]));
    let colors = dragon
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    // The texture is near black, but its scales and wing bones give many shades; the stand-in
    // box was one tint in a few face shades.
    assert!(colors > 40, "dragon colors {colors}");
    // The dragon is 16 blocks long: 18 blocks away it covers far more than a zombie's few hundred.
    assert!(dragon.len() > 8_000, "dragon pixels {}", dragon.len());
    // `LayerEnderDragonEyes` adds the purple glow of `dragon_eyes.png` on the head.
    let purple = changed(&pixels(&mut r, &[front]))
        .iter()
        .filter(|[r, g, b]| *r > 150 && *b > 150 && *g < 100)
        .count();
    assert!(purple > 5, "eye pixels {purple}");
}
