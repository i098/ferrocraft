//! Skull types survive a rebuild of every chunk (`RenderGlobal.loadRenderers`): the server sends
//! the block entity data once. `SKULL_OUT=<dir>` keeps `before.png` (old full clear) and `after.png`.

use render::{encode_png, Camera, ChunkBlocks, Renderer, Skull};
use std::path::Path;

/// Grass ground with five floor skulls in a row at z 4.
struct Row(i32, i32);

impl ChunkBlocks for Row {
    fn chunk_x(&self) -> i32 {
        self.0
    }
    fn chunk_z(&self) -> i32 {
        self.1
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        match y {
            0..=62 => 1 << 4,
            63 => 2 << 4,
            64 if (self.0, self.1, z) == (0, 0, 4) && x < 5 => 144 << 4 | 1,
            _ => 0,
        }
    }
}

fn upload_all(r: &mut Renderer) {
    for cx in -1..=1 {
        for cz in -1..=1 {
            r.upload_chunk(&Row(cx, cz));
        }
    }
}

fn frame(r: &mut Renderer) -> Vec<u8> {
    let cam = Camera {
        pos: [2.5, 66.0, 9.0],
        yaw: 180.0,
        pitch: 18.0,
        ..Camera::default()
    };
    // The first frame builds the chunk light that the skulls read.
    r.set_block_entities(cam.pos, 0, 0.0);
    r.render_to_rgba(&cam, &[]);
    r.set_block_entities(cam.pos, 0, 0.0);
    r.render_to_rgba(&cam, &[]).2
}

#[test]
#[ignore = "requires the vanilla 1.8.9 client assets and a GPU or software adapter"]
fn skull_types_survive_a_chunk_rebuild() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client assets");
    let mut r = Renderer::new_offscreen(480, 270, Some(Path::new(&jar)));
    for x in 0..5 {
        let kind = x as u8;
        r.set_skull(
            [x, 64, 4],
            Skull {
                kind,
                rotation: 8,
                owner: None,
            },
        );
    }
    upload_all(&mut r);
    let first = frame(&mut r);

    r.drop_chunk_meshes();
    upload_all(&mut r);
    let after = frame(&mut r);
    assert_eq!(first, after, "the rebuild keeps the skull types");

    // What the client did until now: a full clear lost every skull type.
    r.clear_chunks();
    upload_all(&mut r);
    let before = frame(&mut r);
    assert_ne!(first, before, "a full clear loses the skull types");

    if let Some(dir) = std::env::var_os("SKULL_OUT") {
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        for (name, rgba) in [("before", &before), ("after", &after)] {
            encode_png(&dir.join(format!("{name}.png")), 480, 270, rgba).unwrap();
        }
    }
}
