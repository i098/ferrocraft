//! Offscreen render of the comparator, repeater, cocoa, wall torch, door and piston head from their vanilla models.
//! `MODELS_OUT=<dir>` keeps the PNG.

use render::{encode_png, Camera, ChunkBlocks, Renderer};
use std::path::Path;

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

fn frame(r: &mut Renderer) -> Vec<u8> {
    let cam = Camera {
        pos: [5.5, 66.0, 7.0],
        yaw: 180.0,
        pitch: 22.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);
    if let Some(dir) = std::env::var_os("MODELS_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&Path::new(&dir).join("block_models.png"), w, h, &rgba).unwrap();
    }
    rgba
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn model_blocks_draw_from_their_vanilla_models() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(960, 360, Some(Path::new(&jar)));
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let mut prev = frame(&mut r);
    // Comparator (subtract, south), repeater 4 ticks, cocoa age 2, wall torch east, lower door, piston head up.
    let row = [
        149 << 4 | 4,
        93 << 4 | 12,
        127 << 4 | 8,
        50 << 4 | 1,
        64 << 4,
        34 << 4 | 1,
    ];
    for (i, state) in row.into_iter().enumerate() {
        r.set_block(i as i32 * 2 - 1, 64, 3, state);
        let now = frame(&mut r);
        let changed = prev
            .chunks(4)
            .zip(now.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 20, "block {} draws: {changed} pixels", state >> 4);
        prev = now;
    }
}
