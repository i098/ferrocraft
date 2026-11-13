//! Renders armor stands offscreen with the real 1.8.9 assets (`ArmorStandRenderer`).
//! `STAND_PROOF_DIR` keeps `before.png` (the tinted box the client drew for a stand until now) and
//! `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer, StandLook};

/// Flat stone and grass up to y = 62 (the stands stand on it at y = 63).
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

fn stand(class: Class, x: f64, stand: StandLook) -> EntityDraw {
    EntityDraw {
        class,
        pos: [x, 63.0, 6.0],
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
        size: (0.5, 1.975),
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
        eye_height: 1.7,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: stand.flags & 1 != 0,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            stand,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn armor_stands_draw_their_model_pose_and_equipment() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(640, 360, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 64.6, 0.0],
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let class = Class::Object(78);
    let arms = |flags, pose| {
        StandLook {
            flags,
            ..StandLook::default()
        }
        .with_pose(pose)
    };
    // Plain stand, a posed one with arms, a sword and a diamond helmet, a small one with arms and a
    // diamond block on its head, and an invisible one in diamond armor.
    let plain = stand(class, -3.0, StandLook::default());
    let mut posed = stand(
        class,
        -1.0,
        arms(
            4 | 8,
            [
                Some([0.0, 30.0, 0.0]),
                None,
                Some([-40.0, 0.0, -30.0]),
                Some([-80.0, 0.0, 10.0]),
                None,
                Some([20.0, 0.0, 0.0]),
            ],
        ),
    );
    posed.held = 276;
    posed.armor[3] = 310;
    let mut small = stand(class, 1.0, arms(1 | 4, [None; 6]));
    small.armor[3] = 57;
    small.held = 276;
    small.look.stand.head_damage = 0;
    let mut ghost = stand(class, 3.0, StandLook::default());
    ghost.invisible = true;
    ghost.armor = [313, 312, 311, 310];
    let draws = [plain, posed, small, ghost];

    // The first frame meshes the chunks; entities read their light after it.
    let empty = r.render_to_rgba(&cam, &[]).2;
    // What the client drew before: the box of an unported object.
    let boxes: Vec<EntityDraw> = draws
        .iter()
        .map(|d| EntityDraw {
            class: Class::Object(99),
            held: 0,
            armor: [0; 4],
            ..d.clone()
        })
        .collect();
    r.set_entities(&boxes);
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&draws);
    let after = r.render_to_rgba(&cam, &[]).2;
    assert_ne!(before, after, "the model is not the old box");
    if let Some(dir) = std::env::var_os("STAND_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("before.png"), 640, 360, &before).unwrap();
        encode_png(&dir.join("after.png"), 640, 360, &after).unwrap();
    }

    // An invisible stand without equipment draws nothing; its armor still does.
    let mut bare = draws[3].clone();
    bare.armor = [0; 4];
    r.set_entities(&[bare]);
    assert_eq!(empty, r.render_to_rgba(&cam, &[]).2, "invisible body");
    r.set_entities(&[draws[3].clone()]);
    assert_ne!(
        empty,
        r.render_to_rgba(&cam, &[]).2,
        "armor of an invisible stand"
    );

    // The arms flag adds the arms; the small flag shrinks the picture.
    let mut no_arms = draws[1].clone();
    no_arms.look.stand.flags &= !4;
    r.set_entities(&[no_arms]);
    let without = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&[draws[1].clone()]);
    assert_ne!(without, r.render_to_rgba(&cam, &[]).2, "arms flag");
    let mut grown = draws[2].clone();
    grown.look.stand.flags = 4;
    grown.child = false;
    r.set_entities(&[grown]);
    let tall = r.render_to_rgba(&cam, &[]).2;
    r.set_entities(&[draws[2].clone()]);
    assert_ne!(tall, r.render_to_rgba(&cam, &[]).2, "small flag");
}
