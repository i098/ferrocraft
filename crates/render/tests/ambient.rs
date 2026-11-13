//! A lava lake pops and drips, torches, fire and a furnace smoke: 60 ticks of `random_display_ticks`
//! change the frame. With `RENDER_DIR` set the before and after frames are written as PNG files.

use render::{
    encode_png, random_display_ticks, AmbientEffect, Camera, ChunkBlocks, JavaRandom, Particles,
    Renderer,
};

const LAVA: u16 = 11 << 4;

/// Stone floor at y 60, a lava lake, two torches, fire, a lit furnace and lit redstone ore.
fn block_at(x: i32, y: i32, z: i32) -> u16 {
    match (x, y, z) {
        (6..=13, 60, 8..=15) => LAVA,
        (_, 0..=60, _) => 1 << 4,
        (3, 61, 9) | (3, 61, 12) => 50 << 4 | 5,
        (5, 61, 11) => 51 << 4,
        (2, 61, 6) => 62 << 4 | 3,
        (4, 61, 6) => 74 << 4,
        _ => 0,
    }
}

struct Scene(i32, i32);

impl ChunkBlocks for Scene {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        block_at(x as i32 + self.0 * 16, y as i32, z as i32 + self.1 * 16)
    }
}

#[test]
fn block_ambient_effects_change_the_frame() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Scene(cx, cz));
        }
    }
    let cam = Camera {
        pos: [5.0, 62.62, 4.0],
        yaw: -20.0,
        pitch: 25.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let feet = [5, 61, 4];
    let mut frame = |ps: &Particles| {
        r.set_particles(
            ps.vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, 0.0, |p| {
                r.light_at(p)
            }),
        );
        r.render_to_rgba(&cam, &[]).2
    };
    let before = frame(&Particles::new());

    let mut ps = Particles::new();
    let mut rng = JavaRandom::new(2024);
    let mut sounds = 0;
    for _ in 0..60 {
        let view = [5.0, 61.0, 4.0];
        let effects = random_display_ticks(feet, view, 0, &mut rng, &block_at, &|_, _, _| false);
        for e in effects {
            match e {
                AmbientEffect::Particle { kind, pos, motion } => ps.spawn(kind, pos, motion, 0),
                AmbientEffect::Sound { .. } => sounds += 1,
            }
        }
        ps.tick(block_at);
    }
    assert!(
        !ps.is_empty() && sounds > 0,
        "{} particles, {sounds} sounds",
        ps.len()
    );
    let after = frame(&ps);
    assert_ne!(before, after, "the ambient particles must change the frame");

    if let Some(dir) = std::env::var_os("RENDER_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, px) in [("ambient_before", &before), ("ambient_after", &after)] {
            encode_png(&dir.join(format!("{name}.png")), 854, 480, px).unwrap();
        }
    }
}
