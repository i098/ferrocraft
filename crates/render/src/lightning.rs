//! Lightning bolts: `RenderLightningBolt.doRender`. Four passes of three jagged strips each, drawn
//! additively in a pale blue; the strips follow a path seeded by the bolt.

use crate::jrandom::JavaRandom;
use crate::mesh::{MeshData, Vertex};

const COLOR: [f32; 4] = [0.45, 0.45, 0.5, 0.3];

/// Client side `LightningBoltEntity`: a bolt flashes for two ticks, then may strike again.
struct Bolt {
    pos: [f64; 3],
    stage: i32,
    seed: i64,
    fire_attempts: i32,
    rng: JavaRandom,
}

/// The lightning bolts of the world and `World.lightningCooldown`.
#[derive(Default)]
pub struct Bolts {
    list: Vec<Bolt>,
    cooldown: i32,
}

impl Bolts {
    /// `handleAddGlobalEntity`; `entropy` seeds the bolt's own random like vanilla's `new Random()`.
    pub fn spawn(&mut self, pos: [f64; 3], entropy: i64) {
        let mut rng = JavaRandom::new(entropy);
        let seed = rng.next_long();
        let fire_attempts = rng.next_int(3) + 1;
        self.list.push(Bolt {
            pos,
            stage: 2,
            seed,
            fire_attempts,
            rng,
        });
    }

    pub fn clear(&mut self) {
        *self = Bolts::default();
    }

    /// One 20 Hz tick: `Minecraft.runTick` counts the cooldown down, then each bolt's `onUpdate`.
    pub fn tick(&mut self) {
        self.cooldown = (self.cooldown - 1).max(0);
        for b in &mut self.list {
            b.stage -= 1;
            if b.stage < 0 && b.fire_attempts > 0 && b.stage < -b.rng.next_int(10) {
                b.fire_attempts -= 1;
                b.stage = 1;
                b.seed = b.rng.next_long();
            }
            if b.stage >= 0 {
                self.cooldown = 2;
            }
        }
        self.list.retain(|b| b.stage >= 0 || b.fire_attempts > 0);
    }

    /// Sky flash strength `0..=1` (`World.getSkyColor` with `lightningCooldown - partialTick`).
    pub fn flash(&self, partial: f32) -> f32 {
        if self.cooldown > 0 {
            (self.cooldown as f32 - partial).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// `(position, seed)` of each bolt, as [`build`] takes them.
    pub fn shapes(&self) -> Vec<([f64; 3], i64)> {
        self.list.iter().map(|b| (b.pos, b.seed)).collect()
    }
}

/// Triangles of every bolt `(position, seed)`, relative to `eye` so far-away worlds keep f32 precision.
pub fn build(bolts: &[([f64; 3], i64)], eye: [f64; 3]) -> MeshData {
    let mut out = MeshData::default();
    for &(pos, seed) in bolts {
        bolt(&mut out, [0, 1, 2].map(|i| pos[i] - eye[i]), seed);
    }
    out
}

fn bolt(out: &mut MeshData, [x, y, z]: [f64; 3], seed: i64) {
    let (mut xs, mut zs) = ([0.0f64; 8], [0.0f64; 8]);
    let (mut sx, mut sz) = (0.0, 0.0);
    let mut rng = JavaRandom::new(seed);
    for i in (0..8).rev() {
        xs[i] = sx;
        zs[i] = sz;
        sx += f64::from(rng.next_int(11) - 5);
        sz += f64::from(rng.next_int(11) - 5);
    }
    for pass in 0..4 {
        let mut rng = JavaRandom::new(seed);
        for j in 0..3 {
            let top = if j > 0 { 7 - j } else { 7 };
            let bottom = if j > 0 { top - 2 } else { 0 };
            let mut dx = xs[top as usize] - sx;
            let mut dz = zs[top as usize] - sz;
            for k in (bottom..=top).rev() {
                let (prev_x, prev_z) = (dx, dz);
                let spread = if j == 0 { 11 } else { 31 };
                dx += f64::from(rng.next_int(spread) - spread / 2);
                dz += f64::from(rng.next_int(spread) - spread / 2);
                let mut upper = 0.1 + f64::from(pass) * 0.2;
                let mut lower = upper;
                if j == 0 {
                    upper *= f64::from(k) * 0.1 + 1.0;
                    lower *= f64::from(k - 1) * 0.1 + 1.0;
                }
                let strip: Vec<Vertex> = (0..5)
                    .flat_map(|corner| {
                        let at = |w: f64| {
                            let (mut cx, mut cz) = (x + 0.5 - w, z + 0.5 - w);
                            if corner == 1 || corner == 2 {
                                cx += w * 2.0;
                            }
                            if corner == 2 || corner == 3 {
                                cz += w * 2.0;
                            }
                            (cx, cz)
                        };
                        let (lx, lz) = at(lower);
                        let (ux, uz) = at(upper);
                        [
                            vertex([lx + dx, y + f64::from(k * 16), lz + dz]),
                            vertex([ux + prev_x, y + f64::from((k + 1) * 16), uz + prev_z]),
                        ]
                    })
                    .collect();
                strip_to_triangles(out, &strip);
            }
        }
    }
}

fn vertex(p: [f64; 3]) -> Vertex {
    Vertex {
        pos: p.map(|v| v as f32),
        uv: crate::sky::white_uv(),
        color: COLOR,
        light: [1.0; 2],
    }
}

/// `GL_TRIANGLE_STRIP`: every second triangle swaps two corners to keep the winding.
fn strip_to_triangles(out: &mut MeshData, strip: &[Vertex]) {
    let base = out.vertices.len() as u32;
    out.vertices.extend_from_slice(strip);
    for i in 0..strip.len() as u32 - 2 {
        let tri = if i % 2 == 0 {
            [i, i + 1, i + 2]
        } else {
            [i + 1, i, i + 2]
        };
        out.indices.extend(tri.map(|v| base + v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_bolt_is_56_strips_of_8_triangles() {
        let m = build(&[([0.0, 64.0, 0.0], 7)], [0.0; 3]);
        // 4 passes; per pass 8 + 3 + 3 strips of 8 triangles.
        assert_eq!(m.indices.len(), 4 * (8 + 3 + 3) * 8 * 3);
    }
}
