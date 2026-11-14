//! Client tick of `MobSpawnerBaseLogic.updateSpawner` (1.8.9): an activated spawner puffs one smoke
//! and one flame particle at a random point inside its block every tick.
//!
//! Not ported: the spinning display mob (`mobRotation`, `spawnDelay`) of the spawner box renderer.

use crate::jrandom::JavaRandom;

/// `requiredPlayerRange` of a fresh spawner.
const REQUIRED_PLAYER_RANGE: f64 = 16.0;

/// Particle positions for one tick of every spawner in `spawners`. `players` are the non-spectator
/// players; a spawner is `isActivated` while one is closer than 16 blocks to its centre.
pub fn spawner_particles(
    spawners: impl Iterator<Item = [i32; 3]>,
    players: &[[f64; 3]],
    rng: &mut JavaRandom,
) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for pos in spawners {
        let centre = pos.map(|c| f64::from(c) + 0.5);
        let near = players.iter().any(|p| {
            (0..3).map(|i| (p[i] - centre[i]).powi(2)).sum::<f64>()
                < REQUIRED_PLAYER_RANGE * REQUIRED_PLAYER_RANGE
        });
        if near {
            out.push(pos.map(|c| f64::from(c) + f64::from(rng.next_float())));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_spawner_with_a_player_within_16_blocks_emits() {
        let mut rng = JavaRandom::new(1);
        let spawners = [[0, 64, 0], [100, 64, 0]];
        let got = spawner_particles(spawners.into_iter(), &[[10.0, 64.0, 0.0]], &mut rng);
        assert_eq!(got.len(), 1);
        assert!((0.0..1.0).contains(&got[0][0]) && (64.0..65.0).contains(&got[0][1]));
        let far = spawner_particles([[0, 64, 0]].into_iter(), &[[16.5, 64.5, 0.5]], &mut rng);
        assert!(far.is_empty());
    }
}
