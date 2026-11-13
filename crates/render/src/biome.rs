//! Biome climate of 1.8.9 (`BiomeGenBase`): rain and snow, the sky color and the grass, foliage and water tints.

use crate::assets::{decode_png, AssetSource};
use crate::jrandom::JavaRandom;
use std::sync::LazyLock;

/// Plains: the temperature of biome ids this table does not know, like `BiomeGenBase.getBiome`.
pub const PLAINS_TEMPERATURE: f32 = 0.8;

/// `(temperature, rainy, snowy)` of a biome.
type Climate = (f32, bool, bool);

/// Biome ids and their climate, dumped from the 1.8.9 client; other ids are plains.
const BIOMES: [(&[u8], Climate); 16] = [
    (&[0, 7, 24], (0.5, true, false)),
    (
        &[2, 8, 17, 37, 38, 39, 130, 165, 166, 167],
        (2.0, false, false),
    ),
    (&[3, 20, 25, 34, 131, 162], (0.2, true, false)),
    (&[4, 18, 29, 132, 157], (0.7, true, false)),
    (&[5, 19, 133, 160, 161], (0.25, true, false)),
    (&[9], (0.5, false, false)),
    (&[10, 11, 12, 13, 140], (0.0, false, true)),
    (&[14, 15], (0.9, true, false)),
    (&[21, 22, 23, 149, 151], (0.95, true, false)),
    (&[26], (0.05, false, true)),
    (&[27, 28, 155, 156], (0.6, true, false)),
    (&[30, 31, 158], (-0.5, false, true)),
    (&[32, 33], (0.3, true, false)),
    (&[35], (1.2, false, false)),
    (&[36, 164], (1.0, false, false)),
    (&[163], (1.1, false, false)),
];

/// Climate of biome `id`.
fn biome(id: u8) -> Climate {
    BIOMES
        .iter()
        .find(|(ids, _)| ids.contains(&id))
        .map_or((PLAINS_TEMPERATURE, true, false), |&(_, b)| b)
}

/// Biome ids and their `downfall` (rainfall); other ids are plains.
const DOWNFALL: [(&[u8], f32); 8] = [
    (
        &[2, 8, 17, 35, 36, 37, 38, 39, 130, 163, 164, 165, 166, 167],
        0.0,
    ),
    (&[3, 20, 25, 26, 34, 131, 162], 0.3),
    (&[1, 16, 30, 31, 129, 158], 0.4),
    (&[0, 7, 9, 10, 11, 12, 13, 24, 140], 0.5),
    (&[27, 28, 155, 156], 0.6),
    (
        &[4, 5, 18, 19, 23, 29, 32, 33, 132, 133, 151, 157, 160, 161],
        0.8,
    ),
    (&[6, 21, 22, 134, 149], 0.9),
    (&[14, 15], 1.0),
];

/// Plains downfall, for ids `DOWNFALL` does not list.
const PLAINS_DOWNFALL: f32 = 0.4;

const SWAMP: [u8; 2] = [6, 134];
const ROOFED_FOREST: [u8; 2] = [29, 157];
const MESA: [u8; 6] = [37, 38, 39, 165, 166, 167];

fn downfall(id: u8) -> f32 {
    DOWNFALL
        .iter()
        .find(|(ids, _)| ids.contains(&id))
        .map_or(PLAINS_DOWNFALL, |&(_, d)| d)
}

/// Which tint a block asks the biome for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Grass,
    Foliage,
    Water,
}

/// `textures/colormap/grass.png` and `foliage.png` (`GrassColors`, `FoliageColors`), RGB, row-major.
pub struct Colormaps {
    grass: Vec<u32>,
    foliage: Vec<u32>,
}

impl Colormaps {
    /// Plains colors everywhere, for renders without assets.
    pub fn plains() -> Colormaps {
        Colormaps {
            grass: vec![0x91_bd59; 256 * 256],
            foliage: vec![0x77_ab2f; 256 * 256],
        }
    }

    /// The two maps of the client; a map that is missing or not 256x256 stays plains.
    pub fn load(source: &mut AssetSource) -> Colormaps {
        let mut maps = Colormaps::plains();
        for (name, map) in [("grass", &mut maps.grass), ("foliage", &mut maps.foliage)] {
            let path = format!("assets/minecraft/textures/colormap/{name}.png");
            let image = source.read(&path).and_then(|b| decode_png(&b));
            if let Some(image) = image.filter(|i| (i.width, i.height) == (256, 256)) {
                for (c, p) in map.iter_mut().zip(image.rgba.as_chunks::<4>().0) {
                    *c = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
                }
            }
        }
        maps
    }
}

