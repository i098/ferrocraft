//! Offscreen render of flower pots with the real 1.8.9 assets: a chunk re-upload keeps their contents.
//! `POT_OUT=<dir>` keeps the PNG.

use render::{encode_png, flower_pot_state, Camera, ChunkBlocks, Renderer};
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
        match y {
            0..=61 => 1 << 4,
            62 => 3 << 4,
            63 => 2 << 4,
            64 if z == 6 && matches!(x, 6 | 8 | 10) => 140 << 4,
            _ => 0,
        }
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn pot_scene() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(960, 540, Some(Path::new(&jar)));
    r.set_block(8, 64, 6, flower_pot_state(38, 0));
    r.set_block(10, 64, 6, flower_pot_state(37, 0));
    // The pot contents arrived before the chunk, whose data holds only empty pots.
    r.upload_chunk(&Flat);
    let cam = Camera {
        pos: [8.5, 66.0, 10.0],
        yaw: 180.0,
        pitch: 25.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);
    let rose = rgba
        .chunks(4)
        .filter(|p| p[0] > 120 && p[1] < 40 && p[2] < 40)
        .count();
    assert!(rose > 20, "the potted rose is not drawn: {rose} red pixels");
    if let Some(dir) = std::env::var_os("POT_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join("pot.png"), w, h, &rgba).unwrap();
    }
}
