//! Offscreen render of an end portal with the real 1.8.9 assets. `PORTAL_OUT=<dir>` keeps the PNGs.

use render::{encode_png, Camera, ChunkBlocks, Renderer};
use std::path::{Path, PathBuf};

/// Flat ground: stone below y 63, grass at 63 (state ids `id << 4 | meta`).
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
            0..=62 => 1 << 4,
            63 => 2 << 4,
            _ => 0,
        }
    }
}

fn frame(r: &mut Renderer, cam: &Camera, name: &str) -> Vec<u8> {
    r.set_block_entities(cam.pos, 0, 0.0);
    let (w, h, rgba) = r.render_to_rgba(cam, &[]);
    if let Some(dir) = std::env::var_os("PORTAL_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join(format!("{name}.png")), w, h, &rgba).unwrap();
    }
    rgba
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn end_portal_fills_its_frame_with_stars() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(480, 270, Some(Path::new(&jar)));
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let cam = Camera {
        pos: [1.5, 66.0, 7.5],
        yaw: 180.0,
        pitch: 35.0,
        ..Camera::default()
    };
    let before = frame(&mut r, &cam, "before");
    // A 3 by 3 portal in the grass layer.
    for x in 0..=2 {
        for z in 3..=5 {
            r.set_block(x, 63, z, 119 << 4);
        }
    }
    let after = frame(&mut r, &cam, "after");
    let changed = before
        .chunks(4)
        .zip(after.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert!(changed > 2000, "only {changed} pixels changed");
}
