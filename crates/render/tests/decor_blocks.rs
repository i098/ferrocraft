//! Offscreen render of the hopper, cauldron, anvil, brewing stand and dragon egg with the real 1.8.9 assets.
//! `DECOR_OUT=<dir>` keeps the PNGs.

use render::{encode_png, Camera, ChunkBlocks, Renderer};
use std::path::{Path, PathBuf};

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

fn frame(r: &mut Renderer, name: &str) -> Vec<u8> {
    let cam = Camera {
        pos: [5.5, 66.0, 7.0],
        yaw: 180.0,
        pitch: 22.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);
    if let Some(dir) = std::env::var_os("DECOR_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join(format!("{name}.png")), w, h, &rgba).unwrap();
    }
    rgba
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn each_block_draws_where_it_used_to_be_invisible() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(960, 360, Some(Path::new(&jar)));
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let before = frame(&mut r, "before");
    // Row at z 3: hopper down, hopper east, cauldron levels 0 and 3, anvil, brewing stand with all bottles, egg.
    let row = [
        154 << 4,
        154 << 4 | 5,
        118 << 4,
        118 << 4 | 3,
        145 << 4 | 1,
        117 << 4 | 7,
        122 << 4,
    ];
    let mut prev = before.clone();
    for (i, state) in row.into_iter().enumerate() {
        r.set_block(i as i32 * 2 - 1, 64, 3, state);
        let now = frame(&mut r, &format!("step{i}"));
        let changed = prev
            .chunks(4)
            .zip(now.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 100,
            "block {} draws: {changed} pixels changed",
            state >> 4
        );
        prev = now;
    }
    frame(&mut r, "after");
}
