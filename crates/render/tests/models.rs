//! Renders the ported mob and player models offscreen with the real 1.8.9 assets.

use render::{encode_png, Camera, CapeSway, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the entities stand on it at y = 63).
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
            0..=61 => 1 << 4,
            62 => 2 << 4,
            _ => 0,
        }
    }
}

fn draw(class: Class, x: f64, size: (f32, f32)) -> EntityDraw {
    EntityDraw {
        class,
        pos: [x, 63.0, 8.0],
        yaw: 180.0,
        head_yaw: 180.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 0.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        size,
        held: 0,
        armor: [0; 4],
        armor_dye: [None; 4],
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: size.1 * 0.85,
        always_name: false,
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look::default(),
    }
}

/// Every ported mob draws more than the empty scene; `MODELS_PROOF_DIR` keeps the picture.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn ported_mobs_render() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(960, 360, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let empty = r.render_to_rgba(&cam, &[]).2;
    let mobs: [(u8, (f32, f32)); 8] = [
        (120, (0.6, 1.8)),
        (99, (1.4, 2.9)),
        (60, (0.4, 0.3)),
        (93, (0.4, 0.7)),
        (91, (0.9, 1.3)),
        (52, (1.4, 0.9)),
        (59, (0.7, 0.5)),
        (58, (0.6, 2.9)),
    ];
    for (i, &(id, size)) in mobs.iter().enumerate() {
        r.set_entities(&[draw(Class::Mob(id), -1.0 + i as f64 * 0.01, size)]);
        assert_ne!(
            empty,
            r.render_to_rgba(&cam, &[]).2,
            "mob {id} draws nothing"
        );
    }
    let mut draws: Vec<EntityDraw> = mobs
        .iter()
        .enumerate()
        .map(|(i, &(id, size))| draw(Class::Mob(id), -6.0 + i as f64 * 1.7, size))
        .collect();
    r.set_entities(&draws);
    let image = r.render_to_rgba(&cam, &[]).2;
    let mut player = draw(Class::Player { slim: false }, 0.0, (0.6, 1.8));
    player.look = Look {
        cape: Some(CapeSway::default()),
        ..Look::default()
    };
    draws = vec![player];
    r.set_entities(&draws);
    let steve = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(empty, steve);
    if let Some(dir) = std::env::var_os("MODELS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("mobs.png"), 960, 360, &image).unwrap();
        encode_png(&dir.join("player.png"), 960, 360, &steve).unwrap();
    }
}

/// Downloaded skins and capes go through `write_texture`; filling every slot (LRU wrap included)
/// must not trip wgpu validation, and a coloured skin must change the picture.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn downloaded_skins_and_capes_upload_and_draw() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(480, 360, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let solid = |w: usize, h: usize, c: [u8; 4]| render::Image {
        width: w,
        height: h,
        rgba: c.iter().copied().cycle().take(w * h * 4).collect(),
    };
    let mut player = draw(Class::Player { slim: false }, 0.0, (0.6, 1.8));
    player.look = Look {
        cape: Some(CapeSway::default()),
        ..Look::default()
    };
    player.skin = Some(1);
    r.set_entities(&[player.clone()]);
    let before = r.render_to_rgba(&cam, &[]).2;
    for k in 1..=200u128 {
        r.set_player_skin(k, &solid(64, 64, [255, 0, 255, 255]));
        r.set_player_cape(k, &solid(64, 32, [0, 255, 0, 255]));
    }
    player.skin = Some(200);
    r.set_entities(&[player]);
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after);
    if let Some(dir) = std::env::var_os("MODELS_PROOF_DIR") {
        encode_png(
            &std::path::PathBuf::from(dir).join("skin_cape.png"),
            480,
            360,
            &after,
        )
        .unwrap();
    }
}
