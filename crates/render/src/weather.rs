//! Rain and snow, ported from vanilla 1.8.9 `EntityRenderer.renderRainSnow`.

use crate::biome::{precipitation, splashes, Precipitation};
use crate::blocks::{cached, light_opacity, Kind};
use crate::jrandom::JavaRandom;
use crate::mesh::HEIGHT;
use crate::particles::Kind as ParticleKind;
use crate::sky::{Environment, SkyGeometry};

/// Fancy graphics, the 1.8.9 default.
const RANGE: i32 = 10;
/// Vanilla light map coordinates for open sky, as `(sky, block)` of 240: rain uses the raw light,
/// snow `(light * 3 + 0xF000F0) / 4`.
/// shortcut: every column counts as open sky; read the real light if roofed columns look wrong.
const RAIN_LIGHT: [f32; 2] = [1.0, 0.0];
const SNOW_LIGHT: [f32; 2] = [1.0, 0.25];

/// Whether rain stops at this block (`Material.blocksMovement() || isLiquid()`).
pub fn stops_rain(state: u16) -> bool {
    light_opacity(state) > 0
        || matches!(
            cached(state).kind,
            Kind::Opaque | Kind::Cutout | Kind::Water | Kind::Lava | Kind::Shape
        )
}

/// Rain and snow quads around `feet` (the player's position, not the eye), each in the geometry its texture draws.
/// `top(x, z)` is the precipitation height (the y above the highest rain-stopping block) and the
/// column's biome id; `None` for an unloaded column.
pub fn build_precipitation(
    env: &Environment,
    feet: [f64; 3],
    top: impl Fn(i32, i32) -> Option<(i32, u8)>,
) -> (SkyGeometry, SkyGeometry) {
    let (mut rain, mut snow) = (SkyGeometry::default(), SkyGeometry::default());
    if env.rain <= 0.0 {
        return (rain, snow);
    }
    let (i, j, k) = (
        feet[0].floor() as i32,
        feet[1].floor() as i32,
        feet[2].floor() as i32,
    );
    for z in k - RANGE..=k + RANGE {
        for x in i - RANGE..=i + RANGE {
            // The vanilla offset table holds NaN at the centre column, which draws nothing.
            if (x, z) == (i, k) {
                continue;
            }
            let Some((ground, biome)) = top(x, z) else {
                continue;
            };
            let (low, high) = ((j - RANGE).max(ground), (j + RANGE).max(ground));
            if low == high {
                continue;
            }
            // `renderRainSnow` reads the temperature at the slab's bottom.
            let heights = (low as f32, high as f32);
            match precipitation(biome, x, low, z) {
                Some(Precipitation::Rain) => rain.quad(column_quad(env, feet, (x, z), heights)),
                Some(Precipitation::Snow) => snow.quad(snow_quad(env, feet, (x, z), heights)),
                None => {}
            }
        }
    }
    for v in &mut rain.vertices {
        v.light = RAIN_LIGHT;
    }
    for v in &mut snow.vertices {
        v.light = SNOW_LIGHT;
    }
    (rain, snow)
}

type Corner = ([f32; 3], [f32; 2], [f32; 4]);

/// One rain slab through the middle of column `(x, z)` between heights `low` and `high`.
fn column_quad(
    env: &Environment,
    feet: [f64; 3],
    (x, z): (i32, i32),
    (low, high): (f32, f32),
) -> [Corner; 4] {
    let (dx, dz) = (
        x as f32 - feet[0].floor() as f32,
        z as f32 - feet[2].floor() as f32,
    );
    let len = (dx * dx + dz * dz).sqrt();
    let (ox, oz) = (-dz / len * 0.5, dx / len * 0.5);
    let seed = (x.wrapping_mul(x).wrapping_mul(3121)).wrapping_add(x.wrapping_mul(45_238_971))
        ^ (z.wrapping_mul(z).wrapping_mul(418_711)).wrapping_add(z.wrapping_mul(13_761));
    let mut rng = JavaRandom::new(i64::from(seed));
    // `rendererUpdateCount` is a Java int.
    let tick = (env.ticks as i32).wrapping_add(column_hash(x, z)) & 31;
    let phase = (tick as f32 + env.partial) / 32.0 * (3.0 + rng.next_float());
    let (cx, cz) = (
        x as f32 + 0.5 - feet[0] as f32,
        z as f32 + 0.5 - feet[2] as f32,
    );
    let f3 = (cx * cx + cz * cz).sqrt() / RANGE as f32;
    let color = [1.0, 1.0, 1.0, ((1.0 - f3 * f3) * 0.5 + 0.5) * env.rain];
    let (xf, zf) = (x as f32 + 0.5, z as f32 + 0.5);
    // Each corner's v follows its own height, so the texture moves down as `phase` grows.
    [
        ([xf - ox, low, zf - oz], [0.0, low * 0.25 + phase], color),
        ([xf + ox, low, zf + oz], [1.0, low * 0.25 + phase], color),
        ([xf + ox, high, zf + oz], [1.0, high * 0.25 + phase], color),
        ([xf - ox, high, zf - oz], [0.0, high * 0.25 + phase], color),
    ]
}

