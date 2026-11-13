//! Offscreen renders of chests, signs, skulls and banners with the real 1.8.9 assets.
//! `BLOCKENT_OUT=<dir>` keeps the PNGs.

use render::{decode_png, encode_png, Banner, Camera, ChunkBlocks, Renderer, Skull, WornHead};
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

/// Looks at (`x`, 64, `z`) from the south, 5 blocks away.
fn cam(x: f64, z: f64) -> Camera {
    Camera {
        pos: [x, 66.0, z + 5.0],
        yaw: 180.0,
        pitch: 18.0,
        ..Camera::default()
    }
}

fn frame(r: &mut Renderer, cam: &Camera, name: &str) -> Vec<u8> {
    r.set_block_entities(cam.pos, 0, 0.0);
    let (w, h, rgba) = r.render_to_rgba(cam, &[]);
    if let Some(dir) = std::env::var_os("BLOCKENT_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        encode_png(&dir.join(format!("{name}.png")), w, h, &rgba).unwrap();
    }
    rgba
}

fn differs(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count()
}

const CHEST: u16 = 54 << 4;
const ENDER: u16 = 130 << 4;

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn chests_single_double_and_ender_open_and_closed() {
    let mut r = scene();
    let empty = frame(&mut r, &cam(3.5, 4.5), "chests_none");
    // Single chest facing south, a double chest (x 4 and 5) and an ender chest, all facing south.
    for (x, state) in [
        (0, CHEST | 3),
        (2, CHEST | 3),
        (3, CHEST | 3),
        (5, ENDER | 3),
    ] {
        r.set_block(x, 64, 4, state);
    }
    let closed = frame(&mut r, &cam(3.5, 4.5), "chests_closed");
    assert!(differs(&empty, &closed) > 2000, "chests draw");

    for (x, id) in [(0, 54), (2, 54), (3, 54), (5, 130)] {
        r.block_action([x, 64, 4], id, 1, 1);
    }
    for _ in 0..5 {
        r.tick_block_entities();
    }
    let open = frame(&mut r, &cam(3.5, 4.5), "chests_half_open");
    assert!(differs(&closed, &open) > 300, "the lids lift");
    // After the chest closes the frame returns.
    for (x, id) in [(0, 54), (2, 54), (3, 54), (5, 130)] {
        r.block_action([x, 64, 4], id, 1, 0);
    }
    for _ in 0..12 {
        r.tick_block_entities();
    }
    // Two renders of one unchanged scene already differ by up to 3 per channel (GPU rounding), so compare with that slack.
    let again = frame(&mut r, &cam(3.5, 4.5), "chests_closed_again");
    let worst = closed
        .iter()
        .zip(&again)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        worst <= 3,
        "the closed lid returns to the closed frame (worst {worst})"
    );
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn signs_standing_and_wall_with_four_lines() {
    let mut r = scene();
    let empty = frame(&mut r, &cam(2.0, 4.5), "signs_none");
    r.set_block(0, 64, 4, 63 << 4 | 8);
    r.set_sign_text(
        [0, 64, 4],
        ["Welcome", "to", "§1Ferrocraft", "§lBold"].map(String::from),
    );
    // A wall sign on the south face of a stone block, facing south.
    r.set_block(3, 64, 4, 1 << 4);
    r.set_block(3, 64, 5, 68 << 4 | 3);
    r.set_sign_text(
        [3, 64, 5],
        ["Line one", "Line two", "Line three", "Line four"].map(String::from),
    );
    let drawn = frame(&mut r, &cam(2.0, 5.5), "signs");
    assert!(differs(&empty, &drawn) > 1500, "signs draw");
    // Past 16 blocks the text is cut but the board stays.
    let far = Camera {
        pos: [2.0, 66.0, 24.0],
        yaw: 180.0,
        pitch: 5.0,
        ..Camera::default()
    };
    frame(&mut r, &far, "signs_far");
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn skulls_and_banners() {
    let mut r = scene();
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").unwrap();
    // The player head uses a real skin: the alex texture stands in for a downloaded one.
    let skin = zip_png(Path::new(&jar), "assets/minecraft/textures/entity/alex.png");
    r.set_player_skin(7, &skin);
    let empty = frame(&mut r, &cam(3.5, 4.5), "heads_none");

    for (x, kind) in [(0, 0u8), (1, 1), (2, 2), (3, 3), (4, 4)] {
        r.set_block(x, 64, 4, 144 << 4 | 1);
        r.set_skull(
            [x, 64, 4],
            Skull {
                kind,
                rotation: 0,
                owner: (kind == 3).then_some(7),
            },
        );
    }
    // A wall skull and a banner with two layers: a red base, a white stripe, a blue border.
    r.set_block(6, 64, 4, 144 << 4 | 3);
    r.set_skull(
        [6, 64, 4],
        Skull {
            kind: 1,
            rotation: 0,
            owner: None,
        },
    );
    r.set_block(8, 64, 4, 176 << 4);
    r.set_banner(
        [8, 64, 4],
        Banner {
            base: 1,
            layers: vec![("ms".into(), 15), ("bo".into(), 4)],
        },
    );
    let drawn = frame(&mut r, &cam(4.0, 4.5), "heads_and_banner");
    assert!(differs(&empty, &drawn) > 2000, "skulls and the banner draw");

    // A skull on a player's head: Steve-sized frame at the origin, 2 blocks away.
    r.set_worn_heads(&[WornHead {
        pos: [0.5, 64.0, 12.5],
        body_yaw: 180.0,
        head_yaw: 20.0,
        pitch: 10.0,
        sneaking: false,
        scale: 0.9375,
        kind: 3,
        owner: Some(7),
    }]);
    let head = Camera {
        pos: [0.5, 65.6, 15.0],
        yaw: 180.0,
        pitch: 0.0,
        ..Camera::default()
    };
    let (w, h, rgba) = r.render_to_rgba(&head, &[]);
    if let Some(dir) = std::env::var_os("BLOCKENT_OUT") {
        encode_png(&PathBuf::from(dir).join("worn_head.png"), w, h, &rgba).unwrap();
    }
}

fn zip_png(jar: &Path, name: &str) -> render::Image {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar).unwrap()).unwrap();
    let mut bytes = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
    decode_png(&bytes).unwrap()
}

