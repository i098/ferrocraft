//! Potion effects of the local player as the 1.8.9 client tracks them: the `EntityEffect` (0x1D)
//! and `RemoveEntityEffect` (0x1E) packets, `PotionEffect.combine` and the per-tick countdown,
//! plus the `generic.movementSpeed` attribute that `getFovModifier` reads.

/// `Potion.id` values the client reads.
pub mod id {
    pub const HASTE: i8 = 3;
    pub const MINING_FATIGUE: i8 = 4;
    pub const JUMP_BOOST: i8 = 8;
    pub const NAUSEA: i8 = 9;
    pub const REGENERATION: i8 = 10;
    pub const INVISIBILITY: i8 = 14;
    pub const BLINDNESS: i8 = 15;
    pub const NIGHT_VISION: i8 = 16;
    pub const HUNGER: i8 = 17;
    pub const POISON: i8 = 19;
    pub const WITHER: i8 = 20;
}

/// One `PotionEffect` in `activePotionsMap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: i8,
    pub amplifier: i8,
    /// Ticks left; the client counts down to 0 and keeps the entry until the server removes it.
    pub duration: i32,
    /// `isPotionDurationMax`: the packet carried 32767 ticks.
    pub permanent: bool,
    pub ambient: bool,
    pub particles: bool,
}

/// `S1DPacketEntityEffect` duration that means "no end".
pub const PERMANENT_TICKS: i32 = 32767;

/// `HashMap` default table size.
const INITIAL_CAPACITY: usize = 16;

/// `EntityLivingBase.activePotionsMap`, kept in the order `HashMap<Integer, ...>.values()` iterates:
/// bucket `id & (capacity - 1)` ascending, insertion order inside a bucket. The table doubles when
/// the size passes 3/4 of the capacity and never shrinks (a resize keeps the order inside a bucket).
#[derive(Clone, Debug, PartialEq)]
pub struct Effects(Vec<Effect>, usize);

impl Default for Effects {
    fn default() -> Self {
        Self(Vec::new(), INITIAL_CAPACITY)
    }
}

/// `HashMap.hash` of an `Integer` key is `h ^ (h >>> 16)`; the bucket is that masked by `cap - 1`.
fn bucket(id: i8, cap: usize) -> usize {
    let h = i32::from(id) as u32;
    ((h ^ (h >> 16)) as usize) & (cap - 1)
}

impl Effects {
    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `isPotionActive`.
    pub fn has(&self, id: i8) -> bool {
        self.0.iter().any(|e| e.id == id)
    }

    /// `getActivePotionEffect`.
    pub fn get(&self, id: i8) -> Option<&Effect> {
        self.0.iter().find(|e| e.id == id)
    }

    /// `handleEntityEffect` then `addPotionEffect`: a new effect is stored, a known id is combined.
    pub fn add(&mut self, new: Effect) {
        let Some(e) = self.0.iter_mut().find(|e| e.id == new.id) else {
            let b = bucket(new.id, self.1);
            let at = self.0.partition_point(|e| bucket(e.id, self.1) <= b);
            self.0.insert(at, new);
            if self.0.len() > self.1 * 3 / 4 {
                self.1 *= 2;
                let cap = self.1;
                self.0.sort_by_key(|e| bucket(e.id, cap));
            }
            return;
        };
        // `PotionEffect.combine`
        if new.amplifier > e.amplifier {
            e.amplifier = new.amplifier;
            e.duration = new.duration;
        } else if new.amplifier == e.amplifier && e.duration < new.duration {
            e.duration = new.duration;
        } else if !new.ambient && e.ambient {
            e.ambient = false;
        }
        e.particles = new.particles;
    }

    /// `LivingEntity.getArmSwingDuration` (MCP `getArmSwingAnimationEnd`): Haste shortens the swing, Mining Fatigue lengthens it.
    pub fn arm_swing_duration(&self) -> i32 {
        if let Some(e) = self.get(id::HASTE) {
            6 - (1 + i32::from(e.amplifier))
        } else if let Some(e) = self.get(id::MINING_FATIGUE) {
            6 + (1 + i32::from(e.amplifier)) * 2
        } else {
            6
        }
    }

    /// `handleRemoveEntityEffect`.
    pub fn remove(&mut self, id: i8) {
        self.0.retain(|e| e.id != id);
    }

    /// `PotionEffect.onUpdate` on the client: the countdown stops at 0.
    pub fn tick(&mut self) {
        for e in &mut self.0 {
            e.duration = (e.duration - 1).max(0);
        }
    }
}

