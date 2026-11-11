//! Potion effects of the local player as the 1.8.9 client tracks them: the `EntityEffect` (0x1D)
//! and `RemoveEntityEffect` (0x1E) packets, `PotionEffect.combine` and the per-tick countdown,
//! plus the `generic.movementSpeed` attribute that `getFovModifier` reads.

/// `Potion.id` values the client reads.
pub mod id {
    pub const NAUSEA: i8 = 9;
    pub const REGENERATION: i8 = 10;
    pub const INVISIBILITY: i8 = 14;
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

/// `EntityLivingBase.activePotionsMap` in id order, as `HashMap<Integer, ...>.values()` iterates it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects(Vec<Effect>);

impl Effects {
    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// `isPotionActive`.
    pub fn has(&self, id: i8) -> bool {
        self.0.iter().any(|e| e.id == id)
    }

    /// `handleEntityEffect` then `addPotionEffect`: a new effect is stored, a known id is combined.
    pub fn add(&mut self, new: Effect) {
        let Some(e) = self.0.iter_mut().find(|e| e.id == new.id) else {
            let at = self.0.partition_point(|e| e.id < new.id);
            self.0.insert(at, new);
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
    /// `ModifiableAttributeInstance.computeValue`, clamped by `RangedAttribute(0, 0, Double.MAX_VALUE)`.
    /// `sprinting` adds the sprint boost that `EntityLivingBase.setSprinting` keeps on the instance.
    pub fn value(&self, sprinting: bool) -> f64 {
        let sprint = Modifier {
            uuid: SPRINT_MODIFIER,
            amount: SPRINT_AMOUNT,
            operation: 2,
        };
        let mods = || {
            self.modifiers
                .iter()
                .filter(|m| m.uuid != SPRINT_MODIFIER)
                .chain(sprinting.then_some(&sprint))
        };
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
    NAMES.get(usize::try_from(id).ok()?.checked_sub(1)?).copied()
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
        assert!((s.value(false) - walk).abs() < 1e-12);
        assert!((s.value(true) - walk * 1.300_000_011_920_929).abs() < 1e-12);
        // A server-sent sprint boost is not counted twice.
        s.modifiers.push(Modifier {
            uuid: SPRINT_MODIFIER,
            amount: SPRINT_AMOUNT,
            operation: 2,
        });
        assert!((s.value(true) - walk * 1.300_000_011_920_929).abs() < 1e-12);
    }

    #[test]
    fn duration_reads_minutes_and_seconds() {
        assert_eq!(duration_string(&fx(1, 0, 20 * 65)), "1:05");
        let mut p = fx(1, 0, 5);
        p.permanent = true;
        assert_eq!(duration_string(&p), "**:**");
    }
}
