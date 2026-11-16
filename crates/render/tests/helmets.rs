//! `WornSkullLayer`: block and skull helmets on players, offscreen with the real 1.8.9 assets.
//! `BLOCKENT_OUT=<dir>` keeps the PNGs.

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};
use std::path::{Path, PathBuf};

/// Flat ground: stone below y 63, grass at 63 (state ids `id << 4 | meta`).
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
            0..=62 => 1 << 4,
            63 => 2 << 4,
            _ => 0,
        }
    }
}

fn scene() -> Renderer {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(480, 270, Some(Path::new(&jar)));
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    r
}

fn differs(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count()
}

/// A player at (`x`, 64, 12.5) facing south with `helmet` (id, damage) on the head.
fn player(x: f64, helmet: (i16, i16)) -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [x, 64.0, 12.5],
        yaw: 0.0,
        head_yaw: 0.0,
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
        armor: [0, 0, 0, helmet.0],
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
        eye_height: 1.62,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            stand: render::StandLook {
                head_damage: helmet.1,
                ..Default::default()
            },
            ..Look::default()
        },
    }
}

/// `WornSkullLayer`: a pumpkin (block 86) and a player head (item 397, damage 3) on players.
#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn players_wear_block_and_skull_helmets() {
    let mut r = scene();
    let cam = Camera {
        pos: [0.0, 65.6, 16.0],
        yaw: 180.0,
        pitch: 0.0,
        ..Camera::default()
    };
    let bare = [player(-0.8, (0, 0)), player(0.8, (0, 0))];
    let worn = [player(-0.8, (86, 0)), player(0.8, (397, 3))];
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&bare);
    let before = r.render_to_rgba(&cam, &[]);
    r.set_entities(&worn);
    let after = r.render_to_rgba(&cam, &[]);
    if let Some(dir) = std::env::var_os("BLOCKENT_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(
            &dir.join("helmets_before.png"),
            before.0,
            before.1,
            &before.2,
        )
        .unwrap();
        encode_png(&dir.join("helmets_after.png"), after.0, after.1, &after.2).unwrap();
    }
    // Each half of the frame (one player each) changes.
    let half = |rgba: &[u8], left: bool| -> Vec<u8> {
        let w = before.0 as usize * 4;
        rgba.chunks(w)
            .flat_map(|row| if left { &row[..w / 2] } else { &row[w / 2..] }.to_vec())
            .collect()
    };
    for left in [true, false] {
        let n = differs(&half(&before.2, left), &half(&after.2, left));
        assert!(
            n > 300,
            "helmet changes the {} player: {n}",
            if left { "left" } else { "right" }
        );
    }
}
