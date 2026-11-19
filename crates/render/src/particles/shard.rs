//! The colour `EntityDiggingFX` takes from the block: `colorMultiplier(world, pos)` for a block that is broken or
//! hit (`func_174846_a`), `getRenderColor(state)` for a packet shard (`func_174845_l`). The grass block is skipped.

use super::{Kind, Particle, Particles, Sprite};
use crate::biome::{self, Colormaps, Tint as BiomeTint};
use crate::blocks::{self, Tint};

const GRASS_BLOCK: u16 = 2;

/// Where a shard's block stood, with the biome of the 3x3 columns the tint blends over.
#[derive(Clone, Copy, Debug)]
pub(super) struct Site {
    pos: [i32; 3],
    biomes: [u8; 9],
}

impl Site {
    pub(super) fn new(pos: [i32; 3], biome_at: impl Fn(i32, i32) -> u8) -> Site {
        let mut biomes = [0; 9];
        for (i, b) in biomes.iter_mut().enumerate() {
            *b = biome_at(pos[0] + i as i32 % 3 - 1, pos[2] + i as i32 / 3 - 1);
        }
        Site { pos, biomes }
    }
}

/// The factor per channel for a shard of the block `state`.
pub(super) fn multiplier(maps: &Colormaps, state: u16, site: Option<&Site>) -> [f32; 3] {
    let tint = blocks::look(state).tint;
    if state >> 4 == GRASS_BLOCK {
        return [1.0; 3];
    }
    let biome_tint = match tint {
        Tint::Grass => Some(BiomeTint::Grass),
        Tint::Foliage => Some(BiomeTint::Foliage),
        Tint::Water => Some(BiomeTint::Water),
        _ => None,
    };
    match (biome_tint, site) {
        (Some(bt), Some(s)) => {
            let at = (s.pos[0], s.pos[1], s.pos[2]);
            biome::blended(maps, bt, at, |x, z| {
                let (dx, dz) = (x - at.0 + 1, z - at.2 + 1);
                s.biomes[(dz * 3 + dx) as usize]
            })
        }
        // `ColorizerGrass.getGrassColor(0.5, 1.0)` and `ColorizerFoliage.getFoliageColorBasic()`.
        (Some(BiomeTint::Grass), None) => unpack(biome::sample(&maps.grass, 0.5, 1.0)),
        (Some(BiomeTint::Foliage), None) => unpack(0x48_b518),
        (Some(BiomeTint::Water), None) => [1.0; 3],
        _ => blocks::tint_color(tint),
    }
}

fn unpack(rgb: u32) -> [f32; 3] {
    [16, 8, 0].map(|s| (rgb >> s & 255) as f32 / 255.0)
}

/// The particle with the colour its block gives it, for a block shard; `None` for any other sprite.
pub(super) fn shaded(p: &Particle, maps: &Colormaps) -> Option<Particle> {
    let Sprite::Block { state, site, .. } = p.sprite else {
        return None;
    };
    let m = multiplier(maps, state, site.as_ref());
    Some(Particle {
        color: [0, 1, 2].map(|i| p.color[i] * m[i]),
        ..*p
    })
}

impl Particles {
    /// `EntityDiggingFX` (37) and `EntityBlockDustFX` (38, exact packet motion).
    pub(super) fn block_shard(
        &mut self,
        kind: Kind,
        pos: [f64; 3],
        motion: [f64; 3],
        data: i32,
    ) -> Option<Particle> {
        let state = (data & 0xFFFF) as u16;
        // `EntityBlockDustFX.Factory` makes nothing for a block of render type -1: air, signs, moving piston, end portal, skull, barrier, banners.
        if kind == Kind::BlockDust
            && matches!(state >> 4, 0 | 36 | 63 | 68 | 119 | 144 | 166 | 176 | 177)
        {
            return None;
        }
        let jitter = [(self.rand() * 3.0) as f32, (self.rand() * 3.0) as f32];
        let sprite = Sprite::Block {
            state,
            jitter,
            site: None,
        };
        let mut p = self.base(pos, motion, sprite);
        p.gravity = 1.0;
        p.color = [0.6; 3];
        p.scale /= 2.0;
        if kind == Kind::BlockDust {
            p.vel = motion;
        }
        Some(p)
    }

    /// Records the block site on the shard spawned last.
    pub(super) fn shade_last(&mut self, site: Site) {
        if let Some(Sprite::Block { site: s, .. }) = self.list.last_mut().map(|p| &mut p.sprite) {
            *s = Some(site);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, Particles};
    use super::*;

    #[test]
    fn leaves_shards_are_green_and_grass_blocks_stay_grey() {
        let maps = Colormaps::plains();
        let leaves = 18u16 << 4;
        let site = Site::new([0, 64, 0], |_, _| 1);
        let m = multiplier(&maps, leaves, Some(&site));
        assert!(m[1] > m[0] && m[1] > m[2], "{m:?}");
        assert_eq!(multiplier(&maps, 2 << 4, Some(&site)), [1.0; 3]);
        assert_eq!(multiplier(&maps, 1 << 4, Some(&site)), [1.0; 3]);
        // A packet shard uses the render colour, 0x48b518 for leaves.
        assert_eq!(multiplier(&maps, leaves, None), unpack(0x48_b518));
    }

    #[test]
    fn a_broken_leaves_block_throws_shards_that_remember_the_site() {
        let mut ps = Particles::new();
        ps.break_block([3, 70, 4], 18 << 4, |_, _| 1);
        assert_eq!(ps.len(), 64);
        assert!(ps
            .list
            .iter()
            .all(|p| matches!(p.sprite, Sprite::Block { site: Some(_), .. })));
        ps.spawn(Kind::BlockCrack, [0.0; 3], [0.0; 3], 18 << 4);
        assert!(matches!(
            ps.list.last().unwrap().sprite,
            Sprite::Block { site: None, .. }
        ));
    }
}
