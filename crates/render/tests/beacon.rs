//! Offscreen render of a beacon on an iron pyramid with the real 1.8.9 assets.
//! `BEACON_OUT=<dir>` keeps the PNGs.

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

fn frame(r: &mut Renderer, cam: &Camera, time: i64, name: &str) -> Vec<u8> {
    r.set_block_entities(cam.pos, time, 0.0);
    let (w, h, rgba) = r.render_to_rgba(cam, &[]);
    if let Some(dir) = std::env::var_os("BEACON_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join(format!("{name}.png")), w, h, &rgba).unwrap();
    }
    rgba
}

fn differs(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count()
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn beacon_beam_appears_after_the_scan_and_tints_through_glass() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(480, 270, Some(Path::new(&jar)));
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // An iron pyramid of one level under a beacon at (0, 64, 4).
    for x in -1..=1 {
        for z in 3..=5 {
            r.set_block(x, 63, z, 42 << 4);
        }
    }
    r.set_block(0, 64, 4, 138 << 4);
    let cam = Camera {
        pos: [0.5, 66.0, 11.0],
        yaw: 180.0,
        pitch: -25.0,
        ..Camera::default()
    };
    // A beacon first seen waits for the next scan (a tick with `time % 80 == 0`).
    let block_only = frame(&mut r, &cam, 1, "beacon_before_scan");
    let again = frame(&mut r, &cam, 2, "beacon_before_scan_2");
    assert_eq!(differs(&block_only, &again), 0, "no beam before the scan");
    let mut grown = frame(&mut r, &cam, 80, "beacon_beam_start");
    for t in 80..110 {
        grown = frame(&mut r, &cam, t, "beacon_beam");
    }
    assert!(differs(&block_only, &grown) > 1500, "the beam draws");

    // Red glass at y 70 tints the beam above it.
    r.set_block(0, 70, 4, (95 << 4) | 14);
    let mut tinted = grown.clone();
    for t in 160..190 {
        tinted = frame(&mut r, &cam, t, "beacon_beam_red");
    }
    assert!(differs(&grown, &tinted) > 800, "glass changes the beam");
}
