//! Renders players offscreen with the real 1.8.9 assets, each holding a flat item, a tool or a block, seen
//! from the front-right. `HELD_PROOF_DIR` keeps `<HELD_PROOF_NAME>.png` (default `after`).

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the witches stand on it at y = 63).
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

fn player(x: f64, z: f64, yaw: f32, held: i16) -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [x, 63.0, z],
        yaw,
        head_yaw: yaw,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 40.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        size: (0.6, 1.8),
        held,
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
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            id: 7,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn held_flat_items_and_chest_draw_at_the_hand() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(1280, 720, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 66.5, 0.0],
        pitch: 28.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let empty = r.render_to_rgba(&cam, &[]).2;
    // apple, sapling, bow, torch, pickaxe, sword, stick, door, map, chest, stone
    let ids = [260, 6, 261, 50, 257, 276, 280, 324, 358, 54, 1];
    let mut rows = Vec::new();
    for (row, chunk) in ids.chunks(6).enumerate() {
        let draws: Vec<EntityDraw> = chunk
            .iter()
            .enumerate()
            .map(|(i, &id)| player(-4.0 + 1.6 * i as f64, 4.0 + 2.2 * row as f64, 150.0, id))
            .collect();
        rows.push(draws);
    }
    let all: Vec<EntityDraw> = rows.concat();
    r.set_entities(&all);
    let held_all = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("HELD_PROOF_DIR") {
        let name = std::env::var("HELD_PROOF_NAME").unwrap_or_else(|_| "after".into());
        let path = std::path::PathBuf::from(dir).join(format!("{name}.png"));
        encode_png(&path, 1280, 720, &held_all).unwrap();
    }
    if let Some(dir) = std::env::var_os("HELD_PROOF_DIR") {
        // F5 front view: the camera at the eye height, level, looking at one row from the front.
        let name = std::env::var("HELD_PROOF_NAME").unwrap_or_else(|_| "after".into());
        let eye = Camera {
            pos: [0.0, 64.62, 0.0],
            fov_deg: 70.0,
            ..Camera::default()
        };
        for (n, ids) in [("a", &ids[..4]), ("b", &ids[4..8]), ("c", &ids[8..])] {
            let row: Vec<EntityDraw> = ids
                .iter()
                .enumerate()
                .map(|(i, &id)| player(-2.4 + 1.6 * i as f64, 3.0, 180.0, id))
                .collect();
            r.set_entities(&row);
            let frame = r.render_to_rgba(&eye, &[]).2;
            let path = std::path::PathBuf::from(&dir).join(format!("{name}_eye_{n}.png"));
            encode_png(&path, 1280, 720, &frame).unwrap();
        }
    }
    if let Some(dir) = std::env::var_os("HELD_PROOF_DIR") {
        // Seen from below, as a camera under a flying player sees the hands.
        let name = std::env::var("HELD_PROOF_NAME").unwrap_or_else(|_| "after".into());
        let under = Camera {
            pos: [0.0, 63.0, 0.0],
            pitch: -30.0,
            fov_deg: 70.0,
            ..Camera::default()
        };
        let row: Vec<EntityDraw> = [260, 6, 276, 324, 358]
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                let mut e = player(-3.0 + 1.5 * i as f64, 4.0, 180.0, id);
                e.pos[1] = 66.0;
                e
            })
            .collect();
        r.set_entities(&row);
        let frame = r.render_to_rgba(&under, &[]).2;
        let path = std::path::PathBuf::from(&dir).join(format!("{name}_below.png"));
        encode_png(&path, 1280, 720, &frame).unwrap();
    }
    // Seen from below, the apple shows its filled face, not only the ring of its edges.
    let under = Camera {
        pos: [0.0, 63.0, 0.0],
        pitch: -30.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let mut apple = player(0.0, 4.0, 180.0, 260);
    apple.pos[1] = 66.0;
    r.set_entities(&[apple]);
    let apple_below = r.render_to_rgba(&under, &[]).2;
    let red = apple_below
        .chunks(4)
        .filter(|p| p[0] > 120 && p[1] < 50 && p[2] < 50)
        .count();
    println!("apple pixels from below: {red}");
    assert!(red > 150, "the apple must show its face from below: {red}");
    let differs = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    let bare: Vec<EntityDraw> = all
        .iter()
        .cloned()
        .map(|mut e| {
            e.held = 0;
            e
        })
        .collect();
    r.set_entities(&bare);
    let none = r.render_to_rgba(&cam, &[]).2;
    assert!(differs(&none, &empty) > 2000, "the players must draw");
    assert!(differs(&none, &held_all) > 500, "the held items must draw");
    let chest = player(0.0, 4.0, 150.0, 54);
    let mut bare_chest = chest.clone();
    bare_chest.held = 0;
    r.set_entities(&[bare_chest]);
    let without = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&[chest]);
    let with = r.render_to_rgba(&cam, &[]).2;
    let wood = |f: &[u8]| {
        f.chunks(4)
            .filter(|p| p[0] > 110 && p[0] > p[1] + 30 && p[1] > 70 && p[2] < 60)
            .count()
    };
    let (a, b) = (wood(&with), wood(&without));
    assert!(
        a > b + 100,
        "the held chest must draw wood pixels: {a} vs {b}"
    );
}
