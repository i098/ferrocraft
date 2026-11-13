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

/// Synthetic terrain in a given biome.
struct InBiome(i32, i32, u8);

impl ChunkBlocks for InBiome {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        Synthetic(self.0, self.1).block(x, y, z)
    }
    fn biome(&self, _x: usize, _z: usize) -> u8 {
        self.2
    }
}

/// Plains rain, ice plains snow and desert dry weather draw three different frames (vanilla `renderRainSnow`).
#[test]
fn precipitation_follows_the_biome() {
    let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from) else {
        eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
        return;
    };
    let cam = Camera {
        pos: [8.0, 65.62, 8.0],
        yaw: 20.0,
        pitch: 10.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let frame = |biome: u8| {
        let mut r = Renderer::new_offscreen(854, 480, Some(&jar));
        for cx in -1..=1 {
            for cz in 0..=2 {
                r.upload_chunk(&InBiome(cx, cz, biome));
            }
        }
        r.set_environment(Environment {
            rain: 1.0,
            ticks: 40,
            ..Environment::default()
        });
        r.render_to_rgba(&cam, &[]).2
    };
    let (plains, ice, desert) = (frame(1), frame(12), frame(2));
    assert_ne!(plains, ice, "snow must differ from rain");
    assert_ne!(plains, desert, "a dry biome draws no rain");
    if let Some(dir) = std::env::var_os("RENDER_SNOW_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, rgba) in [("rain", &plains), ("snow", &ice), ("dry", &desert)] {
            encode_png(&dir.join(format!("{name}.png")), 854, 480, rgba).unwrap();
        }
    }
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
        glint: false,
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
    let enchanted = frame(
        &mut r,
        0.0,
        Some(Hand {
            held: Some(HeldKind::Item { id: 276, damage: 0 }),
            glint: true,
            ..bare
        }),
    );
    assert_ne!(
        sword, enchanted,
        "an enchanted sword must glint in the hand"
    );

    let mut ps = Particles::new();
    ps.break_block([7, 66, 12], 1 << 4);
    ps.spawn_packet(26, [8.5, 66.0, 11.0], [0.5, 0.3, 0.5], 0.01, 30, &[]);
    ps.spawn_packet(34, [7.0, 66.5, 12.0], [0.5, 0.3, 0.5], 0.0, 8, &[]);
    r.set_particles(
        ps.vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, 0.0, |p| {
            r.light_at(p)
        }),
    );
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
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: render::Look::default(),
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

