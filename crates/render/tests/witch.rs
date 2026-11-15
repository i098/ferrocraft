//! Renders witches offscreen with the real 1.8.9 assets: front, side and back, and one with a potion
//! in the hand. `WITCH_PROOF_DIR` keeps `<WITCH_PROOF_NAME>.png` (default `after`; run before and after the fix).

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

fn witch(x: f64, z: f64, yaw: f32, held: i16) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(66),
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
        size: (0.6, 1.95),
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

const POTION: i16 = 373;

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn witch_draws_its_model_with_hat_and_potion() {
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
    let alone = |r: &mut Renderer, e: EntityDraw| {
        r.set_entities(&[e]);
        r.render_to_rgba(&cam, &[]).2
    };
    let differs = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    let mut holding = witch(1.5, 3.0, 180.0, POTION);
    holding.held_damage = 8194;
    r.set_entities(&[
        witch(-1.5, 3.0, 180.0, 0),
        witch(-0.5, 3.0, 90.0, 0),
        witch(0.5, 3.0, 0.0, 0),
        holding,
    ]);
    let frame = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("WITCH_PROOF_DIR") {
        let name = std::env::var("WITCH_PROOF_NAME").unwrap_or_else(|_| "after".into());
        let path = std::path::PathBuf::from(dir).join(format!("{name}.png"));
        encode_png(&path, 800, 450, &frame).unwrap();
    }
    if let Some(dir) = std::env::var_os("WITCH_PROOF_DIR") {
        let name = std::env::var("WITCH_PROOF_NAME").unwrap_or_else(|_| "after".into());
        let mut close = witch(0.0, 1.6, 180.0, POTION);
        close.held_damage = 8194;
        close.head_yaw = 150.0;
        let frame = alone(&mut r, close);
        let path = std::path::PathBuf::from(dir).join(format!("{name}_potion.png"));
        encode_png(&path, 800, 450, &frame).unwrap();
    }
    let plain = alone(&mut r, witch(0.0, 5.0, 180.0, 0));
    let mut with_potion = witch(0.0, 5.0, 180.0, POTION);
    with_potion.held_damage = 8194;
    let potion = alone(&mut r, with_potion);
    assert!(differs(&plain, &empty) > 2000, "the witch must draw");
    assert!(differs(&plain, &potion) > 100, "the potion must draw");
}
