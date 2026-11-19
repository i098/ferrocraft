//! Rain and snow, ported from vanilla 1.8.9 `EntityRenderer.renderRainSnow`.

use crate::biome::{precipitation, splashes, Precipitation};
use crate::blocks::{cached, light_opacity, Kind};
use crate::jrandom::JavaRandom;
use crate::mesh::HEIGHT;
use crate::particles::Kind as ParticleKind;
use crate::sky::{Environment, SkyGeometry};

/// Rain radius in columns (`EntityRenderer.renderRainSnow`: `b0 = 5`, 10 with fancy graphics).
pub fn rain_radius(fancy: bool) -> i32 {
    if fancy {
        10
    } else {
        5
    }
}

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
    range: i32,
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
    for z in k - range..=k + range {
        for x in i - range..=i + range {
            // The vanilla offset table holds NaN at the centre column, which draws nothing.
            if (x, z) == (i, k) {
                continue;
            }
            let Some((ground, biome)) = top(x, z) else {
                continue;
            };
            let (low, high) = ((j - range).max(ground), (j + range).max(ground));
            if low == high {
                continue;
            }
            // `renderRainSnow` reads the temperature at the slab's bottom.
            let heights = (low as f32, high as f32);
            match precipitation(biome, x, low, z) {
                Some(Precipitation::Rain) => {
                    rain.quad(column_quad(env, feet, (x, z), heights, range))
                }
                Some(Precipitation::Snow) => {
                    snow.quad(snow_quad(env, feet, (x, z), heights, range))
                }
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
    range: i32,
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
    let phase = (tick as f32 + env.partial) / 32.0 * (3.0 + rng.next_double() as f32);
    let (cx, cz) = (
        x as f32 + 0.5 - feet[0] as f32,
        z as f32 + 0.5 - feet[2] as f32,
    );
    let f3 = (cx * cx + cz * cz).sqrt() / range as f32;
    let color = [1.0, 1.0, 1.0, ((1.0 - f3 * f3) * 0.5 + 0.5) * env.rain];
    let (xf, zf) = (x as f32 + 0.5, z as f32 + 0.5);
    // Vanilla `renderRainSnow`: the vertex at y = low gets v = low * 0.25 + phase, the one at y = high gets high * 0.25 + phase.
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
    range: i32,
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
    let f6 = (cx * cx + cz * cz).sqrt() / range as f32;
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

/// A world sound a weather step asks for: `(event, position, volume, pitch)`.
pub type WeatherSound = (&'static str, [f64; 3], f32, f32);

/// `EntityRenderer.m` (client ticks) and `rainSoundCounter`.
pub struct RainClock<'a> {
    pub ticks: u64,
    pub sound_counter: &'a mut i32,
}

/// `EntityRenderer.addRainParticles`: the splashes (and smoke over lava) one client tick adds around
/// the player, and the `ambient.weather.rain` sound it may play. `option` is `gameSettings.particles`
/// (0 all, 1 decreased, 2 minimal); `top(x, z)` is the precipitation height and biome id;
/// `block(x, y, z)` a block state id.
pub fn rain_particles(
    rain: f32,
    fancy: bool,
    option: i32,
    clock: RainClock,
    feet: [f64; 3],
    top: impl Fn(i32, i32) -> Option<(i32, u8)>,
    block: impl Fn(i32, i32, i32) -> u16,
) -> (Vec<(ParticleKind, [f64; 3])>, Option<WeatherSound>) {
    let strength = if fancy { rain } else { rain / 2.0 };
    if strength == 0.0 {
        return (Vec::new(), None);
    }
    let mut rng = JavaRandom::new((clock.ticks as i64).wrapping_mul(312_987_231));
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
    // Vanilla keeps one random splash (`d0, d1, d2`) out of `j` by `nextInt(++j) == 0`.
    let (mut picked, mut seen) = ([0.0f64; 3], 0);
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
        let Some((kind, at)) = splash(state, x, y, z, dx, dz) else {
            continue;
        };
        out.push((kind, at));
        if kind == ParticleKind::Droplet {
            seen += 1;
            if rng.next_int(seen) == 0 {
                picked = [at[0], at[1] - 1.0, at[2]];
            }
        }
    }
    let mut sound = None;
    if seen > 0 {
        let play = rng.next_int(3) < *clock.sound_counter;
        *clock.sound_counter += 1;
        if play {
            *clock.sound_counter = 0;
            let covered = top(fx, fz).is_some_and(|(h, _)| h > fy);
            let (volume, pitch) = if picked[1] > f64::from(fy + 1) && covered {
                (0.1, 0.5)
            } else {
                (0.2, 1.0)
            };
            sound = Some(("ambient.weather.rain", picked, volume, pitch));
        }
    }
    (out, sound)
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
                build_precipitation(&env, [0.5, 70.0, 0.5], |_, _| Some((64, biome)), 10);
            (rain.vertices.len(), snow.vertices.len())
        };
        assert!(matches!(counts(1), (n, 0) if n > 0), "plains rain");
        assert!(matches!(counts(12), (0, n) if n > 0), "ice plains snow");
        assert_eq!(counts(2), (0, 0), "desert stays dry");
    }

    #[test]
    fn fast_graphics_halves_the_rain_radius() {
        let env = Environment {
            rain: 1.0,
            ..Environment::default()
        };
        // One quad of 4 vertices per column of the (2r + 1)^2 square, minus the centre.
        let verts = |fancy| {
            let r = rain_radius(fancy);
            build_precipitation(&env, [0.5, 70.0, 0.5], |_, _| Some((64, 1)), r)
                .0
                .vertices
                .len()
        };
        assert_eq!(verts(true), 4 * (21 * 21 - 1));
        assert_eq!(verts(false), 4 * (11 * 11 - 1));
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
                build_precipitation(&env, [0.5, 70.0, 0.5], |_, _| Some((64, biome)), 10);
            let v = if biome == 1 { rain } else { snow }.vertices;
            let (bottom, top) = (&v[0], &v[2]);
            assert!(
                bottom.pos[1] < top.pos[1] && bottom.uv[1] < top.uv[1],
                "biome {biome}"
            );
        }
    }

    #[test]
    fn snow_uv_follows_the_vanilla_formula() {
        // Column (0, 0) seeds `java.util.Random(0)`: nextDouble 0.730967787376657, nextGaussian
        // -1.2895731239084833, nextDouble 0.5504370051176339, nextGaussian 0.6829853173685814 (jshell).
        // At tick 100, partial 0.5: u = 0.7309678 + 100.5 * 0.01 * -1.2895731, and
        // v = 100.5 / 512 + 0.5504370 + 100.5 * 0.6829853 * 0.001, plus low * 0.25 per vertex.
        let env = Environment {
            rain: 1.0,
            ticks: 100,
            partial: 0.5,
            ..Environment::default()
        };
        let q = snow_quad(&env, [3.5, 70.0, 3.5], (0, 0), (64.0, 68.0), 10);
        let (u, v) = (-0.565_053_2_f32, 0.815_366_f32 + 16.0);
        assert!((q[0].1[0] - u).abs() < 1e-4, "{}", q[0].1[0]);
        assert!((q[1].1[0] - (u + 1.0)).abs() < 1e-4);
        assert!((q[0].1[1] - v).abs() < 1e-3, "{}", q[0].1[1]);
        assert!((q[2].1[1] - (v + 1.0)).abs() < 1e-3);
        assert_eq!(q[2].1[0], q[1].1[0]);
        assert_eq!(q[3].1[0], q[0].1[0]);
        assert_eq!(q[1].1[1], q[0].1[1]);
    }

    fn tick(ticks: u64, counter: &mut i32, feet: [f64; 3], option: i32, rain: f32) -> Tick {
        let flat = |_, _| Some((64, 1));
        let ground = |_: i32, y: i32, _: i32| if y == 63 { GRASS } else { 0 };
        let clock = RainClock {
            ticks,
            sound_counter: counter,
        };
        rain_particles(rain, true, option, clock, feet, flat, ground)
    }

    type Tick = (Vec<(ParticleKind, [f64; 3])>, Option<WeatherSound>);

    #[test]
    fn splashes_land_on_the_ground_and_stop_without_rain() {
        let mut c = 0;
        let (drops, _) = tick(7, &mut c, [0.5, 64.0, 0.5], 0, 1.0);
        assert!(drops.len() > 50);
        assert!(drops
            .iter()
            .all(|(k, p)| *k == ParticleKind::Droplet && (p[1] - 64.1).abs() < 1e-4));
        assert!(tick(7, &mut c, [0.5, 64.0, 0.5], 0, 0.0).0.is_empty());
        assert!(tick(7, &mut c, [0.5, 64.0, 0.5], 2, 1.0).0.is_empty());
    }

    #[test]
    fn rain_sound_follows_the_counter_with_vanilla_volume_and_pitch() {
        // Vanilla `nextInt(3) < rainSoundCounter++`: never on the first splash tick, then reset to 0.
        let mut c = 0;
        let (_, first) = tick(7, &mut c, [0.5, 64.0, 0.5], 0, 1.0);
        assert!(first.is_none());
        assert_eq!(c, 1);
        let mut sounds = Vec::new();
        for t in 8..200 {
            sounds.extend(tick(t, &mut c, [0.5, 64.0, 0.5], 0, 1.0).1);
        }
        assert!(sounds.len() > 20, "{}", sounds.len());
        for (name, at, v, p) in &sounds {
            // Player in the open: the loud, normal-pitch variant at the ground the splash lands on.
            assert_eq!((*name, *v, *p), ("ambient.weather.rain", 0.2, 1.0));
            assert!((at[1] - 63.1).abs() < 1e-4);
        }
        // Under a roof above the head and the splash well below: the quiet, low-pitch variant.
        let mut c = 5;
        let (_, s) = tick(9, &mut c, [0.5, 60.0, 0.5], 0, 1.0);
        let (name, _, v, p) = s.expect("counter 5 always passes nextInt(3)");
        assert_eq!((name, v, p), ("ambient.weather.rain", 0.1, 0.5));
        assert_eq!(c, 0);
    }

    #[test]
    fn no_rain_sound_without_a_splash() {
        let mut c = 5;
        let none = |_, _| None;
        let clock = RainClock {
            ticks: 1,
            sound_counter: &mut c,
        };
        let (_, s) = rain_particles(1.0, true, 0, clock, [0.5, 64.0, 0.5], none, |_, _, _| 0);
        assert!(s.is_none());
        assert_eq!(c, 5);
    }

    #[test]
    fn columns_far_above_or_below_are_skipped() {
        let high = |_, _| Some((80, 1));
        let mut c = 0;
        let clock = RainClock {
            ticks: 7,
            sound_counter: &mut c,
        };
        let (drops, s) =
            rain_particles(1.0, true, 0, clock, [0.5, 64.0, 0.5], high, |_, _, _| GRASS);
        assert!(drops.is_empty() && s.is_none());
    }
}
