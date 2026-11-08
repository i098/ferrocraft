//! Meshes a synthetic terrain chunk, renders it offscreen and writes a PNG (`RENDER_PNG` overrides the path).

use render::{encode_png, Camera, ChunkBlocks, Renderer};

struct Synthetic(i32, i32);

impl ChunkBlocks for Synthetic {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        let (wx, wz) = (x as i32 + self.0 * 16, z as i32 + self.1 * 16);
        let height = 60 + (wx.rem_euclid(9) / 3 + wz.rem_euclid(8) / 4) as usize;
        match y {
            0 => 7 << 4,
            _ if y < height - 3 => 1 << 4,
            _ if y < height => 3 << 4,
            _ if y == height => 2 << 4,
            _ if y <= 61 => 9 << 4,
            _ => 0,
        }
    }
}

#[test]
fn renders_synthetic_chunks_to_png() {
    // Optional 1.8.9 client jar (or extracted assets dir) for real textures and the ascii.png font.
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    for cx in -1..=1 {
        for cz in 0..=2 {
            r.upload_chunk(&Synthetic(cx, cz));
        }
    }
    let cam = Camera {
        pos: [8.0, 68.0, -4.0],
        yaw: 10.0,
        pitch: 25.0,
        fov_deg: 70.0,
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &[]);

    let px = |x: u32, y: u32| &rgba[((y * w + x) * 4) as usize..][..4];
    assert_ne!(
        px(w / 2, h - 20),
        px(w - 5, 5),
        "terrain below the horizon must differ from the sky"
    );
    assert_eq!(
        px(w - 5, 5),
        [121, 167, 255, 255],
        "top-right shows the sky clear color"
    );

    let path = std::env::var("RENDER_PNG").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("ferrocraft-render.png")
            .to_string_lossy()
            .into()
    });
    encode_png(path.as_ref(), w, h, &rgba).expect("write png");
}
