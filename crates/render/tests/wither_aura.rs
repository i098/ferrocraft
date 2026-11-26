//! `LayerWitherAura` against the vanilla alpha test: the armor pass runs with `alphaFunc(GREATER, 0.1)`,
//! so a texel of alpha 0.1 or less leaves the plain pixel as it was, and any other texel adds 0.5 of its
//! colour (`color(0.5)`, `blendFunc(ONE, ONE)`).

use render::{encode_png, AssetSource, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};
use std::path::{Path, PathBuf};

const ARMOR: &str = "assets/minecraft/textures/entity/wither/wither_armor.png";

/// Flat stone and grass up to y = 62, full light.
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
    fn light_section(&self, _s: usize, _sky: bool) -> Option<&[u8; 2048]> {
        Some(&[0xff; 2048])
    }
}

/// A wither facing the camera with `health` of 300.
fn wither(health: f32) -> EntityDraw {
    let size = (0.9, 3.5);
    let mut d = EntityDraw {
        class: Class::Mob(64),
        pos: [0.0, 63.0, 0.0],
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
        eye_height: size.1 * 0.85,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look::default(),
    };
    d.look.health = health;
    d
}

/// One frame of a wither below half health over a 64x64 armor texture of one colour `(255, 255, 255, alpha)`.
/// `None` draws a full-health wither, which has no armor pass.
fn frame(alpha: Option<u8>) -> Vec<u8> {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let pack: PathBuf = std::env::temp_dir().join(format!("wither-aura-{}", alpha.unwrap_or(0)));
    let file = pack.join(ARMOR);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let texture = [255, 255, 255, alpha.unwrap_or(0)].repeat(64 * 64);
    encode_png(&file, 64, 64, &texture).unwrap();
    let source = AssetSource::layered(&[pack], Path::new(&jar));
    let mut r = Renderer::new_offscreen_from(400, 300, source);
    for cx in -2..=2 {
        for cz in -2..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let cam = Camera {
        pos: [0.0, 65.5, -7.0],
        pitch: 5.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    // The first frame builds the chunk meshes and their light, which entities read.
    r.render_to_rgba(&cam, &[]);
    r.set_entities(&[wither(if alpha.is_some() { 100.0 } else { 300.0 })]);
    r.render_to_rgba(&cam, &[]).2
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn texels_at_or_below_alpha_point_one_leave_the_plain_pixel_unchanged() {
    let plain = frame(None);
    // 25 / 255 = 0.098: dropped. 26 / 255 = 0.102: drawn. (`alphaFunc(GREATER, 0.1)`)
    assert!(frame(Some(0)) == plain, "alpha 0 changed the frame");
    assert!(frame(Some(25)) == plain, "alpha 25/255 changed the frame");
    assert!(
        frame(Some(26)) != plain,
        "alpha 26/255 left the frame as it was"
    );

    // A drawn white texel adds 0.5 to red, green and blue of the pixel below it; the shell has no
    // culling and the depth test is LEQUAL, so a pixel where two faces draw adds 0.5 once per face.
    let solid = frame(Some(255));
    let changed: Vec<_> = solid
        .chunks(4)
        .zip(plain.chunks(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .collect();
    assert!(changed.len() > 2000, "pixels with armor: {}", changed.len());
    for (a, b) in changed {
        for c in 0..3 {
            let once = (b[c] as u32 + 128).min(255) as u8;
            assert!(u32::from(a[c]) + 1 >= u32::from(once), "{a:?} over {b:?}");
        }
    }
    // Most pixels carry one face only: they add exactly 0.5.
    let exact = solid
        .chunks(4)
        .zip(plain.chunks(4))
        .filter(|(a, b)| (0..3).all(|c| a[c].abs_diff((b[c] as u32 + 128).min(255) as u8) <= 1))
        .count();
    assert!(exact > 2000, "pixels that add 0.5 once: {exact}");
}
