//! Renders the saddled pig and the armored wither offscreen with the real 1.8.9 assets.
//! `MOBS_PROOF_DIR` keeps `pig.png`, `wither.png` and `wither_plain.png`.
use render::{encode_png, Camera, ChunkBlocks, Class, EntityDraw, Look, Renderer};

struct Flat(i32, i32);
impl ChunkBlocks for Flat {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn light_section(&self, _s: usize, sky: bool) -> Option<&[u8; 2048]> {
        static FULL: [u8; 2048] = [0xFF; 2048];
        sky.then_some(&FULL)
    }
    fn block(&self, _x: usize, y: usize, _z: usize) -> u16 {
        match y {
            0..=61 => 1 << 4,
            62 => 2 << 4,
            _ => 0,
        }
    }
}

fn mob(id: u8, x: f64, z: f64, size: (f32, f32), child: bool) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(id),
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
        child,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look { ..Look::default() },
    }
}


fn shot(draws: &[EntityDraw], cam: &Camera, night: bool) -> Vec<u8> {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("FERROCRAFT_CLIENT_JAR must name the 1.8.9 client jar");
    let mut r = Renderer::new_offscreen(1280, 720, Some(std::path::Path::new(&jar)));
    if night {
        r.set_environment(render::Environment {
            world_time: 13000,
            ..Default::default()
        });
    }
    for cx in -2..=2 {
        for cz in -2..=2 {
            r.upload_chunk(&Flat(cx, cz));
        }
    }
    // The first frame uploads the light the entities read.
    let _ = r.render_to_rgba(cam, &[]);
    r.set_entities(draws);
    r.render_to_rgba(cam, &[]).2
}

fn save(name: &str, rgba: &[u8]) {
    if let Some(dir) = std::env::var_os("MOBS_PROOF_DIR") {
        encode_png(&std::path::PathBuf::from(dir).join(name), 1280, 720, rgba).unwrap();
    }
}

fn cam(y: f64, pitch: f32) -> Camera {
    Camera {
        pos: [0.0, y, 0.0],
        pitch,
        fov_deg: 70.0,
        ..Camera::default()
    }
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn a_saddled_pig_draws_the_saddle() {
    let pig = |saddled: u8| {
        let mut d = mob(90, 0.0, 3.0, (0.9, 0.9), false);
        d.look.variant = saddled;
        d
    };
    let c = cam(64.62, 20.0);
    let plain = shot(&[pig(0)], &c, false);
    let saddled = shot(&[pig(1)], &c, false);
    save("pig.png", &saddled);
    let changed = plain.chunks(4).zip(saddled.chunks(4)).filter(|(a, b)| a != b).count();
    assert!(changed > 300, "the saddle moved {changed} pixels");
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn the_wither_armor_stays_translucent() {
    let wither = |health: f32| {
        let mut d = mob(64, 0.0, 5.0, (0.9, 3.5), false);
        d.pos[1] = 64.0;
        d.look.health = health;
        d
    };
    let c = cam(65.62, 0.0);
    let plain = shot(&[wither(300.0)], &c, false);
    let armored = shot(&[wither(100.0)], &c, false);
    save("wither.png", &armored);
    save("wither_plain.png", &plain);
    let diffs: Vec<u32> = plain
        .chunks(4)
        .zip(armored.chunks(4))
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (0..3).map(|i| a[i].abs_diff(b[i]) as u32).sum::<u32>())
        .collect();
    assert!(diffs.len() > 300, "the armor moved {} pixels", diffs.len());
    let mean = diffs.iter().sum::<u32>() as f32 / diffs.len() as f32;
    eprintln!("armor moved {} pixels, mean rgb shift {mean}", diffs.len());
    assert!(mean < 90.0, "the armor is a dense shell: mean rgb shift {mean}");
}

