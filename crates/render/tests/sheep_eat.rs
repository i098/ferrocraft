//! Renders sheep that eat grass offscreen with the real 1.8.9 assets.
//! `SHEEP_PROOF_DIR` keeps `before.png` (head up, as drawn before status 10) and `after.png` (mid-bite).

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

fn sheep(x: f64, z: f64, eat_timer: i32) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(91),
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
        size: (0.9, 1.3),
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
        eye_height: 1.3 * 0.8,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            eat_timer,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn sheep_lowers_its_head_while_eating_grass() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // Timers 0, 3 (head sinking), 20 (head down, bobbing) and 38 (head rising).
    let row = |timers: [i32; 4]| -> Vec<EntityDraw> {
        timers
            .iter()
            .enumerate()
            .map(|(i, &t)| sheep(-4.5 + 3.0 * i as f64, 6.0, t))
            .collect()
    };
    // Skins load on the first frames that use them: draw the sheep once before the proof frame.
    r.set_entities(&row([0, 3, 20, 38]));
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&row([0; 4]));
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&row([0, 3, 20, 38]));
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after, "eating sheep must differ from idle sheep");
    if let Some(dir) = std::env::var_os("SHEEP_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}
