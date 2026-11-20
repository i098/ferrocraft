//! A sealed cave: the server sends block light 15 for its cells. The renderer must draw the sent light, flood
//! it from the blocks when none is sent, and go back to flooding once a block in the chunk changes.
//! With `RENDER_DIR` set the frames are written as PNG files.

use render::{encode_png, Camera, ChunkBlocks, Renderer};

fn cave(x: i32, y: i32, z: i32) -> bool {
    (4..12).contains(&x) && (40..44).contains(&y) && (4..12).contains(&z)
}

struct Scene {
    lit: Option<[u8; 2048]>,
}

impl ChunkBlocks for Scene {
    fn chunk_x(&self) -> i32 {
        0
    }
    fn chunk_z(&self) -> i32 {
        0
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        if y <= 60 && !cave(x as i32, y as i32, z as i32) {
            1 << 4
        } else {
            0
        }
    }
    fn light_section(&self, s: usize, sky: bool) -> Option<&[u8; 2048]> {
        if s == 2 && !sky {
            self.lit.as_ref()
        } else {
            None
        }
    }
}

/// Block light 15 on every cave cell of section 2 (y 32..47).
fn sent() -> [u8; 2048] {
    let mut l = [0u8; 2048];
    for y in 8..12usize {
        for z in 4..12usize {
            for x in 4..12usize {
                let i = y << 8 | z << 4 | x;
                l[i / 2] |= 15 << (4 * (i & 1));
            }
        }
    }
    l
}

fn mean(px: &[u8]) -> f64 {
    px.chunks(4)
        .map(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]))
        .sum::<u32>() as f64
        / (px.len() / 4) as f64
}

fn frame(r: &mut Renderer) -> Vec<u8> {
    let cam = Camera {
        pos: [7.5, 41.6, 5.0],
        yaw: 0.0,
        pitch: 0.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    r.render_to_rgba(&cam, &[]).2
}

#[test]
fn server_light_is_drawn_and_a_block_change_relights() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(400, 300, jar.as_deref());
    r.upload_chunk(&Scene { lit: None });
    let flooded = frame(&mut r);
    r.upload_chunk(&Scene { lit: Some(sent()) });
    let server = frame(&mut r);
    // Placing a block in the chunk drops the sent light: the cave floods from its blocks again.
    r.set_block(7, 60, 7, 0);
    let after_edit = frame(&mut r);
    let (f, s, e) = (mean(&flooded), mean(&server), mean(&after_edit));
    eprintln!("mean brightness: flooded {f:.1}, server light {s:.1}, after edit {e:.1}");
    if let Some(dir) = std::env::var_os("RENDER_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, px) in [
            ("flooded", &flooded),
            ("server_light", &server),
            ("after_edit", &after_edit),
        ] {
            encode_png(&dir.join(format!("server_light_{name}.png")), 400, 300, px).unwrap();
        }
    }
    assert!(f < 30.0, "a sealed cave with no light is dark: {f}");
    assert!(s > f + 40.0, "the sent light brightens it: {s} vs {f}");
    assert!(e < s - 40.0, "an edit drops the sent light: {e} vs {s}");
}
