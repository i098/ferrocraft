//! Renders a firework rocket and its large ball burst offscreen with the real 1.8.9 assets.
//! `FIREWORK_PROOF_DIR` keeps `before.png` (the box and no burst the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Particles, Renderer};
use world::entities::FireworkExplosion;

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
fn a_firework_rocket_and_its_burst_draw() {
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
    let rocket = mob(76, 0.0, 0.0, (0.25, 0.25), 0);
    let rocket = EntityDraw {
        class: Class::Object(76),
        pos: [-2.0, 65.0, 5.0],
        ..rocket
    };
    let mut frame = |ps: &Particles, d: &EntityDraw| {
        r.set_entities(std::slice::from_ref(d));
        r.set_particles(
            ps.vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, 0.5, |p| {
                r.light_at(p)
            }),
        );
        r.render_to_rgba(&cam, &[]).2
    };
    let generic = EntityDraw {
        class: Class::Object(99),
        ..rocket.clone()
    };
    let before = frame(&Particles::new(), &generic);
    let mut ps = Particles::new();
    ps.firework(
        [1.0, 68.0, 8.0],
        [0.0; 3],
        vec![FireworkExplosion {
            shape: 1,
            trail: true,
            colors: vec![0xFF0000, 0xFFAA00],
            fade: vec![0x0000FF],
            ..FireworkExplosion::default()
        }],
    );
    for _ in 0..8 {
        ps.tick(|_, _, _| 0);
    }
    let after = frame(&ps, &rocket);
    let rocket_only = frame(&Particles::new(), &rocket);
    assert_ne!(before, rocket_only, "the rocket draws its item sprite");
    let burst = after
        .chunks(4)
        .zip(rocket_only.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(burst > 500, "the burst changed only {burst} pixels");
    if let Some(dir) = std::env::var_os("FIREWORK_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}
