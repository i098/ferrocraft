//! Offscreen render of pistons, sloped rails and redstone dust with the real 1.8.9 assets.
//! `REDSTONE_OUT=<dir>` keeps the PNG.

use render::{encode_png, Camera, ChunkBlocks, Renderer};
use std::path::{Path, PathBuf};

struct Flat;

impl ChunkBlocks for Flat {
    fn chunk_x(&self) -> i32 {
        0
    }
    fn chunk_z(&self) -> i32 {
        0
    }
    fn block(&self, _x: usize, y: usize, _z: usize) -> u16 {
        match y {
            0..=62 => 1 << 4,
            63 => 4 << 4,
            _ => 0,
        }
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn redstone_scene() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(960, 540, Some(Path::new(&jar)));
    r.upload_chunk(&Flat);
    let put = |r: &mut Renderer, x: i32, y: i32, z: i32, id: u16, meta: u16| {
        r.set_block(x, y, z, id << 4 | meta)
    };
    // Row z 2: pistons facing up, north, east, south; sticky piston facing west.
    put(&mut r, 1, 64, 2, 33, 1);
    put(&mut r, 3, 64, 2, 33, 2);
    put(&mut r, 5, 64, 2, 33, 5);
    put(&mut r, 7, 64, 2, 29, 3);
    put(&mut r, 9, 64, 2, 29, 4);
    // Extended pistons with heads: up, east, north (sticky).
    put(&mut r, 1, 64, 4, 33, 8 | 1);
    put(&mut r, 1, 65, 4, 34, 1);
    put(&mut r, 4, 64, 4, 33, 8 | 5);
    put(&mut r, 5, 64, 4, 34, 5);
    put(&mut r, 8, 64, 5, 29, 8 | 2);
    put(&mut r, 8, 64, 4, 34, 8 | 2);
    // Row z 8: rails, flat, curve, ascending in the four directions, powered rail.
    put(&mut r, 1, 64, 8, 66, 0);
    put(&mut r, 3, 64, 8, 66, 9);
    put(&mut r, 5, 64, 8, 66, 2);
    put(&mut r, 5, 64, 8, 66, 2);
    put(&mut r, 7, 64, 8, 66, 3);
    put(&mut r, 9, 64, 8, 66, 4);
    put(&mut r, 11, 64, 8, 66, 5);
    put(&mut r, 13, 64, 8, 27, 8 | 2);
    // Row z 12: single dust, a line, a cross, and dust that climbs a wall.
    put(&mut r, 1, 64, 12, 55, 15);
    put(&mut r, 3, 64, 12, 55, 15);
    put(&mut r, 4, 64, 12, 55, 15);
    put(&mut r, 6, 64, 12, 55, 7);
    put(&mut r, 5, 64, 12, 55, 7);
    put(&mut r, 7, 64, 12, 55, 7);
    put(&mut r, 6, 64, 11, 55, 7);
    put(&mut r, 6, 64, 13, 55, 7);
    put(&mut r, 10, 64, 12, 1, 0);
    put(&mut r, 10, 65, 12, 55, 12);
    put(&mut r, 9, 64, 12, 55, 12);
    let cam = Camera {
        pos: [7.5, 72.0, 19.0],
        yaw: 180.0,
        pitch: 50.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);
    assert!(rgba.iter().any(|&b| b != 0));
    if let Some(dir) = std::env::var_os("REDSTONE_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join("redstone.png"), w, h, &rgba).unwrap();
    }
}
