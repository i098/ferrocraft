//! Client side of `MobSpawnerBaseLogic` (1.8.9) and `TileEntityMobSpawnerRenderer`: an activated
//! spawner puffs one smoke and one flame particle at a random point inside its block every tick,
//! and turns the mob it shows inside the cage.
//!
//! The mob is the entity type the server names, drawn without `onInitialSpawn`'s random choices
//! (baby zombies, sheep colors, slime sizes). Mobs with no ported model (see `entity::has_model`)
//! show no mob.

use super::Mat;
use crate::entity::{self, Class, EntityDraw, Look, Skins, TeamTag};
use crate::jrandom::JavaRandom;
use crate::mesh::MeshData;

/// `requiredPlayerRange` of a fresh spawner.
const REQUIRED_PLAYER_RANGE: f64 = 16.0;
/// `MobSpawnerBaseLogic.minSpawnDelay` and the initial `spawnDelay`.
const MIN_SPAWN_DELAY: i32 = 200;
const START_DELAY: i32 = 20;
/// `TileEntityMobSpawnerRenderer`: the mob is drawn at this scale.
const MOB_SCALE: f32 = 0.4375;
const BOW: i16 = 261;

/// The animation state of one spawner (fields by their MCP names).
#[derive(Clone, Debug, PartialEq)]
pub struct Spawner {
    mob: Option<u8>,
    spawn_delay: i32,
    min_spawn_delay: i32,
    mob_rotation: f64,
    prev_mob_rotation: f64,
}

impl Default for Spawner {
    fn default() -> Spawner {
        Spawner {
            mob: None,
            spawn_delay: START_DELAY,
            min_spawn_delay: MIN_SPAWN_DELAY,
            mob_rotation: 0.0,
            prev_mob_rotation: 0.0,
        }
    }
}

impl Spawner {
    /// `readFromNBT` of the data packet: the entity, `Delay` and, when sent, `MinSpawnDelay`.
    pub fn read(&mut self, entity: &str, delay: i16, min_delay: Option<i32>) {
        self.mob = mob_id(entity).filter(|&id| entity::has_model(id));
        self.spawn_delay = i32::from(delay);
        if let Some(min) = min_delay {
            self.min_spawn_delay = min;
        }
    }

    /// `setDelayToMin` of `S24 BlockAction` event 1.
    pub fn delay_to_min(&mut self) {
        self.spawn_delay = self.min_spawn_delay;
    }

    /// The client branch of `updateSpawner` for an activated spawner.
    pub fn tick(&mut self) {
        if self.spawn_delay > 0 {
            self.spawn_delay -= 1;
        }
        self.prev_mob_rotation = self.mob_rotation;
        let step = 1000.0_f32 / (self.spawn_delay as f32 + 200.0);
        self.mob_rotation = (self.mob_rotation + f64::from(step)) % 360.0;
    }

    /// The mesh of the mob, in the world, for the block whose corner is `at`; `None` without one.
    pub fn mob_mesh(
        &self,
        at: Mat,
        partial: f32,
        skins: &Skins,
        light: [f32; 2],
    ) -> Option<MeshData> {
        let mob = self.mob?;
        let spin = self.prev_mob_rotation
            + (self.mob_rotation - self.prev_mob_rotation) * f64::from(partial);
        let size = world::entities::Kind::Mob(mob).size();
        let draw = still_mob(mob, size, partial);
        let mut mesh = entity::build(&[draw], skins, 0, |_| light);
        let m = at
            .translate([0.5, 0.0, 0.5])
            .translate([0.0, 0.4, 0.0])
            .rotate_y(spin as f32 * 10.0)
            .rotate_zyx([-30.0_f32.to_radians(), 0.0, 0.0])
            .translate([0.0, -0.4, 0.0])
            .scale([MOB_SCALE; 3]);
        for v in &mut mesh.vertices {
            v.pos = m.apply(v.pos);
        }
        Some(mesh)
    }
}

