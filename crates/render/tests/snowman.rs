//! Renders snow golems offscreen with the real 1.8.9 assets: front, side and back, and an invisible
//! one. `SNOW_PROOF_DIR` keeps `<SNOW_PROOF_NAME>.png` (default `after`; run before and after the fix).

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

fn mob(id: u8, x: f64, z: f64, yaw: f32, invisible: bool) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(id),
        pos: [x, 63.0, z],
        yaw,
        head_yaw: yaw,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 7.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible,
        size: (0.7, 1.9),
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
        eye_height: 1.0,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look { ..Look::default() },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn snow_golem_draws_its_model_with_a_pumpkin_head() {
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
    let orange = |f: &[u8]| {
        f.chunks(4)
            .filter(|p| p[0] > 150 && (60..170).contains(&p[1]) && p[2] < 60)
            .count()
    };
    let differs = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    r.set_entities(&[
        mob(97, -1.6, 5.0, 180.0, false),
        mob(97, 0.0, 5.0, 90.0, false),
        mob(97, 1.6, 5.0, 0.0, false),
    ]);
    let frame = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("SNOW_PROOF_DIR") {
        let name = std::env::var("SNOW_PROOF_NAME").unwrap_or_else(|_| "after".into());
        let path = std::path::PathBuf::from(dir).join(format!("{name}.png"));
        encode_png(&path, 800, 450, &frame).unwrap();
    }
    let golem = alone(&mut r, mob(97, 0.0, 5.0, 180.0, false));
    assert!(differs(&golem, &empty) > 2000, "the golem must draw");
    assert!(
        orange(&golem) > 150,
        "the pumpkin must show: {}",
        orange(&golem)
    );
    let hidden = alone(&mut r, mob(97, 0.0, 5.0, 180.0, true));
    assert!(orange(&hidden) < 20, "an invisible golem wears no pumpkin");
}
