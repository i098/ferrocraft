//! Biome temperature and precipitation of 1.8.9 (`BiomeGenBase`), for rain, snow and the sky color.

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
