//! Renders the sneak pose, a wither skeleton, a zombie villager, an armored skeleton and an XP orb
//! offscreen with the real 1.8.9 assets. `POSE_PROOF_DIR` and `POSE_PROOF_NAME` keep the frame as
//! `<name>.png` (the proof run draws `before` on the commit that came first and `after` on this one).

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

/// Flat stone and grass up to y = 62 (the figures stand on it at y = 63).
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

fn figure(class: Class, x: f64, size: (f32, f32), look: Look) -> EntityDraw {
    EntityDraw {
        class,
        pos: [x, 63.0, 6.0],
        yaw: 180.0,
        head_yaw: 180.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 7.0,
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
        held_dye: None,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: size.1 * 0.9,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look,
    }
}

fn scene() -> Vec<EntityDraw> {
    let player = Class::Player { slim: false };
    let mut sneaker = figure(player, -4.5, (0.6, 1.8), Look::default());
    sneaker.sneaking = true;
    let standing = figure(player, -3.0, (0.6, 1.8), Look::default());
    let wither = Look {
        variant: 1,
        ..Look::default()
    };
    let mut armored = figure(Class::Mob(51), -1.5, (0.6, 1.99), Look::default());
    armored.armor = [309, 308, 307, 306];
    let villager = Look {
        variant: 1,
        ..Look::default()
    };
    let mut orb = figure(Class::Orb, 4.5, (0.5, 0.5), Look::default());
    orb.pos[1] = 64.0;
    orb.look.xp = 3; //AFTER
    vec![
        sneaker,
        standing,
        armored,
        figure(Class::Mob(51), 0.0, (0.7, 2.4), wither),
        figure(Class::Mob(54), 1.5, (0.6, 1.95), villager),
        figure(Class::Mob(54), 3.0, (0.6, 1.95), Look::default()),
        orb,
    ]
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn pose_variants_change_the_frame() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let empty = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&scene());
    let frame = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(frame, empty, "the figures must be drawn");
    if let (Some(dir), Some(name)) = (
        std::env::var_os("POSE_PROOF_DIR"),
        std::env::var_os("POSE_PROOF_NAME"),
    ) {
        let file = std::path::PathBuf::from(dir)
            .join(name)
            .with_extension("png");
        encode_png(&file, 800, 450, &frame).unwrap();
    }
}