/// Changed-pixel bounding box `(x0, y0, x1, y1)` of `frame` against `base`.
fn changed_box(base: &[u8], frame: &[u8], w: usize) -> Option<(usize, usize, usize, usize)> {
    let mut b: Option<(usize, usize, usize, usize)> = None;
    for (i, (a, c)) in base.chunks(4).zip(frame.chunks(4)).enumerate() {
        if a != c {
            let (x, y) = (i % w, i / w);
            b = Some(b.map_or((x, y, x, y), |(x0, y0, x1, y1)| {
                (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
            }));
        }
    }
    b
}

/// Pinhole projection of a world point for a Minecraft camera (yaw 0 faces +z, positive pitch looks
/// down, `fov` is vertical), in pixels.
fn project(cam: &Camera, p: [f64; 3], w: f64, h: f64) -> (f64, f64) {
    let (sy, cy) = f64::from(cam.yaw).to_radians().sin_cos();
    let (sp, cp) = f64::from(cam.pitch).to_radians().sin_cos();
    let f = [-sy * cp, -sp, cy * cp];
    let r = [-cy, 0.0, -sy];
    let u = [-sy * sp, cp, cy * sp];
    let d = [p[0] - cam.pos[0], p[1] - cam.pos[1], p[2] - cam.pos[2]];
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    let t = (f64::from(cam.fov_deg).to_radians() / 2.0).tan();
    let (x, y) = (dot(r) / dot(f) / (t * w / h), dot(u) / dot(f) / t);
    ((x + 1.0) * w / 2.0, (1.0 - y) * h / 2.0)
}

/// The elder guardian curse (particle 41) draws in front of the camera, and the item, footprint and
/// barrier particles each draw at their world position; with `RENDER_DIR` set the frames are written.
#[test]
fn curse_and_item_particles_draw() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
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
    let mut frame = |ps: &Particles, partial: f32| {
        r.set_particles(
            ps.vertices(r.atlas(), cam.yaw, cam.pitch, cam.pos, partial, |p| {
                r.light_at(p)
            }),
        );
        r.render_to_rgba(&cam, &[]).2
    };
    let (w, h) = (854usize, 480usize);
    let clear = frame(&Particles::new(), 0.0);
    let mut ps = Particles::new();
    ps.spawn_packet(41, [8.0, 65.0, 8.0], [0.0; 3], 0.0, 0, &[]);
    // The entity exists after the first update: nothing is drawn before it.
    assert_eq!(clear, frame(&ps, 0.5), "no guardian before the first tick");
    ps.tick(|_, _, _| 0);
    let early = frame(&ps, 0.5);
    for _ in 0..13 {
        ps.tick(|_, _, _| 0);
    }
    let mid = frame(&ps, 0.5);
    assert_ne!(early, mid, "the curse fades and turns over its life");
    // The guardian floats 1.5 blocks ahead of the eyes, so it covers the middle of the screen.
    let (_, ey0, _, ey1) = changed_box(&clear, &early, w).expect("the early curse must draw");
    let (x0, y0, x1, y1) = changed_box(&clear, &mid, w).expect("the mid curse must draw");
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
    assert!(
        (w / 4..3 * w / 4).contains(&cx) && (h / 4..3 * h / 4).contains(&cy),
        "mid curse centre ({cx}, {cy}) must be near the screen centre"
    );
    assert!(
        x1 - x0 > w / 8 && y1 - y0 > h / 8,
        "mid curse is a large model"
    );
    // `renderParticle` tilts the model by 60 - 150 * progress - pitch: it swings in from above.
    assert!(
        (ey0 + ey1) / 2 < cy,
        "the early curse sits above the mid curse"
    );

    if jar.is_none() {
        eprintln!(
            "FERROCRAFT_CLIENT_JAR not set; skipping the item, footprint and barrier particles"
        );
        return;
    }
    let mut shots = Vec::new();
    for (id, pos, vel, speed, count, args) in [
        (28, [8.5, 64.01, 11.0], [0.0; 3], 0.0, 0, vec![]),
        (35, [8.5, 66.0, 11.0], [0.0; 3], 0.0, 0, vec![]),
        (31, [8.0, 66.0, 11.0], [0.0; 3], 0.0, 0, vec![]),
        (33, [6.5, 66.0, 11.0], [0.0; 3], 0.0, 0, vec![]),
        (36, [7.5, 66.0, 11.0], [0.1, 0.1, 0.0], 1.0, 8, vec![276]),
    ] {
        let mut one = Particles::new();
        one.spawn_packet(id, pos, vel, speed, count, &args);
        let px = frame(&one, 0.0);
        let (x0, y0, x1, y1) =
            changed_box(&clear, &px, w).unwrap_or_else(|| panic!("particle {id} must draw"));
        let (ex, ey) = project(&cam, pos.map(f64::from), w as f64, h as f64);
        let (cx, cy) = (((x0 + x1) / 2) as f64, ((y0 + y1) / 2) as f64);
        assert!(
            (cx - ex).abs() < 100.0 && (cy - ey).abs() < 100.0,
            "particle {id} drew around ({cx}, {cy}), expected near ({ex:.0}, {ey:.0})"
        );
        shots.push((format!("particle_{id}"), px));
    }

    if let Some(dir) = std::env::var_os("RENDER_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, px) in [
            ("curse_clear", &clear),
            ("curse_early", &early),
            ("curse_mid", &mid),
        ]
        .into_iter()
        .map(|(n, p)| (n.to_string(), p))
        .chain(shots.iter().map(|(n, p)| (n.clone(), p)))
        {
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

/// Night Vision lights a night scene, and Blindness closes the fog in on the camera.
#[test]
fn night_vision_and_blindness_change_the_frame() {
    let frame = |env: Environment| {
        let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
        let mut r = Renderer::new_offscreen(320, 200, jar.as_deref());
        r.set_environment(Environment {
            world_time: 18000,
            ..env
        });
        for cx in -1..=1 {
            for cz in 0..=2 {
                r.upload_chunk(&Synthetic(cx, cz));
            }
        }
        let cam = Camera {
            pos: [8.0, 68.0, -4.0],
            pitch: 25.0,
            ..Camera::default()
        };
        r.render_to_rgba(&cam, &[]).2
    };
    let plain = frame(Environment::default());
    let lit = frame(Environment {
        night_vision: 1.0,
        ..Environment::default()
    });
    let blind = frame(Environment {
        blindness: Some(100),
        ..Environment::default()
    });
    let sum = |px: &[u8]| px.iter().map(|&b| u64::from(b)).sum::<u64>();
    assert!(
        sum(&lit) > sum(&plain),
        "night vision brightens the terrain"
    );
    assert_ne!(plain, blind, "blindness fogs the view");
    assert!(sum(&blind) < sum(&plain), "blindness fog is black");
    if let Some(dir) = std::env::var_os("RENDER_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, px) in [("night", &plain), ("night_vision", &lit), ("blind", &blind)] {
            encode_png(&dir.join(format!("{name}.png")), 320, 200, px).expect("write png");
        }
    }
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
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: render::Look::default(),
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

/// A hurt entity mixes 30% red over its lit texture (`RenderLivingBase.setBrightness`).
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn hurt_entity_flashes_red() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(400, 300, Some(std::path::Path::new(&jar)));
    for cx in -1..=1 {
        for cz in 0..=2 {
            r.upload_chunk(&Synthetic(cx, cz));
        }
    }
    let cam = Camera {
        pos: [8.5, 67.6, -2.0],
        yaw: 0.0,
        pitch: 0.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let mut entity = EntityDraw {
        class: Class::Player { slim: false },
        pos: [8.5, 66.0, 2.0],
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
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: render::Look::default(),
    };
    // Chunk light is known after the first frame, so the entity is lit only from the second.
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&[entity.clone()]);
    let before = r.render_to_rgba(&cam, &[]).2;
    entity.hurt = true;
    r.set_entities(&[entity]);
    let after = r.render_to_rgba(&cam, &[]).2;
    let red_sum = |img: &[u8]| {
        img.chunks(4)
            .map(|p| i64::from(p[0]) - i64::from(p[1]))
            .sum::<i64>()
    };
    assert!(
        red_sum(&after) > red_sum(&before),
        "hurt entity turns redder"
    );
    if let Some(dir) = std::env::var_os("FIGHT_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 400, 300, &before).unwrap();
        encode_png(&dir.join("after.png"), 400, 300, &after).unwrap();
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
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
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
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        view: [0.0; 2],
        line: None,
        look: render::Look::default(),
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

/// Every `shaders/post` effect of the 1.8.9 jar loads and runs between the world and the GUI;
/// `invert` (the enderman view) inverts the world and leaves the GUI alone.
#[test]
fn post_effects_run_between_world_and_gui() {
    let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR") else {
        eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
        return;
    };
    let (w, h) = (400, 240);
    let mut r = Renderer::new_offscreen(w, h, Some(std::path::Path::new(&jar)));
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
    let red = Quad {
        tex: String::new().into(),
        blend: Blend::Alpha,
        pos: [[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]],
        uv: [[0.0; 2]; 4],
        color: [0xFFFF_0000; 4],
    };
    let frame = |r: &mut Renderer| r.render_to_rgba(&cam, std::slice::from_ref(&red)).2;
    let px = |f: &[u8], x: u32, y: u32| f[((y * w + x) * 4) as usize..][..3].to_vec();
    let plain = frame(&mut r);

    r.set_post(Some("shaders/post/invert.json"))
        .expect("invert loads");
    let inverted = frame(&mut r);
    // `invert.json` sets `InverseAmount` to 0.8: mix(c, 1 - c, 0.8).
    for (x, y) in [(w / 2, h - 20), (w - 5, 5), (w / 3, h / 2)] {
        let want: Vec<f32> = px(&plain, x, y)
            .iter()
            .map(|&c| f32::from(c) * 0.2 + f32::from(255 - c) * 0.8)
            .collect();
        let got = px(&inverted, x, y);
        let near = want
            .iter()
            .zip(&got)
            .all(|(a, &b)| (a - f32::from(b)).abs() <= 2.0);
        assert!(near, "({x}, {y}): {got:?} is not 80% inverted {want:?}");
    }
    assert_eq!(
        px(&inverted, 2, 2),
        [255, 0, 0],
        "the GUI draws after the effect"
    );

    r.toggle_post();
    assert_eq!(frame(&mut r), plain, "F4 switches the effect off");

    for name in render::SHADERS {
        r.set_post(Some(&format!("shaders/post/{name}.json")))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_ne!(frame(&mut r), plain, "{name} changes the frame");
    }
    r.set_post(None).unwrap();
    assert_eq!(frame(&mut r), plain, "no effect after `None`");
}

/// Calibrates `project` without assets: a player model draws centred on its world position.
#[test]
fn projection_matches_a_drawn_entity() {
    let mut r = Renderer::new_offscreen(854, 480, None);
    let cam = Camera {
        pos: [8.0, 65.62, 8.0],
        yaw: 20.0,
        pitch: 10.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    let clear = r.render_to_rgba(&cam, &[]).2;
    let at = [5.5, 64.0, 13.0];
    r.set_entities(&[EntityDraw {
        class: Class::Player { slim: false },
        pos: at,
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
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: render::Look::default(),
    }]);
    let px = r.render_to_rgba(&cam, &[]).2;
    let (x0, y0, x1, y1) = changed_box(&clear, &px, 854).expect("the player must draw");
    let (ex, ey) = project(&cam, [at[0], at[1] + 0.9, at[2]], 854.0, 480.0);
    let (cx, cy) = (((x0 + x1) / 2) as f64, ((y0 + y1) / 2) as f64);
    assert!(
        (cx - ex).abs() < 30.0 && (cy - ey).abs() < 30.0,
        "player drew around ({cx}, {cy}), expected near ({ex:.0}, {ey:.0})"
    );
}

/// Held items show their sprite or block in the right hand and dropped items bob as models
/// (`ItemInHandLayer`, `ItemEntityRenderer`); `ITEMS_PROOF_DIR` keeps the frame.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn held_and_dropped_items_draw_as_models() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(640, 360, Some(std::path::Path::new(&jar)));
    for cx in -1..=1 {
        for cz in 0..=2 {
            r.upload_chunk(&Synthetic(cx, cz));
        }
    }
    let player = |x: f64, id: i16| EntityDraw {
        class: Class::Player { slim: false },
        pos: [x, 66.0, 3.0],
        yaw: 180.0 - 50.0,
        head_yaw: 130.0,
        held: id,
        ..player_in_leather()
    };
    let drop = |x: f64, id: i16, count: u8| EntityDraw {
        class: Class::Object(2),
        pos: [x, 66.0, 1.0],
        size: (0.25, 0.25),
        stack: Some(render::item::Stack {
            id,
            damage: 0,
            count,
            glint: false,
        }),
        age: 12.0,
        ..player_in_leather()
    };
    let draws = [
        player(5.0, 276),
        player(7.0, 261),
        player(9.0, 1),
        player(11.0, 346),
        drop(5.0, 276, 1),
        drop(7.0, 1, 3),
        drop(9.0, 322, 1),
    ];
    let cam = Camera {
        pos: [8.0, 67.6, -2.5],
        yaw: 0.0,
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    // The first frame meshes the chunks; entities read their light after it.
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&draws);
    let image = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("ITEMS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("items.png"), 640, 360, &image).unwrap();
    }
    let bare: Vec<EntityDraw> = draws
        .iter()
        .map(|d| EntityDraw {
            held: 0,
            stack: None,
            ..d.clone()
        })
        .collect();
    r.set_entities(&bare);
    let baseline = r.render_to_rgba(&cam, &[]).2;
    for (i, d) in draws.iter().enumerate() {
        let (mid, half) = if matches!(d.class, Class::Player { .. }) {
            (0.9, 45.0)
        } else {
            (0.15, 25.0)
        };
        let (cx, cy) = project(&cam, [d.pos[0], d.pos[1] + mid, d.pos[2]], 640.0, 360.0);
        let mut colours = std::collections::HashSet::new();
        for y in (cy - half).max(0.0) as usize..((cy + half) as usize).min(360) {
            for x in (cx - half).max(0.0) as usize..((cx + half) as usize).min(640) {
                let o = (y * 640 + x) * 4;
                if image[o..o + 3] != baseline[o..o + 3] {
                    colours.insert([image[o], image[o + 1], image[o + 2]]);
                }
            }
        }
        assert!(
            colours.len() >= 4,
            "item {i} drew {} distinct colours near ({cx:.0}, {cy:.0}); a flat tint cannot be a model",
            colours.len()
        );
    }
}

/// Paintings, item frames and the colour map of a frame draw (`RenderPainting`,
/// `RenderItemFrame`, `MapRenderer`); `HANGING_PROOF_DIR` keeps the frame.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn paintings_frames_and_maps_draw() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(640, 360, Some(std::path::Path::new(&jar)));
    for cx in -1..=1 {
        for cz in 0..=2 {
            r.upload_chunk(&Synthetic(cx, cz));
        }
    }
    let hang = |x: f64, y: f64, class, title: &str, stack| EntityDraw {
        class,
        pos: [x, y, 4.97],
        hang: render::hanging::Hang {
            facing: 2,
            title: title.to_owned(),
            rotation: 0,
        },
        stack,
        ..player_in_leather()
    };
    let item = |id, damage| {
        Some(render::item::Stack {
            id,
            damage,
            count: 1,
            glint: false,
        })
    };
    let draws = [
        hang(4.5, 68.5, Class::Painting, "Fighters", None),
        hang(7.5, 67.5, Class::Painting, "Kebab", None),
        hang(9.5, 67.5, Class::Object(71), "", item(276, 0)),
        hang(11.5, 67.5, Class::Object(71), "", item(358, 3)),
    ];
    let colors: Vec<u8> = (0..128 * 128)
        .map(|i| (4 + (i / 128 / 8 + i % 128 / 8) % 140) as u8)
        .collect();
    r.map_update(3, [0, 0, 128, 128], &colors);
    let cam = Camera {
        pos: [8.0, 67.6, -1.0],
        yaw: 0.0,
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    r.render_to_rgba(&cam, &[]);
    let baseline = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&draws);
    let image = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("HANGING_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("hanging.png"), 640, 360, &image).unwrap();
    }
    for d in &draws {
        let (cx, cy) = project(&cam, [d.pos[0], d.pos[1], d.pos[2]], 640.0, 360.0);
        let mut changed = 0;
        for y in (cy - 20.0).max(0.0) as usize..((cy + 20.0) as usize).min(360) {
            for x in (cx - 20.0).max(0.0) as usize..((cx + 20.0) as usize).min(640) {
                let o = (y * 640 + x) * 4;
                changed += usize::from(image[o..o + 3] != baseline[o..o + 3]);
            }
        }
        assert!(
            changed > 100,
            "{:?} at {cx},{cy} changed {changed} pixels",
            d.class
        );
    }
}

/// Primed TNT pulses and flashes white, and a falling block draws its own state
/// (`PrimedTntRenderer`, `FallingBlockRenderer`); `BLOCKS_PROOF_DIR` keeps the frames.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn primed_tnt_and_falling_blocks_draw_as_models() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(640, 360, Some(std::path::Path::new(&jar)));
    for cx in -1..=1 {
        for cz in 0..=2 {
            r.upload_chunk(&Synthetic(cx, cz));
        }
    }
    let object = |class, x: f64, age: f32, stack| EntityDraw {
        class,
        pos: [x, 66.0, 1.0],
        size: (0.98, 0.98),
        stack,
        age,
        ..player_in_leather()
    };
    let block = |id: i16, damage: i16| {
        Some(render::item::Stack {
            id,
            damage,
            count: 1,
            glint: false,
        })
    };
    // Timer 80 flashes (80 / 5 is even), timer 8 does not, timer 3 flashes while it pulses.
    let draws = [
        object(Class::Object(50), 4.0, 0.0, None),
        object(Class::Object(50), 6.0, 72.0, None),
        object(Class::Object(50), 8.0, 77.5, None),
        object(Class::Object(70), 10.0, 3.0, block(12, 0)),
        object(Class::Object(70), 12.0, 3.0, block(12, 1)),
    ];
    let cam = Camera {
        pos: [8.0, 67.6, -3.5],
        yaw: 0.0,
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&draws);
    let image = r.render_to_rgba(&cam, &[]).2;
    if let Some(dir) = std::env::var_os("BLOCKS_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("blocks.png"), 640, 360, &image).unwrap();
    }
    r.set_entities(&[]);
    let baseline = r.render_to_rgba(&cam, &[]).2;
    let mut tints = Vec::new();
    for d in &draws {
        let (cx, cy) = project(&cam, [d.pos[0], d.pos[1] + 0.5, d.pos[2]], 640.0, 360.0);
        let mut colours = std::collections::HashSet::new();
        let mut sum = [0u32; 3];
        for y in (cy - 25.0).max(0.0) as usize..((cy + 25.0) as usize).min(360) {
            for x in (cx - 25.0).max(0.0) as usize..((cx + 25.0) as usize).min(640) {
                let o = (y * 640 + x) * 4;
                if image[o..o + 3] != baseline[o..o + 3] {
                    colours.insert([image[o], image[o + 1], image[o + 2]]);
                    (0..3).for_each(|c| sum[c] += u32::from(image[o + c]));
                }
            }
        }
        assert!(
            colours.len() >= 4,
            "{:?} drew {} colours; a flat tint cannot be a model",
            d.class,
            colours.len()
        );
        tints.push(sum);
    }
    assert_ne!(tints[3], tints[4], "red sand must differ from sand");
}
