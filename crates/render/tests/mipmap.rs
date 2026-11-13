//! The Mipmap Levels option: a far fence line shimmers at level 0 and is calm at level 4.
//! Needs `FERROCRAFT_CLIENT_JAR` (the 1.8.9 client jar) for real textures; `RENDER_MIPMAP_DIR` keeps the frames.

use render::{encode_png, Camera, ChunkBlocks, Renderer};

/// Stone floor and one oak fence line (block 85) down the middle of each chunk.
struct FenceLine(i32);

impl ChunkBlocks for FenceLine {
    fn chunk_x(&self) -> i32 {
        0
    }
    fn chunk_z(&self) -> i32 {
        self.0
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        match y {
            0..=63 => 1 << 4,
            64 if x == 8 && z.is_multiple_of(2) => 85 << 4,
            _ => 0,
        }
    }
}

/// Sum of absolute differences of the red channel in the far half of two frames.
fn far_difference(a: &[u8], b: &[u8], w: usize, h: usize) -> u64 {
    let far = (h / 3) * w..(h / 2) * w;
    far.map(|p| u64::from(a[p * 4].abs_diff(b[p * 4]))).sum()
}

#[test]
fn level_four_calms_a_far_fence_line() {
    let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from) else {
        eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
        return;
    };
    let mut r = Renderer::new_offscreen(854, 480, Some(&jar));
    for cz in 0..8 {
        r.upload_chunk(&FenceLine(cz));
    }
    // Two eye positions a fifth of a texel apart: far fences land on different texels in each.
    let cam = |dx: f64| Camera {
        pos: [8.5 + dx, 65.2, -2.0],
        yaw: 0.0,
        pitch: 3.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let mut frames = Vec::new();
    for levels in [0, 4, 0] {
        r.set_mipmap_levels(levels);
        let a = r.render_to_rgba(&cam(0.0), &[]);
        let b = r.render_to_rgba(&cam(0.013), &[]).2;
        frames.push((a.0 as usize, a.1 as usize, a.2, b));
    }
    let (w, h) = (frames[0].0, frames[0].1);
    let shimmer = |f: &(usize, usize, Vec<u8>, Vec<u8>)| far_difference(&f.2, &f.3, w, h);
    assert!(
        shimmer(&frames[1]) < shimmer(&frames[0]) / 2,
        "level 4 shimmer {} vs level 0 {}",
        shimmer(&frames[1]),
        shimmer(&frames[0])
    );
    assert_ne!(frames[0].2, frames[1].2, "the option changes the frame");
    assert_eq!(
        frames[0].2, frames[2].2,
        "level 0 restores the unmipped frame"
    );
    if let Some(dir) = std::env::var_os("RENDER_MIPMAP_DIR") {
        for (name, f) in [("level0", &frames[0]), ("level4", &frames[1])] {
            let path = std::path::Path::new(&dir).join(format!("{name}.png"));
            encode_png(&path, w as u32, h as u32, &f.2).expect("write png");
        }
    }
}
