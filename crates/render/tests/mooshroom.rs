//! Renders mooshrooms offscreen with the real 1.8.9 assets: adult from the front, the side and the
//! back, a baby and a cow. `MOOSH_PROOF_DIR` keeps `before.png` and `after.png` as the tests of this
//! file write them (run before and after the fix).

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

fn mob(id: u8, x: f64, z: f64, yaw: f32, child: bool) -> EntityDraw {
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
        invisible: false,
        size: if child { (0.45, 0.65) } else { (0.9, 1.3) },
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
        child,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look { ..Look::default() },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn mooshroom_draws_the_cow_model_with_its_skin_and_mushrooms() {
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
    // Front, side and back of an adult, a baby (no mushrooms) and a cow for comparison.
    let scene = [
        mob(96, -3.6, 7.0, 180.0, false),
        mob(96, -1.8, 7.0, 90.0, false),
        mob(96, 0.0, 7.0, 0.0, false),
        mob(96, 1.8, 7.0, 180.0, true),
        mob(92, 3.6, 7.0, 180.0, false),
    ];
    let empty = r.render_to_rgba(&cam, &[]).2;
    let red = |f: &[u8]| {
        f.chunks(4)
            .filter(|p| {
                p[0] > 60 && p[0] as u16 > p[1] as u16 + 40 && p[0] as u16 > p[2] as u16 + 40
            })
            .count()
    };
    let bright_red = |f: &[u8]| {
        f.chunks(4)
            .filter(|p| p[0] > 140 && p[1] < 80 && p[2] < 80)
            .count()
    };
    let alone = |r: &mut Renderer, e: EntityDraw| {
        r.set_entities(&[e]);
        r.render_to_rgba(&cam, &[]).2
    };
    let back = alone(&mut r, mob(96, 0.0, 7.0, 0.0, false));
    let cow = alone(&mut r, mob(92, 0.0, 7.0, 0.0, false));
    let differs = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    assert!(differs(&back, &empty) > 2000, "the mooshroom must draw");
    assert!(differs(&cow, &empty) > 2000, "the cow must draw");
    assert!(
        red(&back) > 3 * red(&cow) + 200,
        "the mooshroom skin and the red mushrooms must show: {} vs cow {}",
        red(&back),
        red(&cow)
    );
    let close = Camera {
        pos: [0.0, 64.6, 2.5],
        fov_deg: 22.0,
        ..Camera::default()
    };
    r.set_entities(&[mob(96, 0.0, 7.0, 0.0, false)]);
    let closeup = r.render_to_rgba(&close, &[]).2;
    // Vanilla 1.8.9 shades the back mushrooms dark (issue #737); unshaded they measured 17199 bright-red pixels, shaded 1494.
    assert!(
        bright_red(&closeup) < 6000,
        "the back mushrooms must be lit like vanilla, not bright: {}",
        bright_red(&closeup)
    );
    let baby = alone(&mut r, mob(96, 0.0, 7.0, 0.0, true));
    let baby_cow = alone(&mut r, mob(92, 0.0, 7.0, 0.0, true));
    assert!(
        bright_red(&baby) < 3 * bright_red(&baby_cow) + 200,
        "a baby mooshroom has no mushrooms"
    );
    r.set_entities(&scene);
    let frame = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("MOOSH_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let name = std::env::var("MOOSH_PROOF_NAME").unwrap_or_else(|_| "after".into());
        encode_png(&dir.join(format!("{name}.png")), 800, 450, &frame).unwrap();
        // Close-up of the side and the back view, where the three mushrooms show.
        let close = Camera {
            pos: [0.0, 64.6, 2.5],
            fov_deg: 22.0,
            ..Camera::default()
        };
        let frame = r.render_to_rgba(&close, &[]).2;
        encode_png(&dir.join(format!("{name}_close.png")), 800, 450, &frame).unwrap();
    }
}
