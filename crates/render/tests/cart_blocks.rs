//! Minecarts carrying each block of 1.8.9, offscreen with the real assets (issues 381 and 385).

use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

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

fn draw(class: Class, pos: [f64; 3], size: (f32, f32)) -> EntityDraw {
    EntityDraw {
        class,
        pos,
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
    }
}

/// Carried states with their offsets: chest, ender chest, trapped chest, furnace, hopper, spawner,
/// command block and a custom display block (diamond block).
const BLOCKS: [(&str, u16, i32); 8] = [
    ("chest", 54 << 4 | 2, 8),
    ("ender", 130 << 4 | 2, 8),
    ("trapped", 146 << 4 | 2, 8),
    ("furnace", 61 << 4 | 2, 6),
    ("hopper", 154 << 4, 1),
    ("spawner", 52 << 4, 6),
    ("command", 137 << 4, 6),
    ("custom", 57 << 4, 6),
];

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn every_carried_block_draws() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(480, 240, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.2, 3.2],
        pitch: 35.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    let cart = |x: f64, block: u16, offset: i32| {
        let mut d = draw(Class::Object(10), [x, 63.0, 5.0], (0.98, 0.7));
        d.look.vehicle.block = block;
        d.look.vehicle.block_offset = offset;
        d
    };
    let plain = {
        r.set_entities(&[cart(0.0, 0, 6)]);
        r.render_to_rgba(&cam, &[]).2
    };
    if let Some(dir) = std::env::var_os("CART_PROOF_DIR") {
        let path = std::path::PathBuf::from(dir).join("cart_none.png");
        encode_png(&path, 480, 240, &plain).unwrap();
    }
    for (name, state, offset) in BLOCKS.iter() {
        r.set_entities(&[cart(0.0, *state, *offset)]);
        let with = r.render_to_rgba(&cam, &[]);
        let changed = plain
            .chunks(4)
            .zip(with.2.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 100,
            "{name}: carried block missing ({changed} px)"
        );
        if let Some(dir) = std::env::var_os("CART_PROOF_DIR") {
            let path = std::path::PathBuf::from(dir).join(format!("cart_{name}.png"));
            encode_png(&path, with.0, with.1, &with.2).unwrap();
        }
    }
}
