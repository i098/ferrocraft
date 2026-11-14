//! Renders a player offscreen with the real 1.8.9 assets, with and without `entityShadows`.
//! `SHADOW_PROOF_DIR` keeps `before.png` (option off) and `after.png` (option on).

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

fn player(x: f64, z: f64) -> EntityDraw {
    let size = (0.6, 1.8);
    EntityDraw {
        class: Class::Player { slim: false },
        pos: [x, 63.0, z],
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
        eye_height: size.1 * 0.8,
        always_name: false,
        team_tag: Default::default(),
        pointed: false,
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            variant: 0,
            ..Look::default()
        },
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn a_player_casts_a_shadow_on_the_block_below() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(800, 450, Some(std::path::Path::new(&jar)));
    let cam = Camera {
        pos: [0.0, 65.0, 0.0],
        pitch: 20.0,
        fov_deg: 70.0,
        ..Camera::default()
    };
    for cx in -1..=1 {
        for cz in -1..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // Offscreen rendering waits for chunk meshes and light before we build the entity mesh.
    r.render_to_rgba(&cam, &[]);
    // A player standing on the grass, and one jumping two blocks up, whose shadow is gone.
    r.set_entities(&[
        player(-1.0, 4.0),
        EntityDraw {
            pos: [1.5, 65.5, 4.0],
            ..player(0.0, 0.0)
        },
    ]);
    r.set_entity_shadows(false);
    let before = r.render_to_rgba(&cam, &[]).2;
    r.set_entity_shadows(true);
    let after = r.render_to_rgba(&cam, &[]).2;
    let darker = before
        .chunks(4)
        .zip(after.chunks(4))
        .filter(|(b, a)| a[0] < b[0] && a[1] < b[1] && a[2] < b[2])
        .count();
    let brighter = before
        .chunks(4)
        .zip(after.chunks(4))
        .filter(|(b, a)| a[0] > b[0])
        .count();
    assert!(darker > 200, "the blob darkens the ground: {darker} pixels");
    assert_eq!(brighter, 0, "a shadow never lightens a pixel");
    if let Some(dir) = std::env::var_os("SHADOW_PROOF_DIR") {
        let dir = std::path::Path::new(&dir);
        for (name, rgba) in [("before", &before), ("after", &after)] {
            encode_png(&dir.join(format!("{name}.png")), 800, 450, rgba).unwrap();
        }
    }
}