/// `SPRINTING_SPEED_MODIFIER_ID`: `662A6B8D-DA3E-4C1C-8813-96EA6097278D`.
pub const SPRINT_MODIFIER: u128 = 0x662A_6B8D_DA3E_4C1C_8813_96EA_6097_278D;
/// `0.3F` widened to double.
const SPRINT_AMOUNT: f64 = 0.300_000_011_920_928_96;

/// `AttributeModifier` of an `S20PacketEntityProperties` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifier {
    pub uuid: u128,
    pub amount: f64,
    pub operation: i8,
}

/// The local player's `generic.movementSpeed` instance.
#[derive(Clone, Debug, PartialEq)]
pub struct Speed {
    pub base: f64,
    pub modifiers: Vec<Modifier>,
}

impl Default for Speed {
    /// `EntityPlayer.applyEntityAttributes`: `0.10000000149011612`.
    fn default() -> Self {
        Speed {
            base: 0.100_000_001_490_116_12,
            modifiers: Vec::new(),
        }
    }
}

impl Speed {
    /// `EntityLivingBase.setSprinting`: drops the sprint modifier, then applies it again when sprinting.
    pub fn set_sprint(&mut self, sprinting: bool) {
        self.modifiers.retain(|m| m.uuid != SPRINT_MODIFIER);
        if sprinting {
            self.modifiers.push(Modifier {
                uuid: SPRINT_MODIFIER,
                amount: SPRINT_AMOUNT,
                operation: 2,
            });
        }
    }

    /// `ModifiableAttributeInstance.computeValue`, clamped by `RangedAttribute(0, 0, Double.MAX_VALUE)`.
    /// The list is what the server sent (`handleEntityAttributes` clears and refills it) plus local sprint changes.
    pub fn value(&self) -> f64 {
        let mods = || self.modifiers.iter();
        let mut x = self.base;
        for m in mods().filter(|m| m.operation == 0) {
            x += m.amount;
        }
        let mut y = x;
        for m in mods().filter(|m| m.operation == 1) {
            y += x * m.amount;
        }
        for m in mods().filter(|m| m.operation == 2) {
            y *= 1.0 + m.amount;
        }
        y.max(0.0)
    }
}

/// English `potion.*` names indexed by `Potion.id - 1`.
const NAMES: [&str; 23] = [
    "Speed",
    "Slowness",
    "Haste",
    "Mining Fatigue",
    "Strength",
    "Instant Health",
    "Instant Damage",
    "Jump Boost",
    "Nausea",
    "Regeneration",
    "Resistance",
    "Fire Resistance",
    "Water Breathing",
    "Invisibility",
    "Blindness",
    "Night Vision",
    "Hunger",
    "Weakness",
    "Poison",
    "Wither",
    "Health Boost",
    "Absorption",
    "Saturation",
];

/// English `potion.*` name of an effect id; `None` for ids the client does not know.
pub fn name(id: i8) -> Option<&'static str> {
    NAMES
        .get(usize::try_from(id).ok()?.checked_sub(1)?)
        .copied()
}

/// `setIconIndex(column, row)` of ids 1 to 22, indexed by `Potion.id - 1`; `None` for the instant effects.
const ICONS: [Option<(i32, i32)>; 22] = [
    Some((0, 0)),
    Some((1, 0)),
    Some((2, 0)),
    Some((3, 0)),
    Some((4, 0)),
    None,
    None,
    Some((2, 1)),
    Some((3, 1)),
    Some((7, 0)),
    Some((6, 1)),
    Some((7, 1)),
    Some((0, 2)),
    Some((0, 1)),
    Some((5, 1)),
    Some((4, 1)),
    Some((1, 1)),
    Some((5, 0)),
    Some((6, 0)),
    Some((1, 2)),
    Some((2, 2)),
    Some((2, 2)),
];

/// `Potion.getStatusIconIndex` (`column + row * 8`); `None` for the instant effects, which have no icon.
pub fn icon(id: i8) -> Option<i32> {
    let (col, row) = (*ICONS.get(usize::try_from(id).ok()?.checked_sub(1)?)?)?;
    Some(col + row * 8)
}

/// `EntityRenderer.getNightVisionBrightness`: full above 200 ticks, then a fading flicker.
pub fn night_vision_brightness(duration: i32, partial: f32) -> f32 {
    if duration > 200 {
        1.0
    } else {
        0.7 + super::sin((duration as f32 - partial) * std::f32::consts::PI * 0.2) * 0.3
    }
}

