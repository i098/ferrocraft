//! Meshes a synthetic terrain chunk, renders it offscreen and writes a PNG (`RENDER_PNG` overrides the path).

use render::{encode_png, Camera, ChunkBlocks, Environment, Hand, Hud, Renderer};

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
    let hud = Hud {
        crosshair: true,
        chat: vec![
            "<Steve> hello world".into(),
            "§eAlex joined the game".into(),
        ],
        chat_input: Some("/say hi".into()),
        debug: vec![
            "Minecraft 1.8.9 (ferrocraft)".into(),
            "XYZ: 8.000 / 68.000 / -4.000".into(),
        ],
    };
    let (w, h, rgba) = r.render_to_rgba(&cam, &hud);

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

/// Rain and the first-person hand: the rainy frame differs from the clear one, and the hand fills the lower right.
#[test]
fn rain_and_hand_draw_on_top_of_the_world() {
    let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from) else {
        // Rain and the arm need textures from the client jar.
        eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
        return;
    };
    let mut r = Renderer::new_offscreen(854, 480, Some(&jar));
    for cx in -1..=1 {
        for cz in 0..=2 {
            r.upload_chunk(&Synthetic(cx, cz));
        }
    }
    let cam = Camera {
        pos: [8.0, 65.62, 8.0],
        yaw: 20.0,
        pitch: 10.0,
        fov_deg: 70.0,
    };
    let hud = Hud {
        crosshair: true,
        ..Hud::default()
    };
    let frame = |r: &mut Renderer, rain: f32, hand: Option<Hand>| {
        r.set_environment(Environment {
            rain,
            ticks: 40,
            ..Environment::default()
        });
        r.set_hand(hand);
        r.render_to_rgba(&cam, &hud).2
    };
    let bare = Hand {
        held: None,
        swing: 0.0,
        lowered: 0.0,
    };
    let clear = frame(&mut r, 0.0, None);
    let rainy = frame(&mut r, 1.0, None);
    let armed = frame(&mut r, 0.0, Some(bare));
    let holding = frame(
        &mut r,
        0.0,
        Some(Hand {
            held: Some(1 << 4),
            ..bare
        }),
    );
    assert_ne!(clear, rainy, "rain must change the frame");
    assert_ne!(clear, armed, "the arm must change the frame");
    assert_ne!(armed, holding, "a held block must differ from the arm");
    if let Some(dir) = std::env::var_os("RENDER_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, px) in [
            ("clear", &clear),
            ("rain", &rainy),
            ("arm", &armed),
            ("block", &holding),
        ] {
            encode_png(&dir.join(format!("{name}.png")), 854, 480, px).expect("write png");
        }
    }
}
