//! Renders a standing and a flapping chicken offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `chicken.png`.
use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

struct Flat(i32, i32);
impl ChunkBlocks for Flat {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn light_section(&self, _s: usize, sky: bool) -> Option<&[u8; 2048]> {
        static FULL: [u8; 2048] = [0xFF; 2048];
        sky.then_some(&FULL)
    }
    fn block(&self, _x: usize, y: usize, _z: usize) -> u16 {
        match y {
            0..=61 => 1 << 4,
            62 => 2 << 4,
            _ => 0,
        }
    }
}

fn mob(id: u8, x: f64, z: f64, size: (f32, f32), child: bool) -> EntityDraw {
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
        child,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look { ..Look::default() },
    }
}

fn chicken(x: f64, flap: f32) -> EntityDraw {
    let mut d = mob(93, x, 4.5, (0.4, 0.7), false);
    d.look = Look {
        chicken_flap: flap,
        ..Look::default()
    };
    d
}

#[test]
fn chicken_draws_legs_and_flapped_wings() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(1280, 720, jar.as_deref());
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
    // The first frame uploads the light the entities read.
    let _ = r.render_to_rgba(&cam, &[]);
    r.set_entities(&[chicken(-0.8, 0.0), chicken(0.8, 0.0)]);
    let standing = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&[chicken(-0.8, 0.0), chicken(0.8, 1.2)]);
    let img = r.render_to_rgba(&cam, &[]).2;
    let (mut left, mut right) = (0usize, 0usize);
    for (i, (a, b)) in standing
        .chunks_exact(4)
        .zip(img.chunks_exact(4))
        .enumerate()
    {
        if a != b {
            if i % 1280 < 640 {
                left += 1;
            } else {
                right += 1;
            }
        }
    }
    // Only the flapping chicken may change, and its spread wings must move pixels.
    // Camera looks down +z, so x=+0.8 (the flapping chicken) lands in the left half.
    assert!(right == 0, "standing chicken changed: {right}");
    assert!(left > 50, "wings stayed folded: {left}");
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        encode_png(
            &std::path::PathBuf::from(dir).join("chicken.png"),
            1280,
            720,
            &img,
        )
        .unwrap();
    }
}
