//! Offscreen render of crops, stems, the sunflower, tall grass, fire, flowing water, grass variants and
//! horizontal logs with the real 1.8.9 assets. `PLANTS_OUT=<dir>` keeps the PNG.

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
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        let _ = (x, z);
        match y {
            0..=61 => 1 << 4,
            62 => 3 << 4,
            63 => 2 << 4,
            _ => 0,
        }
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn plants_scene() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(960, 540, Some(Path::new(&jar)));
    r.upload_chunk(&Flat);
    let put = |r: &mut Renderer, x: i32, y: i32, z: i32, id: u16, meta: u16| {
        r.set_block(x, y, z, id << 4 | meta)
    };
    // Row z 3: wheat, carrots, nether wart, melon stem age 2 and 7 with a melon, pumpkin stem with fruit.
    put(&mut r, 1, 64, 3, 59, 7);
    put(&mut r, 3, 64, 3, 141, 7);
    put(&mut r, 5, 64, 3, 115, 3);
    put(&mut r, 7, 64, 3, 105, 2);
    put(&mut r, 9, 64, 3, 105, 7);
    put(&mut r, 10, 64, 3, 103, 0);
    put(&mut r, 12, 64, 3, 104, 7);
    put(&mut r, 13, 64, 3, 86, 0);
    // Row z 6: sunflower, tall grass, dead bush.
    put(&mut r, 2, 64, 6, 175, 0);
    put(&mut r, 2, 65, 6, 175, 8);
    for x in 4..8 {
        put(&mut r, x, 64, 6, 31, 1);
    }
    put(&mut r, 10, 64, 6, 32, 0);
    // Flowers at two heights. Vanilla sets each offset from the world X and Z coordinates.
    put(&mut r, 12, 64, 6, 38, 0);
    put(&mut r, 13, 64, 6, 37, 0);
    put(&mut r, 14, 64, 6, 1, 0);
    put(&mut r, 14, 65, 6, 38, 0);
    // Row z 9: logs along x and z, hay bale, flowing water, fire against a log.
    put(&mut r, 1, 64, 9, 17, 4);
    put(&mut r, 3, 64, 9, 17, 8);
    put(&mut r, 5, 64, 9, 170, 4);
    put(&mut r, 7, 64, 9, 170, 8);
    put(&mut r, 9, 64, 9, 9, 0);
    put(&mut r, 10, 64, 9, 8, 3);
    put(&mut r, 11, 64, 9, 8, 6);
    put(&mut r, 13, 64, 9, 17, 0);
    put(&mut r, 13, 64, 10, 17, 0);
    put(&mut r, 14, 64, 9, 51, 0);
    put(&mut r, 14, 63, 9, 87, 0);
    let cam = Camera {
        pos: [8.0, 72.0, 15.0],
        yaw: 180.0,
        pitch: 45.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);
    assert!(rgba.iter().any(|&b| b != 0));
    if let Some(dir) = std::env::var_os("PLANTS_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join("plants.png"), w, h, &rgba).unwrap();
    }
}