/// `GrassColors.getColor`: the map is a triangle, rainfall scaled by temperature.
fn sample(map: &[u32], temperature: f32, downfall: f32) -> u32 {
    let t = f64::from(temperature.clamp(0.0, 1.0));
    let rain = f64::from(downfall.clamp(0.0, 1.0)) * t;
    let (x, y) = (
        ((1.0 - t) * 255.0) as usize,
        ((1.0 - rain) * 255.0) as usize,
    );
    map[y << 8 | x]
}

/// `BiomeGenBase.getGrassColor`, `getFoliageColor` and `waterColorMultiplier` at one block.
pub fn color(maps: &Colormaps, tint: Tint, id: u8, x: i32, y: i32, z: i32) -> u32 {
    let (t, d) = (temperature(id, x, y, z), downfall(id));
    match tint {
        Tint::Water if SWAMP.contains(&id) => 0xe0_ffae,
        Tint::Water => 0xff_ffff,
        Tint::Foliage if SWAMP.contains(&id) => 0x6a_7039,
        Tint::Foliage if MESA.contains(&id) => 0x9e_814d,
        Tint::Foliage => sample(&maps.foliage, t, d),
        Tint::Grass if SWAMP.contains(&id) => {
            let n = SWAMP_NOISE.value(f64::from(x) * 0.0225, f64::from(z) * 0.0225);
            if n < -0.1 {
                0x4c_763c
            } else {
                0x6a_7039
            }
        }
        Tint::Grass if MESA.contains(&id) => 0x90_814d,
        Tint::Grass if ROOFED_FOREST.contains(&id) => {
            ((sample(&maps.grass, t, d) & 0xfe_fefe) + 0x28_340a) >> 1
        }
        Tint::Grass => sample(&maps.grass, t, d),
    }
}

/// `BiomeColorHelper.getColor`: the mean of the 3x3 columns around `(x, z)`, each channel truncated.
pub fn blended(
    maps: &Colormaps,
    tint: Tint,
    (x, y, z): (i32, i32, i32),
    biome_at: impl Fn(i32, i32) -> u8,
) -> [f32; 3] {
    let mut sum = [0u32; 3];
    for dz in -1..=1 {
        for dx in -1..=1 {
            let c = color(maps, tint, biome_at(x + dx, z + dz), x + dx, y, z + dz);
            for (s, shift) in sum.iter_mut().zip([16, 8, 0]) {
                *s += c >> shift & 255;
            }
        }
    }
    sum.map(|s| ((s / 9) & 255) as f32 / 255.0)
}

/// What falls on a column in rain: `renderRainSnow` picks rain at 0.15 and above, snow below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precipitation {
    Rain,
    Snow,
}

/// `BiomeGenBase.getFloatTemperature`: above y 64 it drops with height and a noise offset.
pub fn temperature(id: u8, x: i32, y: i32, z: i32) -> f32 {
    let t = biome(id).0;
    if y <= 64 {
        return t;
    }
    let n = (TEMPERATURE_NOISE.value(f64::from(x) / 8.0, f64::from(z) / 8.0) * 4.0) as f32;
    t - (n + y as f32 - 64.0) * 0.05 / 30.0
}

/// Precipitation at `(x, y, z)` in biome `id`; `None` in biomes that are neither rainy nor snowy.
pub fn precipitation(id: u8, x: i32, y: i32, z: i32) -> Option<Precipitation> {
    let (_, rainy, snowy) = biome(id);
    if !rainy && !snowy {
        None
    } else if temperature(id, x, y, z) >= 0.15 {
        Some(Precipitation::Rain)
    } else {
        Some(Precipitation::Snow)
    }
}

/// Whether drops splash at `(x, y, z)`: `canSpawnLightningBolt` (rainy, not snowy) and warm enough.
pub fn splashes(id: u8, x: i32, y: i32, z: i32) -> bool {
    let (_, rainy, snowy) = biome(id);
    rainy && !snowy && temperature(id, x, y, z) >= 0.15
}

/// `BiomeGenBase.temperatureNoise`: `NoiseGeneratorPerlin(new Random(1234L), 1)`, one simplex octave.
static TEMPERATURE_NOISE: LazyLock<Simplex> =
    LazyLock::new(|| Simplex::new(&mut JavaRandom::new(1234)));

/// `BiomeGenBase.field_180281_af`, the swamp grass noise: `NoiseGeneratorPerlin(new Random(2345L), 1)`.
static SWAMP_NOISE: LazyLock<Simplex> = LazyLock::new(|| Simplex::new(&mut JavaRandom::new(2345)));

/// `NoiseGeneratorSimplex`, 2D only.
struct Simplex {
    perm: [usize; 512],
}

