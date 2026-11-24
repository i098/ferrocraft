//! Renders a plain and a charged creeper offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `before.png` (both uncharged, as the client drew until now) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (mobs stand on it at y = 63).
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
    /// Full sky and block light everywhere, as at noon.
    fn light_section(&self, _s: usize, _sky: bool) -> Option<&[u8; 2048]> {
        Some(&[0xff; 2048])
    }
}

/// A mob facing the camera, standing at `x`.
fn mob(id: u8, x: f64, size: (f32, f32), health: f32) -> EntityDraw {
    let mut d = EntityDraw {
        class: Class::Mob(id),
        pos: [x, 63.0, 0.0],
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
    };
    d.look.health = health;
    d
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn a_charged_creeper_wears_the_lightning_shell() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 64.5, -5.0],
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -2..=2 {
        for cz in -2..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let mut draws = [
        mob(50, -1.5, (0.6, 1.7), 20.0),
        mob(50, 1.5, (0.6, 1.7), 20.0),
    ];
    r.set_entities(&draws);
    let before = r.render_to_rgba(&cam, &[]).2;
    draws[1].look.powered = true;
    r.set_entities(&draws);
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after, "the charged creeper draws the shell");
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 800, 450, &before).unwrap();
        encode_png(&dir.join("after.png"), 800, 450, &after).unwrap();
    }
}

/// Renders `draws` on flat ground and returns the RGBA frame, saved as `name` under `MOBS_PROOF_DIR`.
fn shot(draws: &[EntityDraw], cam: &Camera, name: &str) -> Vec<u8> {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    for cx in -2..=2 {
        for cz in -2..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // The first frame builds the chunk meshes and their light, which entities read.
    r.render_to_rgba(cam, &[]);
    r.set_entities(draws);
    let rgba = r.render_to_rgba(cam, &[]).2;
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        encode_png(&std::path::PathBuf::from(dir).join(name), 800, 450, &rgba).unwrap();
    }
    rgba
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn enderman_eyes_add_to_the_body_texture() {
    let cam = Camera {
        pos: [0.0, 65.5, -3.0],
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let mut d = mob(58, 0.0, (0.6, 2.9), 40.0);
    d.look.carried = 2 << 4;
    let rgba = shot(&[d], &cam, "enderman.png");
    // `enderman.png` paints the eyes white under the glow; `enderman_eyes.png` adds purple: pale pink.
    let pale = rgba
        .chunks(4)
        .filter(|p| p[0] > 230 && p[1] > 100 && p[2] > 230)
        .count();
    assert!(pale > 20, "pale pink eye pixels: {pale}");
}
