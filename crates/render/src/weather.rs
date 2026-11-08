//! Rain, ported from vanilla 1.8.9 `EntityRenderer.renderRainSnow`.
//! Biome data does not reach the renderer, so every biome counts as plains (temperature 0.8): precipitation is always rain, never snow.

use crate::blocks::{cached, light_opacity, Kind};
use crate::mesh::HEIGHT;
use crate::sky::{Environment, SkyGeometry};

/// Fancy graphics, the 1.8.9 default.
const RANGE: i32 = 10;
/// Vanilla light map coordinates for open sky: `(15 * 16 * 3 + 240) / 4` of 240 sky, `(0 + 240) / 4` of 240 block.
const RAIN_LIGHT: [f32; 2] = [1.0, 0.25];

/// `java.util.Random`, which vanilla seeds per column.
struct JavaRandom(u64);

impl JavaRandom {
    fn new(seed: i64) -> Self {
        JavaRandom((seed as u64 ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
    }

    fn next_float(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> 24) as f32 / (1 << 24) as f32
    }
}

/// Whether rain stops at this block (`Material.blocksMovement() || isLiquid()`).
pub fn stops_rain(state: u16) -> bool {
    light_opacity(state) > 0
        || matches!(
            cached(state).kind,
            Kind::Opaque | Kind::Cutout | Kind::Water | Kind::Lava | Kind::Shape
        )
}

/// Rain quads around `feet` (the player's position, not the eye). `top(x, z)` is the precipitation height: the y above the highest rain-stopping block, `None` for an unloaded column.
pub fn build_rain(
    env: &Environment,
    feet: [f64; 3],
    top: impl Fn(i32, i32) -> Option<i32>,
) -> SkyGeometry {
    let mut g = SkyGeometry::default();
    if env.rain <= 0.0 {
        return g;
    }
    let (i, j, k) = (
        feet[0].floor() as i32,
        feet[1].floor() as i32,
        feet[2].floor() as i32,
    );
    // `rendererUpdateCount` is a Java int.
    let counter = env.ticks as i32;
    for z in k - RANGE..=k + RANGE {
        for x in i - RANGE..=i + RANGE {
            let (dx, dz) = ((x - i) as f32, (z - k) as f32);
            let len = (dx * dx + dz * dz).sqrt();
            // The vanilla offset table holds NaN at the centre column, which draws nothing.
            if len == 0.0 {
                continue;
            }
            let (ox, oz) = (-dz / len * 0.5, dx / len * 0.5);
            let Some(ground) = top(x, z) else { continue };
            let low = (j - RANGE).max(ground);
            let high = (j + RANGE).max(ground);
            if low == high {
                continue;
            }
            let seed = (x.wrapping_mul(x).wrapping_mul(3121))
                .wrapping_add(x.wrapping_mul(45_238_971))
                ^ (z.wrapping_mul(z).wrapping_mul(418_711)).wrapping_add(z.wrapping_mul(13_761));
            let mut rng = JavaRandom::new(i64::from(seed));
            let phase = ((counter.wrapping_add(column_hash(x, z)) & 31) as f32 + env.partial)
                / 32.0
                * (3.0 + rng.next_float());
            let (cx, cz) = (
                x as f32 + 0.5 - feet[0] as f32,
                z as f32 + 0.5 - feet[2] as f32,
            );
            let f3 = (cx * cx + cz * cz).sqrt() / RANGE as f32;
            let alpha = ((1.0 - f3 * f3) * 0.5 + 0.5) * env.rain;
            let (xf, zf) = (x as f32 + 0.5, z as f32 + 0.5);
            let (low_f, high_f) = (low as f32, high as f32);
            let color = [1.0, 1.0, 1.0, alpha];
            g.quad([
                (
                    [xf - ox, high_f, zf - oz],
                    [0.0, low_f * 0.25 + phase],
                    color,
                ),
                (
                    [xf + ox, high_f, zf + oz],
                    [1.0, low_f * 0.25 + phase],
                    color,
                ),
                (
                    [xf + ox, low_f, zf + oz],
                    [1.0, high_f * 0.25 + phase],
                    color,
                ),
                (
                    [xf - ox, low_f, zf - oz],
                    [0.0, high_f * 0.25 + phase],
                    color,
                ),
            ]);
        }
    }
    for v in &mut g.vertices {
        v.light = RAIN_LIGHT;
    }
    g
}

/// `x*x*3121 + x*45238971 + z*z*418711 + z*13761` as a Java int.
fn column_hash(x: i32, z: i32) -> i32 {
    x.wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(418_711))
        .wrapping_add(z.wrapping_mul(13_761))
}

/// Highest block of a column: `blocks` is one chunk in `mesh::index` order.
pub fn precipitation_height(blocks: &[u16], x: usize, z: usize) -> i32 {
    (0..HEIGHT)
        .rev()
        .find(|&y| stops_rain(blocks[crate::mesh::index(x, y, z)]))
        .map_or(0, |y| y as i32 + 1)
}