/// The x and y of `NoiseGeneratorSimplex.grad3`.
const GRAD: [[f64; 2]; 12] = [
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
    [1.0, 0.0],
    [-1.0, 0.0],
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [0.0, 1.0],
    [0.0, -1.0],
];

impl Simplex {
    fn new(rng: &mut JavaRandom) -> Simplex {
        // The x, y and z offsets are unused in 2D, but drawing them advances the generator.
        for _ in 0..3 {
            rng.next_double();
        }
        let mut perm = [0usize; 512];
        for (i, p) in perm.iter_mut().take(256).enumerate() {
            *p = i;
        }
        for i in 0..256 {
            let j = rng.next_int((256 - i) as i32) as usize + i;
            perm.swap(i, j);
            perm[i + 256] = perm[i];
        }
        Simplex { perm }
    }

    fn value(&self, x: f64, y: f64) -> f64 {
        let sqrt3 = 3f64.sqrt();
        let (f2, g2) = (0.5 * (sqrt3 - 1.0), (3.0 - sqrt3) / 6.0);
        let s = (x + y) * f2;
        let (i, j) = (fast_floor(x + s), fast_floor(y + s));
        let t = f64::from(i + j) * g2;
        let (x0, y0) = (x - (f64::from(i) - t), y - (f64::from(j) - t));
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let corners = [
            (x0, y0, 0, 0),
            (x0 - f64::from(i1) + g2, y0 - f64::from(j1) + g2, i1, j1),
            (x0 - 1.0 + 2.0 * g2, y0 - 1.0 + 2.0 * g2, 1, 1),
        ];
        let (ii, jj) = ((i & 255) as usize, (j & 255) as usize);
        70.0 * corners
            .iter()
            .map(|&(cx, cy, di, dj)| {
                let g = self.perm[ii + di as usize + self.perm[jj + dj as usize]] % 12;
                let t = (0.5 - cx * cx - cy * cy).max(0.0);
                t * t * t * t * (GRAD[g][0] * cx + GRAD[g][1] * cy)
            })
            .sum::<f64>()
    }
}

