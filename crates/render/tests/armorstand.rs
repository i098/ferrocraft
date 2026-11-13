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

    // Vanilla: a small stand is half the size (scale 0.5) and both stand centred on their x.
    let bounds = |r: &mut Renderer, d: EntityDraw| {
        r.set_entities(&[d]);
        let img = r.render_to_rgba(&cam, &[]).2;
        let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
        for (i, (a, b)) in img.chunks(4).zip(empty.chunks(4)).enumerate() {
            if a != b {
                let (x, y) = (i % 640, i / 640);
                (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
            }
        }
        assert!(x1 >= x0, "stand drew nothing");
        ((x0 + x1) as f64 / 2.0 - 320.0, (y1 - y0 + 1) as f64)
    };
    let (nc, nh) = bounds(&mut r, stand(class, 0.0, StandLook::default()));
    let (sc, sh) = bounds(&mut r, stand(class, 0.0, arms(1, [None; 6])));
    assert!((0.4..0.6).contains(&(sh / nh)), "small/normal height {sh}/{nh}");
    assert!(nc.abs() < 6.0 && sc.abs() < 6.0, "centre offsets {nc} {sc}");
    // Vanilla: a stand is 1.975 blocks tall; at ~3 blocks distance and 70 degree fov on a 360 px
    // frame that is well over a fifth of the frame and well under the whole frame.
    assert!((72.0..360.0).contains(&nh), "normal stand height {nh}");

    // Arms (flag 4) add geometry at both shoulders, left and right of the body centre.
    let side_diff = |r: &mut Renderer, a: EntityDraw, b: EntityDraw| {
        r.set_entities(&[a]);
        let ia = r.render_to_rgba(&cam, &[]).2;
        r.set_entities(&[b]);
        let ib = r.render_to_rgba(&cam, &[]).2;
        let (mut l, mut rt) = (0usize, 0usize);
        for (i, (p, q)) in ia.chunks(4).zip(ib.chunks(4)).enumerate() {
            if p != q {
                if i % 640 < 320 {
                    l += 1
                } else {
                    rt += 1
                }
            }
        }
        (l, rt)
    };
    let (l, rt) = side_diff(
        &mut r,
        stand(class, 0.0, StandLook::default()),
        stand(class, 0.0, arms(4, [None; 6])),
    );
    assert!(l > 20 && rt > 20, "arms on both sides, got {l} left {rt} right");
    // Posing only the right arm (index 3) must change pixels on one side of the body only.
    let (l, rt) = side_diff(
        &mut r,
        stand(class, 0.0, arms(4, [None; 6])),
        stand(class, 0.0, arms(4, [None, None, None, Some([-80.0, 0.0, 10.0]), None, None])),
    );
    // The stand faces the camera (yaw 180), so its right arm is on the screen's left half.
    assert!(l > 0 && rt == 0, "right arm pose: {l} left {rt} right");
    // Pinhole projection at z = 6, fov 70, height 360: x = 3 sits 3/6 * 180/tan 35 = 128.6 px
    // from the centre (world +x is screen left). Plain stand at x = -3 and armored ghost at x = 3.
    let (pc, _) = bounds(&mut r, stand(class, -3.0, StandLook::default()));
    assert!((pc - 128.6).abs() < 10.0, "plain stand column {pc}");
    let mut g = stand(class, 3.0, StandLook::default());
    g.invisible = true;
    g.armor = [313, 312, 311, 310];
    let (gc, _) = bounds(&mut r, g);
    assert!((gc + 128.6).abs() < 12.0, "armor column {gc}");
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
    assert!(gc.abs() > 100.0, "armor of an invisible stand drew at {gc}");
}
