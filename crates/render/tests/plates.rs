//! Offscreen render of the blocks that once drew nothing (issue #228): each one must change the pixels around its cell.
//! `FERROCRAFT_CLIENT_JAR` supplies the 1.8.9 textures; `RENDER_PNG` writes the frame.

use render::{Camera, ChunkBlocks, Renderer};

/// `(id, meta)` of the states under test, laid out two rows deep.
const STATES: [(u16, u16); 16] = [
    (69, 5),
    (69, 13),
    (77, 5),
    (77, 13),
    (143, 5),
    (147, 0),
    (147, 3),
    (148, 3),
    (131, 2),
    (131, 14),
    (132, 0),
    (149, 2),
    (150, 10),
    (127, 8),
    (34, 2),
    (34, 11),
];

struct Row(bool);

impl Row {
    fn cell(x: usize, z: usize) -> Option<(u16, u16)> {
        let i = match z {
            6 => x / 2,
            9 => 8 + x / 2,
            _ => return None,
        };
        (x % 2 == 0).then(|| STATES[i])
    }
}

impl ChunkBlocks for Row {
    fn chunk_x(&self) -> i32 {
        0
    }
    fn chunk_z(&self) -> i32 {
        0
    }
    fn block(&self, x: usize, y: usize, z: usize) -> u16 {
        match y {
            60 => 1 << 4,
            61 if self.0 => Self::cell(x, z).map_or(0, |(id, meta)| id << 4 | meta),
            _ => 0,
        }
    }
}

/// Pinhole projection of a world point (see `offscreen.rs`).
fn project(cam: &Camera, p: [f64; 3], w: f64, h: f64) -> (f64, f64) {
    let (sy, cy) = f64::from(cam.yaw).to_radians().sin_cos();
    let (sp, cp) = f64::from(cam.pitch).to_radians().sin_cos();
    let f = [-sy * cp, -sp, cy * cp];
    let r = [-cy, 0.0, -sy];
    let u = [-sy * sp, cp, cy * sp];
    let d = [p[0] - cam.pos[0], p[1] - cam.pos[1], p[2] - cam.pos[2]];
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    let t = (f64::from(cam.fov_deg).to_radians() / 2.0).tan();
    let (x, y) = (dot(r) / dot(f) / (t * w / h), dot(u) / dot(f) / t);
    ((x + 1.0) * w / 2.0, (1.0 - y) * h / 2.0)
}

#[test]
fn redstone_blocks_draw_something() {
    let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
    let mut r = Renderer::new_offscreen(854, 480, jar.as_deref());
    let cam = Camera {
        pos: [7.5, 66.5, 1.0],
        yaw: 0.0,
        pitch: 45.0,
        fov_deg: 80.0,
        ..Camera::default()
    };
    r.upload_chunk(&Row(false));
    let (w, h, base) = r.render_to_rgba(&cam, &[]);
    // Pixels that differ between two identical frames (animated textures, particles) do not count.
    let (_, _, again) = r.render_to_rgba(&cam, &[]);
    r.upload_chunk(&Row(true));
    let (_, _, frame) = r.render_to_rgba(&cam, &[]);

    if let Some(path) = std::env::var_os("RENDER_PNG") {
        render::encode_png(path.as_ref(), w, h, &frame).expect("write png");
    }
    let mut missing = vec![];
    for z in [6, 9] {
        for x in (0..16).step_by(2) {
            let (id, meta) = Row::cell(x, z).unwrap();
            let (cx, cy) = project(
                &cam,
                [x as f64 + 0.5, 61.3, z as f64 + 0.5],
                w as f64,
                h as f64,
            );
            let changed = (-14i32..=14)
                .flat_map(|dy| (-14i32..=14).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| {
                    let i =
                        ((cy as i32 + dy) as usize * w as usize + (cx as i32 + dx) as usize) * 4;
                    base[i..i + 4] != frame[i..i + 4] && base[i..i + 4] == again[i..i + 4]
                })
                .count();
            if changed < 8 {
                missing.push(format!("{id}:{meta} changed {changed} pixels"));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join(", "));
}