/// `StringUtils.ticksToElapsedTime`: `m:ss`, or `**:**` for a maxed duration.
pub fn duration_string(e: &Effect) -> String {
    if e.permanent {
        return "**:**".into();
    }
    let secs = e.duration / 20;
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(id: i8, amplifier: i8, duration: i32) -> Effect {
        Effect {
            id,
            amplifier,
            duration,
            permanent: false,
            ambient: false,
            particles: true,
        }
    }

    #[test]
    fn combine_keeps_the_stronger_or_longer_effect() {
        let mut e = Effects::default();
        e.add(fx(1, 1, 100));
        e.add(fx(1, 0, 900));
        assert_eq!(e.iter().next().unwrap().duration, 100);
        e.add(fx(1, 1, 300));
        assert_eq!(e.iter().next().unwrap().duration, 300);
        e.add(fx(1, 2, 50));
        let only = e.iter().next().unwrap();
        assert_eq!((only.amplifier, only.duration), (2, 50));
    }

    fn order(e: &Effects) -> Vec<i8> {
        e.iter().map(|x| x.id).collect()
    }

    /// Vanilla `/effect` 1, 9, 16, 5 lists Night Vision, Speed, Strength, Nausea: 16, 1, 5, 9.
    #[test]
    fn iteration_follows_hash_map_bucket_order() {
        let mut e = Effects::default();
        for id in [1, 9, 16, 5] {
            e.add(fx(id, 0, 100));
        }
        assert_eq!(order(&e), [16, 1, 5, 9]);
    }

    /// 3, 19, 35, 51, 67, 83 share bucket 3: they keep insertion order, also after a removal.
    #[test]
    fn one_bucket_keeps_insertion_order() {
        let mut e = Effects::default();
        for id in [51, 3, 83, 19, 67, 35] {
            e.add(fx(id, 0, 100));
        }
        assert_eq!(order(&e), [51, 3, 83, 19, 67, 35]);
        e.remove(83);
        e.add(fx(83, 0, 100));
        assert_eq!(order(&e), [51, 3, 19, 67, 35, 83]);
    }

    /// The 13th entry grows the table to 32 buckets: 16 moves from bucket 0 behind 12.
    #[test]
    fn table_doubles_past_three_quarters() {
        let mut e = Effects::default();
        e.add(fx(16, 0, 100));
        for id in 1..=11 {
            e.add(fx(id, 0, 100));
        }
        assert_eq!(order(&e)[0], 16);
        e.add(fx(12, 0, 100));
        assert_eq!(order(&e), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 16]);
        e.clear();
        for id in [17, 1] {
            e.add(fx(id, 0, 100));
        }
        assert_eq!(order(&e), [17, 1]);
    }

    #[test]
    fn countdown_stops_at_zero_and_the_entry_stays() {
        let mut e = Effects::default();
        e.add(fx(19, 0, 1));
        e.tick();
        e.tick();
        assert!(e.has(19));
        assert_eq!(e.iter().next().unwrap().duration, 0);
        e.remove(19);
        assert!(e.is_empty());
    }

    #[test]
    fn speed_potion_and_sprint_scale_the_movement_attribute() {
        let mut s = Speed::default();
        s.modifiers.push(Modifier {
            uuid: 1,
            amount: 0.2,
            operation: 2,
        });
        let walk = 0.100_000_001_490_116_12 * 1.2;
        assert!((s.value() - walk).abs() < 1e-12);
        s.set_sprint(true);
        assert!((s.value() - walk * 1.300_000_011_920_929).abs() < 1e-12);
        // A repeated start does not count the boost twice.
        s.set_sprint(true);
        assert!((s.value() - walk * 1.300_000_011_920_929).abs() < 1e-12);
        s.set_sprint(false);
        assert!((s.value() - walk).abs() < 1e-12);
    }

    #[test]
    fn night_vision_flickers_only_in_the_last_ten_seconds() {
        assert_eq!(night_vision_brightness(201, 0.5), 1.0);
        let b = night_vision_brightness(100, 0.0);
        assert!((0.4..=1.0).contains(&b) && b != 1.0);
    }

    #[test]
    fn duration_reads_minutes_and_seconds() {
        assert_eq!(duration_string(&fx(1, 0, 20 * 65)), "1:05");
        let mut p = fx(1, 0, 5);
        p.permanent = true;
        assert_eq!(duration_string(&p), "**:**");
    }
}
