//! Client side of `MobSpawnerBaseLogic` (1.8.9) and `TileEntityMobSpawnerRenderer`: an activated
//! spawner puffs one smoke and one flame particle at a random point inside its block every tick,
//! and turns the mob it shows inside the cage.
//!
//! The mob is the entity type the server names, with the random choices of `onInitialSpawn` that
//! change how it looks (see [`Initial`]). Mobs with no ported model (see `entity::has_model`) show
//! no mob.

mod initial;

use super::Mat;
use crate::entity::{self, Class, EntityDraw, HorseLook, Look, Skins, TeamTag};
use crate::jrandom::JavaRandom;
use crate::mesh::MeshData;
pub use initial::Local;
use initial::{Initial, MAGMA_CUBE, RABBIT, SHEEP, SLIME, VILLAGER};

/// `requiredPlayerRange` of a fresh spawner.
const REQUIRED_PLAYER_RANGE: f64 = 16.0;
/// `MobSpawnerBaseLogic.minSpawnDelay` and the initial `spawnDelay`.
const MIN_SPAWN_DELAY: i32 = 200;
const START_DELAY: i32 = 20;
/// `TileEntityMobSpawnerRenderer`: the mob is drawn at this scale.
const MOB_SCALE: f32 = 0.4375;

/// The animation state of one spawner (fields by their MCP names).
#[derive(Clone, Debug, PartialEq)]
pub struct Spawner {
    mob: Option<u8>,
    /// The random choices of `onInitialSpawn`, rolled again with each data packet (`cachedEntity`).
    initial: Initial,
    spawn_delay: i32,
    min_spawn_delay: i32,
    mob_rotation: f64,
    prev_mob_rotation: f64,
}

impl Default for Spawner {
    fn default() -> Spawner {
        Spawner {
            mob: None,
            initial: Initial::default(),
            spawn_delay: START_DELAY,
            min_spawn_delay: MIN_SPAWN_DELAY,
            mob_rotation: 0.0,
            prev_mob_rotation: 0.0,
        }
    }
}

impl Spawner {
    /// `readFromNBT` of the data packet: the entity, `Delay` and, when sent, `MinSpawnDelay`. A
    /// new entity name drops `cachedEntity`, so the mob is created and `onInitialSpawn` rolled again.
    pub fn read(
        &mut self,
        entity: &str,
        delay: i16,
        min_delay: Option<i32>,
        local: Local,
        rng: &mut JavaRandom,
    ) {
        self.mob = mob_id(entity).filter(|&id| entity::has_model(id));
        if let Some(id) = self.mob {
            self.initial = Initial::roll(id, local, rng);
        }
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
    /// `viewer` is the eye of the camera entity, which a guardian's pupil follows.
    pub fn mob_mesh(
        &self,
        at: Mat,
        partial: f32,
        viewer: [f64; 3],
        skins: &Skins,
        light: [f32; 2],
    ) -> Option<MeshData> {
        let mob = self.mob?;
        let spin = self.prev_mob_rotation
            + (self.mob_rotation - self.prev_mob_rotation) * f64::from(partial);
        let size = world::entities::Kind::Mob(mob).size();
        let mut draw = still_mob(mob, size, partial, &self.initial);
        draw.look.viewer = viewer;
        let mut mesh = entity::build(&[draw], skins, |_| light);
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

fn hitbox(id: u8, size: (f32, f32), it: &Initial) -> (f32, f32) {
    match id {
        SLIME | MAGMA_CUBE => {
            let side = 0.51000005 * f32::from(it.slime_size);
            (side, side)
        }
        _ => size,
    }
}

fn variant(id: u8, it: &Initial) -> u8 {
    match id {
        SHEEP => it.fleece,
        VILLAGER => it.variant,
        _ => 0,
    }
}

/// An entity that never ticked, at the origin and yaw 0 (`renderEntityWithPosYaw(.., 0, 0, 0, 0, ..)`).
fn still_mob(id: u8, size: (f32, f32), partial: f32, it: &Initial) -> EntityDraw {
    // `EntitySlime.setSlimeSize` sizes the hitbox by 0.51000005 per size step.
    let size = hitbox(id, size, it);
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
        held: it.held,
        armor: it.armor,
        armor_dye: [None; 4],
        armor_glint: it.armor_glint,
        held_glint: it.held_glint,
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
        child: it.child,
        using: None,
        view: [0.0; 2],
        line: None,
        look: Look {
            partial,
            riding: it.riding,
            // Sheep fleece color, villager profession and rabbit type.
            variant: variant(id, it),
            collar: if id == RABBIT {
                it.variant
            } else {
                Look::default().collar
            },
            horse: HorseLook {
                kind: it.horse_kind,
                variant: it.horse_variant,
                ..HorseLook::default()
            },
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

    fn read(s: &mut Spawner, name: &str) {
        s.read(name, 0, None, Local::default(), &mut JavaRandom::new(1));
    }

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
        late.read("Pig", 0, None, Local::default(), &mut JavaRandom::new(1));
        late.tick();
        assert!((late.mob_rotation - 5.0).abs() < 1e-9);
        late.delay_to_min();
        assert_eq!(late.spawn_delay, 200);
    }

    #[test]
    fn only_mobs_with_a_model_show() {
        let mut s = Spawner::default();
        read(&mut s, "Zombie");
        assert_eq!(s.mob, Some(54));
        read(&mut s, "Wolf");
        assert_eq!(s.mob, Some(95));
        read(&mut s, "Rabbit");
        assert_eq!(s.mob, Some(101));
        read(&mut s, "EntityHorse");
        assert_eq!(s.mob, Some(100));
        read(&mut s, "Squid");
        assert_eq!(s.mob, Some(94));
        read(&mut s, "MushroomCow");
        assert_eq!(s.mob, Some(96));
        read(&mut s, "Endermite");
        assert_eq!(s.mob, Some(67));
        read(&mut s, "SnowMan");
        assert_eq!(s.mob, Some(97));
        read(&mut s, "Witch");
        assert_eq!(s.mob, Some(66));
        read(&mut s, "Giant");
        assert_eq!(s.mob, Some(53));
        read(&mut s, "XPOrb");
        assert_eq!(s.mob, None);
    }

    #[test]
    fn re_rolled_horse_spawners_draw_different_species_and_coats() {
        let mut rng = JavaRandom::new(4);
        let mut looks = std::collections::HashSet::new();
        for _ in 0..40 {
            let mut s = Spawner::default();
            s.read("EntityHorse", 0, None, Local::default(), &mut rng);
            let d = still_mob(100, (1.4, 1.6), 0.0, &s.initial);
            looks.insert((d.look.horse.kind, d.look.horse.variant));
        }
        assert!(looks.iter().any(|l| l.0 == 1), "a donkey");
        assert!(looks.iter().filter(|l| l.0 == 0).count() > 5, "coats vary");
    }
}
