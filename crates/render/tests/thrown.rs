//! Renders a thrown eye of ender (72) and XP bottle (75) offscreen with the real 1.8.9 assets.
//! `THROWN_PROOF_DIR` keeps `before.png` (the boxes drawn until now) and `after.png`.

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
fn a_thrown_eye_of_ender_and_xp_bottle_draw_as_sprites() {
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
    let base = mob(0, 0.0, 0.0, (0.25, 0.25), 0);
    let at = |id: i8, x: f64| EntityDraw {
        class: Class::Object(id),
        pos: [x, 65.0, 3.0],
        ..base.clone()
    };
    let mut frame = |ids: [i8; 2]| {
        r.set_entities(&[at(ids[0], -0.4), at(ids[1], 0.4)]);
        r.render_to_rgba(&cam, &[]).2
    };
    let before = frame([99, 99]);
    let after = frame([72, 75]);
    assert_ne!(before, after);
    // The eye and the bottle draw different sprites.
    let eye_only = frame([72, 99]);
    let bottle_only = frame([99, 75]);
    assert_ne!(eye_only, bottle_only);
    if let Some(dir) = std::env::var_os("THROWN_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn a_thrown_snowball_and_pearl_draw_near_and_far() {
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
    let base = mob(0, 0.0, 0.0, (0.25, 0.25), 0);
    let at = |id: i8, x: f64, z: f64| EntityDraw {
        class: Class::Object(id),
        pos: [x, 65.0, z],
        ..base.clone()
    };
    let mut frame = |z: f64, ids: [i8; 2]| {
        r.set_entities(&[at(ids[0], -0.4, z), at(ids[1], 0.4, z)]);
        r.render_to_rgba(&cam, &[]).2
    };
    let empty = frame(3.0, [99, 99]);
    for z in [0.6, 1.0, 3.0] {
        let f = frame(z, [61, 65]);
        if let Some(dir) = std::env::var_os("THROWN_PROOF_DIR") {
            let dir = std::path::PathBuf::from(dir);
            encode_png(&dir.join(format!("sp_{z}.png")), 800, 450, &f).unwrap();
        }
        assert_ne!(empty, f, "z={z}");
    }
    for (yaw, pitch) in [(90.0f32, 0.0f32), (200.0, 20.0)] {
        let cam = Camera { yaw, pitch, ..cam.clone() };
        let (sy, cy) = (yaw.to_radians().sin(), yaw.to_radians().cos());
        let mk = |id: i8, side: f64| EntityDraw {
            class: Class::Object(id),
            pos: [-f64::from(sy) * 3.0 + f64::from(cy) * side, 65.0, f64::from(cy) * 3.0 + f64::from(sy) * side],
            view: [yaw, pitch],
            ..base.clone()
        };
        r.set_entities(&[mk(61, -0.4), mk(65, 0.4)]);
        let f = r.render_to_rgba(&cam, &[]).2;
        r.set_entities(&[]);
        let none = r.render_to_rgba(&cam, &[]).2;
        if let Some(dir) = std::env::var_os("THROWN_PROOF_DIR") {
            let dir = std::path::PathBuf::from(dir);
            encode_png(&dir.join(format!("yaw_{yaw}.png")), 800, 450, &f).unwrap();
        }
        assert_ne!(none, f, "yaw={yaw}");
    }
}
