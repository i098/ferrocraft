//! Meshes a synthetic terrain chunk, renders it offscreen and writes a PNG (`RENDER_PNG` overrides the path).

use render::{
    encode_png, Blend, Camera, ChunkBlocks, Class, EntityDraw, Environment, Hand, HeldKind,
    Particles, Quad, Renderer,
};

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
        ..Camera::default()
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

/// Rain, the first-person hand, a flat held item, particles, the local player model (third person)
/// and a runtime-registered GUI texture: each changes the frame.
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
        ..Camera::default()
    };
    let frame = |r: &mut Renderer, rain: f32, hand: Option<Hand>| {
        r.set_environment(Environment {
            rain,
            ticks: 40,
            ..Environment::default()
        });
        r.set_hand(hand);
        r.render_to_rgba(&cam, &[]).2
    };
    let bare = Hand {
        held: None,
        swing: 0.0,
        lowered: 0.0,
        using: None,
    };
    let clear = frame(&mut r, 0.0, None);
    let rainy = frame(&mut r, 1.0, None);
    let armed = frame(&mut r, 0.0, Some(bare));
    let holding = frame(
        &mut r,
        0.0,
        Some(Hand {
            held: Some(HeldKind::Block(1 << 4)),
            ..bare
        }),
    );
    let sword = frame(
        &mut r,
        0.0,
        Some(Hand {
            held: Some(HeldKind::Item { id: 276, damage: 0 }),
            ..bare
        }),
    );
    let torch = frame(
        &mut r,
        0.0,
        Some(Hand {
            held: Some(HeldKind::Item { id: 50, damage: 0 }),
            ..bare
        }),
    );
    assert_ne!(clear, rainy, "rain must change the frame");
    assert_ne!(clear, armed, "the arm must change the frame");
    assert_ne!(armed, holding, "a held block must differ from the arm");
    assert_ne!(holding, sword, "a flat sword must differ from a held block");
    assert_ne!(sword, torch, "a flat torch must differ from the sword");

    let mut ps = Particles::new();
    ps.break_block([7, 66, 12], 1 << 4);
    ps.spawn_packet(26, [8.5, 66.0, 11.0], [0.5, 0.3, 0.5], 0.01, 30, &[]);
    ps.spawn_packet(34, [7.0, 66.5, 12.0], [0.5, 0.3, 0.5], 0.0, 8, &[]);
    r.set_particles(ps.vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, 0.0));
    let particles = frame(
        &mut r,
        0.0,
        Some(Hand {
            held: Some(HeldKind::Item { id: 276, damage: 0 }),
            ..bare
        }),
    );
    assert_ne!(sword, particles, "particles must change the frame");

    // Third person: the engine pushes the local player as an entity; nothing culls it.
    r.set_entities(&[EntityDraw {
        class: Class::Player { slim: false },
        pos: [7.5, 64.0, 11.5],
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
        size: (0.6, 1.8),
        held: 276,
        armor: [0; 4],
        armor_dye: [None; 4],
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        pointed: false,
        ridden: false,
        child: false,
        using: None,
    }]);
    let third = frame(&mut r, 0.0, None);
    r.set_entities(&[]);
    assert_ne!(clear, third, "the local player model must draw");

    r.register_texture("ferrocraft/server-icon", &tiny_png());
    let icon = Quad {
        tex: "ferrocraft/server-icon".into(),
        blend: Blend::Alpha,
        pos: [[10.0, 10.0], [74.0, 10.0], [74.0, 74.0], [10.0, 74.0]],
        uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color: [0xFFFF_FFFF; 4],
    };
    r.set_hand(None);
    let gui = r.render_to_rgba(&cam, &[icon]).2;
    assert_eq!(&gui[(20 * 854 + 20) * 4..][..4], &[255, 0, 0, 255]);

    if let Some(dir) = std::env::var_os("RENDER_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, px) in [
            ("clear", &clear),
            ("rain", &rainy),
            ("arm", &armed),
            ("block", &holding),
            ("sword", &sword),
            ("torch", &torch),
            ("particles", &particles),
            ("third", &third),
            ("icon", &gui),
        ] {
            encode_png(&dir.join(format!("{name}.png")), 854, 480, px).expect("write png");
        }
    }
}

/// A 2x2 solid red PNG.
fn tiny_png() -> Vec<u8> {
    let path = std::env::temp_dir().join(format!("ferrocraft-icon-{}.png", std::process::id()));
    encode_png(&path, 2, 2, &[255, 0, 0, 255].repeat(4)).expect("write png");
    let bytes = std::fs::read(&path).expect("read png");
    let _ = std::fs::remove_file(path);
    bytes
}