/// Chunk (0,0) with a standing sign at (0,64,4) and an optional extra stone block at (9,64,9).
struct WithSign(bool);

impl ChunkBlocks for WithSign {
    fn chunk_x(&self) -> i32 {
        0
    }
    fn chunk_z(&self) -> i32 {
        0
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        match (x, y, z) {
            (0, 64, 4) => 63 << 4 | 8,
            (9, 64, 9) if self.0 => 1 << 4,
            _ => Flat(0, 0).block(x, y, z),
        }
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn sign_text_survives_chunk_reupload_after_block_change() {
    let mut r = scene();
    r.upload_chunk(&WithSign(false));
    let bare = frame(&mut r, &cam(0.5, 4.5), "sign_reupload_bare");
    r.set_sign_text(
        [0, 64, 4],
        ["Keep", "this", "text", "please"].map(String::from),
    );
    let texted = frame(&mut r, &cam(0.5, 4.5), "sign_reupload_text");
    assert!(differs(&bare, &texted) > 200, "text draws");
    // A block change elsewhere in the chunk re-uploads it; vanilla does not resend the sign text.
    r.upload_chunk(&WithSign(true));
    let after = frame(&mut r, &cam(0.5, 4.5), "sign_reupload_after");
    assert!(
        differs(&bare, &after) > 200,
        "sign text still drawn after re-upload"
    );
    assert!(differs(&texted, &after) < 50, "same frame as before re-upload");
}
