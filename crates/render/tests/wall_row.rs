//! Issue #817: standing on a stone floor at pitch -3, one pixel row across the screen sampled the
//! neighbouring sprite of the block atlas (orange, yellow) because the sprite's edge coordinates sat on its bounds.
//! `WALL_OUT=<png>` keeps the picture.

use render::{encode_png, Camera, ChunkBlocks, Renderer};
use std::path::Path;

struct Floor(i32, i32);

impl ChunkBlocks for Floor {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn block(&self, _x: usize, y: usize, _z: usize) -> u16 {
        if y <= 3 {
            1 << 4
        } else {
            0
        }
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn stone_floor_has_no_stray_line() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(1280, 720, Some(Path::new(&jar)));
    for cx in 2..=6 {
        for cz in 2..=6 {
            r.upload_chunk(&Floor(cx, cz));
        }
    }
    let cam = Camera {
        pos: [72.5, 5.62, 72.5],
        yaw: 0.0,
        pitch: -3.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);
    if let Some(out) = std::env::var_os("WALL_OUT") {
        encode_png(Path::new(&out), w, h, &rgba).expect("write png");
    }
    // Stone is grey; the neighbouring atlas sprites are not.
    for y in 560..h {
        for x in 0..w {
            let p = &rgba[((y * w + x) * 4) as usize..][..3];
            assert!(
                p[0].abs_diff(p[2]) < 12,
                "coloured pixel {p:?} at ({x}, {y}) on a stone floor"
            );
        }
    }
}
