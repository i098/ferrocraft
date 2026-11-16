//! Renders an invisible player offscreen with the real 1.8.9 assets: hidden, and translucent for a viewer
//! who sees it (`LivingEntityRenderer.renderModel`, alpha 0.15). `GHOST_PROOF_DIR` keeps the frames.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

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

fn player(ghost: bool) -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [0.0, 63.0, 5.0],
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
        invisible: true,
        size: (0.6, 1.8),
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
        eye_height: 1.0,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            ghost,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn invisible_player_is_translucent_only_for_a_viewer_who_sees_it() {
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
    let mut frame = |e: EntityDraw| {
        r.set_entities(&[e]);
        r.render_to_rgba(&cam, &[]).2
    };
    let hidden = frame(player(false));
    let ghost = frame(player(true));
    let differs = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    assert_eq!(
        hidden, empty,
        "an invisible player with no armor draws nothing"
    );
    assert!(
        differs(&ghost, &empty) > 2000,
        "the translucent body must draw"
    );
    let strong = ghost
        .chunks(4)
        .zip(empty.chunks(4))
        .filter(|(g, e)| (0..3).any(|c| g[c].abs_diff(e[c]) > 60))
        .count();
    assert_eq!(strong, 0, "the body blends at 15%, not opaque");
    if let Some(dir) = std::env::var_os("GHOST_PROOF_DIR") {
        let dir = std::path::PathBuf::from(dir);
        encode_png(&dir.join("hidden.png"), 800, 450, &hidden).unwrap();
        encode_png(&dir.join("ghost.png"), 800, 450, &ghost).unwrap();
    }
}
