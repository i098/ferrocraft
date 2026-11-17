//! `LivingEntityRenderer.renderLayers`: the hurt overlay covers the skin but not the armor layer.
//! Offscreen with the real 1.8.9 assets. `BLOCKENT_OUT=<dir>` keeps the PNGs.

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

/// A player 4 blocks ahead of the camera in iron boots, leggings and chestplate, bare head.
fn player(hurt: bool) -> EntityDraw {
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [0.0, 64.0, 12.0],
        yaw: 0.0,
        head_yaw: 0.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        age: 0.0,
        sneaking: false,
        hurt,
        death_time: 0.0,
        invisible: false,
        size: (0.6, 1.8),
        held: 0,
        armor: [309, 308, 307, 0],
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
        look: Look::default(),
    }
}

fn save(name: &str, frame: &(u32, u32, Vec<u8>)) {
    if let Some(dir) = std::env::var_os("BLOCKENT_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join(name), frame.0, frame.1, &frame.2).unwrap();
    }
}

/// Sum of red minus the mean of green and blue over the pixels of rows `rows` and columns `cols`.
fn redness(
    frame: &(u32, u32, Vec<u8>),
    rows: std::ops::Range<usize>,
    cols: std::ops::Range<usize>,
) -> i64 {
    let w = frame.0 as usize;
    rows.flat_map(|y| cols.clone().map(move |x| (y, x)))
        .map(|(y, x)| {
            let p = &frame.2[(y * w + x) * 4..][..4];
            i64::from(p[0]) - (i64::from(p[1]) + i64::from(p[2])) / 2
        })
        .sum()
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn a_hurt_player_flashes_red_on_the_skin_but_not_on_the_armor() {
    let mut r = scene();
    let cam = Camera {
        pos: [0.0, 65.6, 16.0],
        yaw: 180.0,
        pitch: 0.0,
        ..Camera::default()
    };
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&[]);
    let ground = r.render_to_rgba(&cam, &[]);
    r.set_entities(&[player(false)]);
    let calm = r.render_to_rgba(&cam, &[]);
    r.set_entities(&[player(true)]);
    let hurt = r.render_to_rgba(&cam, &[]);
    save("hurt_calm.png", &calm);
    save("hurt_hurt.png", &hurt);

    let (w, h) = (calm.0 as usize, calm.1 as usize);
    let figure: Vec<usize> = (0..h)
        .filter(|&y| {
            (0..w).any(|x| calm.2[(y * w + x) * 4..][..4] != ground.2[(y * w + x) * 4..][..4])
        })
        .collect();
    let (top, bottom) = (figure[0], figure[figure.len() - 1] + 1);
    let height = bottom - top;
    let mid = w / 2;
    // The head is the top 1/4 of the figure; the chestplate covers 1/4 to 5/8 of it.
    let head = top..top + height / 4;
    let chest = top + height * 3 / 10..top + height / 2;
    let body = mid - height / 12..mid + height / 12;
    assert!(
        redness(&hurt, head.clone(), body.clone()) > redness(&calm, head, body.clone()) + 500,
        "the face turns red"
    );
    let rows: Vec<usize> = chest.collect();
    for y in rows {
        for x in body.clone() {
            let at = (y * w + x) * 4;
            assert_eq!(
                hurt.2[at..at + 4],
                calm.2[at..at + 4],
                "armor pixel ({x}, {y})"
            );
        }
    }
}
