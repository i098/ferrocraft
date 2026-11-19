//! Block model orientation from the blockstate: the random variant `WeightedBakedModel` picks by
//! `MathHelper.getPositionRandom`, and the axis models of logs, hay bales and quartz pillars.

use super::{face_uv, position_random, rot_uv, DIRS};
use crate::blocks::{EAST, WEST};

/// A cube model turned `x` then `y` quarter turns (clockwise seen from above for `y`), optionally mirrored,
/// with the texels of some model faces turned (`rotation` in the model JSON). Textures do not move with
/// the turn (`uvlock` is off), so the UVs of the model face that lands on a world face are used.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(super) struct Orient {
    x: u8,
    y: u8,
    mirror: bool,
    /// Quarter turns of the texels per model face, `UP, DOWN, NORTH, SOUTH, WEST, EAST`.
    turns: [u8; 6],
}

impl Orient {
    /// A point (relative to the cube centre) of the world mapped back into the unturned model:
    /// the inverse of `ModelRotation`, which turns by `-x` about the x axis, then `-y` about y.
    fn to_model(self, mut p: [f32; 3]) -> [f32; 3] {
        for _ in 0..self.y {
            p = [p[2], p[1], -p[0]];
        }
        for _ in 0..self.x {
            p = [p[0], -p[2], p[1]];
        }
        p
    }

    /// Texture coordinates of the corner `corner` (0 or 1 per axis) of the world face `face`.
    pub(super) fn uv(&self, face: usize, corner: [f32; 3]) -> [f32; 2] {
        if *self == Self::default() {
            return face_uv(face, corner);
        }
        let dir = self.to_model(DIRS[face].map(|d| d as f32));
        let model_face = DIRS.iter().position(|d| d.map(|v| v as f32) == dir);
        let model_face = model_face.expect("quarter turns keep axis directions");
        let c = self.to_model(corner.map(|v| v - 0.5)).map(|v| v + 0.5);
        let [u, v] = face_uv(model_face, c);
        rot_uv(
            [if self.mirror { 1.0 - u } else { u }, v],
            self.turns[model_face],
        )
    }
}

/// `column_side`: the model of a log, hay bale or quartz pillar lying along z (`y` 90 along x).
fn column_side(y: u8) -> Orient {
    let mut turns = [0; 6];
    turns[WEST] = 3;
    turns[EAST] = 1;
    Orient {
        y,
        turns,
        ..Orient::default()
    }
}

/// `WeightedBakedModel.getAlternativeModel`: `getRandomItem(list, Math.abs((int) rand >> 16) % totalWeight)`
/// over `n` variants of weight 1.
fn pick(world: [i32; 3], n: i32) -> u8 {
    let r = (position_random(world[0], world[1], world[2]) as i32) >> 16;
    match r.wrapping_abs() {
        // `Math.abs(Integer.MIN_VALUE)` stays negative and selects the first item.
        a if a < 0 => 0,
        a => (a % n) as u8,
    }
}

/// Axis models: logs, hay bales and quartz pillars (`axis` and `variant` blockstate properties).
fn axis_orient(id: u16, meta: u16) -> Orient {
    match (id, meta >> 2 & 3) {
        (17 | 162 | 170, 1) => column_side(1),
        (17 | 162 | 170, 2) => column_side(0),
        (155, _) if meta == 3 => Orient {
            x: 1,
            y: 1,
            ..Orient::default()
        },
        (155, _) if meta == 4 => Orient {
            x: 1,
            ..Orient::default()
        },
        _ => Orient::default(),
    }
}

/// Random variant lists of the blockstate, picked by the position hash.
fn random_orient(id: u16, world: [i32; 3]) -> Orient {
    let turned = |y: u8| Orient {
        y,
        ..Orient::default()
    };
    match id {
        2 | 3 | 12 | 46 | 111 => turned(pick(world, 4)),
        1 | 7 => {
            let k = pick(world, 4);
            Orient {
                y: k >> 1 << 1,
                mirror: k & 1 == 1,
                ..Orient::default()
            }
        }
        87 => {
            let k = pick(world, 16);
            Orient {
                x: k & 3,
                y: k >> 2,
                ..Orient::default()
            }
        }
        _ => Orient::default(),
    }
}

/// Orientation of the block `state` at world position `world`; `snowy` is the grass block `snowy` property.
pub(super) fn orient(state: u16, world: [i32; 3], snowy: bool) -> Orient {
    let (id, meta) = (state >> 4, state & 15);
    // Snowy grass, coarse dirt and stone variants other than plain stone have a single model.
    let single = (id == 2 && snowy) || (id == 3 && meta != 0) || (id == 1 && meta != 0);
    if single {
        Orient::default()
    } else if matches!(id, 17 | 162 | 170 | 155) {
        axis_orient(id, meta)
    } else {
        random_orient(id, world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinct orientations `random_orient` yields for `id` over a grid of positions.
    fn distinct(id: u16) -> usize {
        let mut seen: Vec<Orient> = Vec::new();
        for x in -20..20 {
            for z in -20..20 {
                let o = random_orient(id, [x, 64, z]);
                if !seen.contains(&o) {
                    seen.push(o);
                }
            }
        }
        seen.len()
    }

    /// Variant counts of `blockstates/{name}.json` `variants.normal` (a list) in the 1.8.9 jar.
    fn jar_variants(jar: &std::path::Path, name: &str) -> usize {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(jar).unwrap()).unwrap();
        let file = zip
            .by_name(&format!("assets/minecraft/blockstates/{name}.json"))
            .unwrap();
        let json: serde_json::Value = serde_json::from_reader(file).unwrap();
        json["variants"]["normal"].as_array().map_or(1, Vec::len)
    }

    #[test]
    fn random_variant_counts_follow_the_blockstates() {
        // tnt, stone, bedrock: 4 variants; netherrack: 16 (`blockstates/*.json` of 1.8.9).
        assert_eq!(
            [distinct(46), distinct(1), distinct(7), distinct(87)],
            [4, 4, 4, 16]
        );
        assert_eq!(
            [distinct(2), distinct(3), distinct(12), distinct(111)],
            [4, 4, 4, 4]
        );
    }

    #[test]
    #[ignore = "requires the vanilla 1.8.9 client jar in FERROCRAFT_CLIENT_JAR"]
    fn jar_blockstates_have_the_counts_the_pick_uses() {
        let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").expect("client jar");
        let jar = std::path::Path::new(&jar);
        let counts: Vec<usize> = ["tnt", "stone", "bedrock", "netherrack"]
            .map(|n| jar_variants(jar, n))
            .into();
        assert_eq!(counts, [4, 4, 4, 16]);
    }
}