/// `NoiseGeneratorSimplex.fastFloor`: one below the truncation for every `v <= 0`, even integers.
fn fast_floor(v: f64) -> i32 {
    v as i32 - i32::from(v <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_noise_matches_vanilla() {
        // Printed by the 1.8.9 client: `temperatureNoise.func_151601_a(x / 8, z / 8)`.
        for (x, z, want) in [
            (0, 0, 0.0),
            (100, -37, -0.259_386_627_805_431_04),
            (-2000, 513, 0.320_712_225_845_144_03),
            (12345, 678, -0.699_386_176_565_179_9),
        ] {
            let got = TEMPERATURE_NOISE.value(f64::from(x) / 8.0, f64::from(z) / 8.0);
            assert!((got - want).abs() < 1e-12, "{x} {z}: {got}");
        }
        // Plains at (12345, 100, 678), printed by the same client.
        assert!((temperature(1, 12345, 100, 678) - 0.744_662_6).abs() < 1e-6);
    }

    /// Colors printed by the 1.8.9 client (`BiomeColors.getGrassColor`, `getFoliageColor` and the water
    /// multiplier over a world of one biome), as `[grass, foliage, water]`. Needs the client jar.
    /// Biome id, block position and the `[grass, foliage, water]` colors there.
    type Row = (u8, (i32, i32, i32), [u32; 3]);

    #[test]
    fn tints_match_vanilla() {
        let Some(jar) = std::env::var_os("FERROCRAFT_CLIENT_JAR") else {
            eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
            return;
        };
        let maps = Colormaps::load(&mut AssetSource::open(jar.as_ref()).unwrap());
        let hex = |c: [f32; 3]| c.map(|v| (v * 255.0).round() as u32);
        let rgb = |c: u32| [c >> 16, c >> 8 & 255, c & 255];
        let want: [Row; 40] = [
            (0, (100, 64, -37), [0x8eb971, 0x71a74d, 0xffffff]),
            (0, (0, 70, 0), [0x8db971, 0x71a64e, 0xffffff]),
            (0, (-2000, 100, 513), [0x8db976, 0x71a653, 0xffffff]),
            (0, (12345, 64, 678), [0x8eb971, 0x71a74d, 0xffffff]),
            (1, (100, 64, -37), [0x91bd59, 0x77ab2f, 0xffffff]),
            (1, (0, 70, 0), [0x91bd59, 0x77ab2f, 0xffffff]),
            (1, (-2000, 100, 513), [0x93bb5d, 0x77a935, 0xffffff]),
            (1, (12345, 64, 678), [0x91bd59, 0x77ab2f, 0xffffff]),
            (2, (100, 64, -37), [0xbfb755, 0xaea42a, 0xffffff]),
            (2, (0, 70, 0), [0xbfb755, 0xaea42a, 0xffffff]),
            (2, (-2000, 100, 513), [0xbfb755, 0xaea42a, 0xffffff]),
            (2, (12345, 64, 678), [0xbfb755, 0xaea42a, 0xffffff]),
            (3, (100, 64, -37), [0x8ab689, 0x6da36b, 0xffffff]),
            (3, (0, 70, 0), [0x8ab689, 0x6ca36b, 0xffffff]),
            (3, (-2000, 100, 513), [0x87b58d, 0x69a270, 0xffffff]),
            (3, (12345, 64, 678), [0x8ab689, 0x6da36b, 0xffffff]),
            (6, (100, 64, -37), [0x4c763c, 0x6a7039, 0xe0ffae]),
            (6, (0, 70, 0), [0x6a7039, 0x6a7039, 0xe0ffae]),
            (6, (-2000, 100, 513), [0x4c763c, 0x6a7039, 0xe0ffae]),
            (6, (12345, 64, 678), [0x6a7039, 0x6a7039, 0xe0ffae]),
            (14, (100, 64, -37), [0x55c93f, 0x2bbb0f, 0xffffff]),
            (14, (0, 70, 0), [0x56c83f, 0x2cba11, 0xffffff]),
            (14, (-2000, 100, 513), [0x5dc646, 0x35b717, 0xffffff]),
            (14, (12345, 64, 678), [0x55c93f, 0x2bbb0f, 0xffffff]),
            (21, (100, 64, -37), [0x59c93c, 0x30bb0b, 0xffffff]),
            (21, (0, 70, 0), [0x5ac83d, 0x31ba0c, 0xffffff]),
            (21, (-2000, 100, 513), [0x5fc742, 0x39b814, 0xffffff]),
            (21, (12345, 64, 678), [0x59c93c, 0x30bb0b, 0xffffff]),
            (29, (100, 64, -37), [0x507a32, 0x59ae30, 0xffffff]),
            (29, (0, 70, 0), [0x517932, 0x58ae31, 0xffffff]),
            (29, (-2000, 100, 513), [0x527935, 0x5cad38, 0xffffff]),
            (29, (12345, 64, 678), [0x507a32, 0x59ae30, 0xffffff]),
            (37, (100, 64, -37), [0x90814d, 0x9e814d, 0xffffff]),
            (37, (0, 70, 0), [0x90814d, 0x9e814d, 0xffffff]),
            (37, (-2000, 100, 513), [0x90814d, 0x9e814d, 0xffffff]),
            (37, (12345, 64, 678), [0x90814d, 0x9e814d, 0xffffff]),
            (134, (100, 64, -37), [0x4c763c, 0x6a7039, 0xe0ffae]),
            (134, (0, 70, 0), [0x6a7039, 0x6a7039, 0xe0ffae]),
            (134, (-2000, 100, 513), [0x4c763c, 0x6a7039, 0xe0ffae]),
            (134, (12345, 64, 678), [0x6a7039, 0x6a7039, 0xe0ffae]),
        ];
        for (id, at, [grass, foliage, water]) in want {
            for (tint, color) in [
                (Tint::Grass, grass),
                (Tint::Foliage, foliage),
                (Tint::Water, water),
            ] {
                assert_eq!(
                    hex(blended(&maps, tint, at, |_, _| id)),
                    rgb(color),
                    "{tint:?} of biome {id} at {at:?}"
                );
            }
        }
        // Swamp on the west, desert on the east of x 0; sampled at (0, 64, 5).
        let border = |x: i32, _| if x < 0 { 6 } else { 2 };
        let at = (0, 64, 5);
        let got =
            [Tint::Grass, Tint::Foliage, Tint::Water].map(|t| hex(blended(&maps, t, at, border)));
        assert_eq!(got, [rgb(0xa29f4b), rgb(0x97922f), rgb(0xf4ffe4)]);
    }

    #[test]
    fn cold_biomes_snow_and_dry_biomes_stay_dry() {
        assert_eq!(precipitation(1, 0, 64, 0), Some(Precipitation::Rain));
        assert_eq!(precipitation(12, 0, 64, 0), Some(Precipitation::Snow));
        assert_eq!(precipitation(2, 0, 64, 0), None);
        // Extreme hills (0.2) turn to snow high up.
        assert_eq!(precipitation(3, 0, 64, 0), Some(Precipitation::Rain));
        assert_eq!(precipitation(3, 0, 120, 0), Some(Precipitation::Snow));
        // Snow and dry biomes do not splash.
        assert!(splashes(1, 0, 64, 0) && !splashes(12, 0, 64, 0) && !splashes(2, 0, 64, 0));
    }
}