/// An entity that never ticked, at the origin and yaw 0 (`renderEntityWithPosYaw(.., 0, 0, 0, 0, ..)`).
fn still_mob(id: u8, size: (f32, f32), partial: f32) -> EntityDraw {
    EntityDraw {
        class: Class::Mob(id),
        pos: [0.0; 3],
        yaw: 0.0,
        head_yaw: 0.0,
        pitch: 0.0,
        limb_swing: 0.0,
        limb_swing_amount: 0.0,
        swing: 0.0,
        // `ticksExisted` stays 0, so `ageInTicks` is the partial tick.
        age: partial,
        sneaking: false,
        hurt: false,
        death_time: 0.0,
        invisible: false,
        size,
        // `EntitySkeleton.onInitialSpawn` in the overworld.
        held: if id == 51 { BOW } else { 0 },
        armor: [0; 4],
        armor_dye: [None; 4],
        armor_glint: [false; 4],
        held_glint: false,
        held_damage: 0,
        held_dye: None,
        stack: None,
        hover_start: 0.0,
        hang: Default::default(),
        name: None,
        skin: None,
        burning: false,
        eye_height: 0.0,
        always_name: false,
        pointed: false,
        team_tag: TeamTag::default(),
        ridden: false,
        child: false,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            partial,
            ..Look::default()
        },
    }
}

/// `EntityList` name to network mob type id of 1.8.9.
fn mob_id(name: &str) -> Option<u8> {
    const NAMES: [(&str, u8); 32] = [
        ("Creeper", 50),
        ("Skeleton", 51),
        ("Spider", 52),
        ("Giant", 53),
        ("Zombie", 54),
        ("Slime", 55),
        ("Ghast", 56),
        ("PigZombie", 57),
        ("Enderman", 58),
        ("CaveSpider", 59),
        ("Silverfish", 60),
        ("Blaze", 61),
        ("LavaSlime", 62),
        ("EnderDragon", 63),
        ("WitherBoss", 64),
        ("Bat", 65),
        ("Witch", 66),
        ("Endermite", 67),
        ("Guardian", 68),
        ("Pig", 90),
        ("Sheep", 91),
        ("Cow", 92),
        ("Chicken", 93),
        ("Squid", 94),
        ("Wolf", 95),
        ("MushroomCow", 96),
        ("SnowMan", 97),
        ("Ozelot", 98),
        ("VillagerGolem", 99),
        ("EntityHorse", 100),
        ("Rabbit", 101),
        ("Villager", 120),
    ];
    NAMES.iter().find(|n| n.0 == name).map(|n| n.1)
}

/// Whether a spawner at `pos` is `isActivated`: a player is closer than 16 blocks to its centre.
/// `players` are the non-spectator players.
pub fn activated(pos: [i32; 3], players: &[[f64; 3]]) -> bool {
    let centre = pos.map(|c| f64::from(c) + 0.5);
    players.iter().any(|p| {
        (0..3).map(|i| (p[i] - centre[i]).powi(2)).sum::<f64>()
            < REQUIRED_PLAYER_RANGE * REQUIRED_PLAYER_RANGE
    })
}

/// Where the smoke and flame of one tick appear: a random point inside the block.
pub fn particle(pos: [i32; 3], rng: &mut JavaRandom) -> [f64; 3] {
    pos.map(|c| f64::from(c) + f64::from(rng.next_float()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_spawner_with_a_player_within_16_blocks_is_activated() {
        assert!(activated([0, 64, 0], &[[10.0, 64.0, 0.0]]));
        assert!(!activated([0, 64, 0], &[[16.5, 64.5, 0.5]]));
        assert!(!activated([100, 64, 0], &[[10.0, 64.0, 0.0]]));
        let mut rng = JavaRandom::new(1);
        let at = particle([0, 64, 0], &mut rng);
        assert!((0.0..1.0).contains(&at[0]) && (64.0..65.0).contains(&at[1]));
    }

    #[test]
    fn the_mob_turns_faster_as_the_spawn_delay_runs_out() {
        let mut s = Spawner::default();
        s.tick();
        assert_eq!(s.spawn_delay, 19);
        assert!((s.mob_rotation - f64::from(1000.0_f32 / 219.0)).abs() < 1e-9);
        let mut late = Spawner::default();
        late.read("Pig", 0, None);
        late.tick();
        assert!((late.mob_rotation - 5.0).abs() < 1e-9);
        late.delay_to_min();
        assert_eq!(late.spawn_delay, 200);
    }

    #[test]
    fn only_mobs_with_a_model_show() {
        let mut s = Spawner::default();
        s.read("Zombie", 0, None);
        assert_eq!(s.mob, Some(54));
        s.read("Wolf", 0, None);
        assert_eq!(s.mob, Some(95));
        s.read("Rabbit", 0, None);
        assert_eq!(s.mob, Some(101));
        s.read("Squid", 0, None);
        assert_eq!(s.mob, Some(94));
        s.read("Giant", 0, None);
        assert_eq!(s.mob, None);
        s.read("XPOrb", 0, None);
        assert_eq!(s.mob, None);
    }
}