/// One snow slab; the same slab as rain, scrolled slowly and jittered by the column's random numbers.
fn snow_quad(
    env: &Environment,
    feet: [f64; 3],
    (x, z): (i32, i32),
    (low, high): (f32, f32),
) -> [Corner; 4] {
    let (dx, dz) = (
        x as f32 - feet[0].floor() as f32,
        z as f32 - feet[2].floor() as f32,
    );
    let len = (dx * dx + dz * dz).sqrt();
    let (ox, oz) = (-dz / len * 0.5, dx / len * 0.5);
    let seed = (x.wrapping_mul(x).wrapping_mul(3121)).wrapping_add(x.wrapping_mul(45_238_971))
        ^ (z.wrapping_mul(z).wrapping_mul(418_711)).wrapping_add(z.wrapping_mul(13_761));
    let mut rng = JavaRandom::new(i64::from(seed));
    let age = (env.ticks as i32) as f32 + env.partial;
    let scroll = (((env.ticks as i32) & 511) as f32 + env.partial) / 512.0;
    let u0 = rng.next_double();
    let (g0, g1) = rng.next_gaussian_pair();
    let u = u0 + f64::from(age * 0.01) * f64::from(g0 as f32);
    let v = rng.next_double() + f64::from(age * g1 as f32) * 0.001;
    let (u, v) = (u as f32, scroll + v as f32);
    let (cx, cz) = (
        x as f32 + 0.5 - feet[0] as f32,
        z as f32 + 0.5 - feet[2] as f32,
    );
    let f6 = (cx * cx + cz * cz).sqrt() / RANGE as f32;
    let color = [1.0, 1.0, 1.0, ((1.0 - f6 * f6) * 0.3 + 0.5) * env.rain];
    let (xf, zf) = (x as f32 + 0.5, z as f32 + 0.5);
    [
        ([xf - ox, low, zf - oz], [u, low * 0.25 + v], color),
        ([xf + ox, low, zf + oz], [1.0 + u, low * 0.25 + v], color),
        ([xf + ox, high, zf + oz], [1.0 + u, high * 0.25 + v], color),
        ([xf - ox, high, zf - oz], [u, high * 0.25 + v], color),
    ]
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

/// `EntityRenderer.addRainParticles`: the splashes (and smoke over lava) one client tick adds around
/// the player. `option` is `gameSettings.particles` (0 all, 1 decreased, 2 minimal); `top(x, z)` is
/// the precipitation height and biome id; `block(x, y, z)` a block state id.
pub fn rain_particles(
    rain: f32,
    fancy: bool,
    option: i32,
    ticks: u64,
    feet: [f64; 3],
    top: impl Fn(i32, i32) -> Option<(i32, u8)>,
    block: impl Fn(i32, i32, i32) -> u16,
) -> Vec<(ParticleKind, [f64; 3])> {
    let strength = if fancy { rain } else { rain / 2.0 };
    if strength == 0.0 {
        return Vec::new();
    }
    let mut rng = JavaRandom::new((ticks as i64).wrapping_mul(312_987_231));
    let (fx, fy, fz) = (
        feet[0].floor() as i32,
        feet[1].floor() as i32,
        feet[2].floor() as i32,
    );
    let count = match option {
        1 => ((100.0 * strength * strength) as i32) >> 1,
        2 => 0,
        _ => (100.0 * strength * strength) as i32,
    };
    let mut out = Vec::new();
    for _ in 0..count {
        let x = fx + rng.next_int(10) - rng.next_int(10);
        let z = fz + rng.next_int(10) - rng.next_int(10);
        let Some((y, biome)) = top(x, z) else {
            continue;
        };
        if y > fy + 10 || y < fy - 10 || !splashes(biome, x, y, z) {
            continue;
        }
        let state = block(x, y - 1, z);
        let (dx, dz) = (rng.next_double(), rng.next_double());
        out.extend(splash(state, x, y, z, dx, dz));
    }
    out
}

/// The particle one rain drop makes on the block `state` below column height `y`, if any.
fn splash(
    state: u16,
    x: i32,
    y: i32,
    z: i32,
    dx: f64,
    dz: f64,
) -> Option<(ParticleKind, [f64; 3])> {
    if cached(state).kind == Kind::Lava {
        let pos = [
            f64::from(x) + dx,
            f64::from(y as f32 + 0.1),
            f64::from(z) + dz,
        ];
        return Some((ParticleKind::Smoke, pos));
    }
    if state >> 4 == 0 {
        return None;
    }
    let max_y = crate::shapes::boxes(state, &|_, _, _| 0)
        .iter()
        .map(|b| b.max[1])
        .fold(0.0f32, f32::max);
    let max_y = if max_y > 0.0 { max_y } else { 1.0 };
    let pos = [
        f64::from(x) + dx,
        f64::from((y - 1) as f32 + 0.1) + f64::from(max_y),
        f64::from(z) + dz,
    ];
    Some((ParticleKind::Droplet, pos))
}

#[cfg(test)]
mod rain_tests {
    use super::*;

    const GRASS: u16 = 2 << 4;

    #[test]
    fn each_biome_fills_only_its_own_geometry() {
        let env = Environment {
            rain: 1.0,
            ..Environment::default()
        };
        let counts = |biome: u8| {
            let (rain, snow) =
                build_precipitation(&env, [0.5, 70.0, 0.5], |_, _| Some((64, biome)));
            (rain.vertices.len(), snow.vertices.len())
        };
        assert!(matches!(counts(1), (n, 0) if n > 0), "plains rain");
        assert!(matches!(counts(12), (0, n) if n > 0), "ice plains snow");
        assert_eq!(counts(2), (0, 0), "desert stays dry");
    }

    #[test]
    fn texture_scrolls_down_the_slab() {
        // v grows with height, so a growing phase moves the drops and flakes down, as in vanilla.
        let env = Environment {
            rain: 1.0,
            ..Environment::default()
        };
        for biome in [1, 12] {
            let (rain, snow) =
                build_precipitation(&env, [0.5, 70.0, 0.5], |_, _| Some((64, biome)));
            let v = if biome == 1 { rain } else { snow }.vertices;
            let (bottom, top) = (&v[0], &v[2]);
            assert!(
                bottom.pos[1] < top.pos[1] && bottom.uv[1] < top.uv[1],
                "biome {biome}"
            );
        }
    }

    #[test]
    fn splashes_land_on_the_ground_and_stop_without_rain() {
        let flat = |_, _| Some((64, 1));
        let ground = |_: i32, y: i32, _: i32| if y == 63 { GRASS } else { 0 };
        let drops = rain_particles(1.0, true, 0, 7, [0.5, 64.0, 0.5], flat, ground);
        assert!(drops.len() > 50);
        assert!(drops
            .iter()
            .all(|(k, p)| *k == ParticleKind::Droplet && (p[1] - 64.1).abs() < 1e-4));
        assert!(rain_particles(0.0, true, 0, 7, [0.5, 64.0, 0.5], flat, ground).is_empty());
        assert!(rain_particles(1.0, true, 2, 7, [0.5, 64.0, 0.5], flat, ground).is_empty());
    }

    #[test]
    fn columns_far_above_or_below_are_skipped() {
        let high = |_, _| Some((80, 1));
        assert!(
            rain_particles(1.0, true, 0, 7, [0.5, 64.0, 0.5], high, |_, _, _| GRASS).is_empty()
        );
    }
}