#[test]
fn view_bob_and_gamma_change_the_frame() {
    let frame = |bob: [f32; 4], gamma: f32| {
        let mut r = Renderer::new_offscreen(320, 200, None);
        r.set_environment(Environment {
            gamma,
            ..Environment::default()
        });
        for cx in -1..=1 {
            for cz in 0..=2 {
                r.upload_chunk(&Synthetic(cx, cz));
            }
        }
        let cam = Camera {
            pos: [8.0, 68.0, -4.0],
            pitch: 25.0,
            bob,
            ..Camera::default()
        };
        r.render_to_rgba(&cam, &[]).2
    };
    let base = frame([0.0; 4], 0.0);
    assert_eq!(base, frame([0.0; 4], 0.0), "rendering is deterministic");
    assert_ne!(base, frame([0.1, 0.1, 2.0, 1.0], 0.0), "bob moves the view");
    assert_ne!(base, frame([0.0; 4], 1.0), "gamma brightens the terrain");
}

/// The real fire assets must show flame cutouts, animate, and disappear when extinguished.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn burning_entity_and_first_person_fire() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(400, 300, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, -4.0],
        ..Camera::default()
    };
    let mut entity = EntityDraw {
        class: Class::Player { slim: false },
        pos: [0.0, 64.0, 0.0],
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
        size: (0.6, 1.8),
        held: 0,
        armor: [0; 4],
        armor_dye: [None; 4],
        using: None,
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        pointed: false,
        ridden: false,
        child: false,
        using: None,
    };
    r.set_entities(&[entity.clone()]);
    let before = r.render_to_rgba(&cam, &[]).2;
    entity.burning = true;
    r.set_entities(&[entity.clone()]);
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after, "burning adds the fire geometry");
    r.set_environment(Environment {
        ticks: 3,
        ..Environment::default()
    });
    let animated = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(after, animated, "the fire advances to another frame");
    entity.invisible = true;
    r.set_entities(&[entity]);
    let hidden = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&[]);
    let empty = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(hidden, empty, "invisibility does not hide the fire");
    r.set_on_fire(true);
    let first = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(empty, first, "local fire draws with no hand");
    r.set_on_fire(false);
    assert_eq!(
        empty,
        r.render_to_rgba(&cam, &[]).2,
        "extinguishing clears the overlay"
    );
    if let Some(dir) = std::env::var_os("FIRE_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        for (name, image) in [
            ("before", before),
            ("after", after),
            ("animated", animated),
            ("invisible", hidden),
            ("first-person", first),
        ] {
            encode_png(&dir.join(format!("{name}.png")), 400, 300, &image).unwrap();
        }
    }
}

/// Armor assets dir with a white leather layer 1 and, optionally, an opaque blue overlay.
fn leather_assets(overlay: bool) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ferrocraft-armor-{}-{overlay}", std::process::id()));
    let armor = dir.join("assets/minecraft/textures/models/armor");
    std::fs::create_dir_all(&armor).unwrap();
    let solid = |rgba: [u8; 4]| rgba.repeat(64 * 32);
    encode_png(&armor.join("leather_layer_1.png"), 64, 32, &solid([255; 4])).unwrap();
    if overlay {
        let blue = solid([0, 0, 255, 255]);
        encode_png(&armor.join("leather_layer_1_overlay.png"), 64, 32, &blue).unwrap();
    }
    dir
}

/// A player in a leather tunic (chest slot).
fn player_in_leather() -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [0.0, 64.0, 0.0],
        yaw: 180.0,
        head_yaw: 180.0,
        size: (0.6, 1.8),
        armor: [0, 0, 299, 0],
        armor_dye: [None; 4],
        eye_height: 1.62,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 0.0,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        held: 0,
        using: None,
        name: None,
        skin: None,
        burning: false,
        always_name: false,
        pointed: false,
        ridden: false,
        child: false,
    }
}

/// Vanilla draws with `GL_LEQUAL`, so the leather overlay box, coplanar with the dyed base, shows.
#[test]
fn leather_overlay_draws_over_the_dyed_base() {
    let cam = Camera {
        pos: [0.0, 65.0, -4.0],
        ..Camera::default()
    };
    let frame = |overlay: bool| {
        let dir = leather_assets(overlay);
        let mut r = Renderer::new_offscreen(400, 300, Some(&dir));
        r.set_entities(&[player_in_leather()]);
        let image = r.render_to_rgba(&cam, &[]).2;
        std::fs::remove_dir_all(&dir).ok();
        image
    };
    assert_ne!(
        frame(false),
        frame(true),
        "the overlay layer must be visible"
    );
}
