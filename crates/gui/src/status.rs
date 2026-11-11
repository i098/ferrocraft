//! `GuiIngame.renderPlayerStats`, `renderExpBar` and `renderHorseJumpBar` (`icons.png` sprites).

use crate::toolkit::{Gui, ICONS};

/// The local player's stats as the server sent them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub health: f32,
    pub max_health: f32,
    pub food: i32,
    pub saturation: f32,
    pub armor: i32,
    pub air: i32,
    pub max_air: i32,
    pub in_water: bool,
    pub xp_bar: f32,
    pub xp_level: i32,
    pub hardcore: bool,
    pub poisoned: bool,
    pub withered: bool,
    /// Hunger effect: the food row uses the green sprites.
    pub hunger: bool,
    pub absorption: f32,
    /// Index of the bouncing heart while regenerating (`updateCounter % ceil(max + 5)`), or -1.
    pub regen_tick: i32,
    /// Riding a horse: the jump charge 0..=1 replaces the XP bar.
    pub horse_jump: Option<f32>,
    /// Riding a living entity: `(health, max_health)` replaces the food row.
    pub mount_health: Option<(f32, f32)>,
    pub creative: bool,
    pub spectator: bool,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            health: 20.0,
            max_health: 20.0,
            food: 20,
            saturation: 5.0,
            armor: 0,
            air: 300,
            max_air: 300,
            in_water: false,
            xp_bar: 0.0,
            xp_level: 0,
            hardcore: false,
            poisoned: false,
            withered: false,
            hunger: false,
            absorption: 0.0,
            regen_tick: -1,
            horse_jump: None,
            mount_health: None,
            creative: false,
            spectator: false,
        }
    }
}

impl Status {
    /// `PlayerControllerMP.shouldDrawHUD`: survival or adventure.
    pub fn survival(&self) -> bool {
        !self.creative && !self.spectator
    }
}

/// The heart blink state `GuiIngame` keeps between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusState {
    /// `healthUpdateCounter`.
    pub health_update_counter: i32,
    /// `playerHealth`.
    pub player_health: i32,
    /// `lastPlayerHealth`.
    pub last_player_health: i32,
    /// `lastSystemTime` in ticks (`updateCounter`) instead of milliseconds.
    pub last_time: i32,
}

/// `java.util.Random`, seeded per frame so the low-health shake matches vanilla.
struct JavaRandom(i64);

impl JavaRandom {
    fn new(seed: i64) -> Self {
        JavaRandom((seed ^ 0x5DEE_CE66D) & ((1 << 48) - 1))
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5DEE_CE66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        if (bound as u32).is_power_of_two() {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                return val;
            }
        }
    }
}

/// `renderPlayerStats`: armor, hearts, food (or mount hearts) and air bubbles.
pub fn draw_stats(gui: &mut Gui, s: &Status, st: &mut StatusState, update_counter: i32) {
    let (w, h) = (gui.scale.width, gui.scale.height);
    let i = s.health.ceil() as i32;
    let flag = st.health_update_counter > update_counter
        && (st.health_update_counter - update_counter) / 3 % 2 == 1;
    track_health(st, i, update_counter);
    let j = st.last_player_health;
    let mut rand = JavaRandom::new(i64::from(update_counter) * 312871);
    let i1 = w / 2 - 91;
    let j1 = w / 2 + 91;
    let k1 = h - 39;
    let l1 = ((s.max_health + s.absorption) / 2.0 / 10.0).ceil() as i32;
    let i2 = (10 - (l1 - 2)).max(3);
    let j2 = k1 - (l1 - 1) * i2 - 10;
    gui.set_color(0xFFFF_FFFF);
    draw_armor(gui, s.armor, i1, j2);
    draw_hearts(gui, s, &mut rand, flag, (i, j), (i1, k1, i2));
    match s.mount_health {
        None => draw_food(gui, s, &mut rand, update_counter, j1, k1),
        Some((health, max)) => draw_mount(gui, health, max, j1, k1),
    }
    if s.in_water {
        draw_air(gui, s, j1, j2);
    }
}

fn track_health(st: &mut StatusState, i: i32, update_counter: i32) {
    // Vanilla also needs `hurtResistantTime > 0`; a health change from the server stands in for it.
    if i < st.player_health {
        st.last_time = update_counter;
        st.health_update_counter = update_counter + 20;
    } else if i > st.player_health && st.player_health > 0 {
        st.last_time = update_counter;
        st.health_update_counter = update_counter + 10;
    }
    if update_counter - st.last_time > 20 {
        st.last_player_health = i;
        st.last_time = update_counter;
    }
    st.player_health = i;
}

/// Full icon at `full_u` while pip `n` is under `val`, half icon at `half_u` when it is the odd last one.
#[allow(clippy::too_many_arguments)]
fn pip(gui: &mut Gui, x: i32, y: i32, n: i32, val: i32, full_u: i32, half_u: i32, v: i32) {
    if n * 2 + 1 < val {
        gui.blit(ICONS, x, y, full_u, v, 9, 9);
    } else if n * 2 + 1 == val {
        gui.blit(ICONS, x, y, half_u, v, 9, 9);
    }
}

fn draw_armor(gui: &mut Gui, armor: i32, x: i32, y: i32) {
    if armor <= 0 {
        return;
    }
    for i3 in 0..10 {
        let u = match (i3 * 2 + 1).cmp(&armor) {
            std::cmp::Ordering::Less => 34,
            std::cmp::Ordering::Equal => 25,
            std::cmp::Ordering::Greater => 16,
        };
        gui.blit(ICONS, x + i3 * 8, y, u, 9, 9, 9);
    }
}

/// Heart sprite column offset (poison, wither) and row (hardcore).
fn heart_uv(s: &Status) -> (i32, i32) {
    let u = if s.poisoned {
        36
    } else if s.withered {
        72
    } else {
        0
    };
    (16 + u, if s.hardcore { 45 } else { 0 })
}

/// `(i, j)` is current and last health; `(i1, k1, i2)` is left x, bottom y and row step.
fn draw_hearts(
    gui: &mut Gui,
    s: &Status,
    rand: &mut JavaRandom,
    flag: bool,
    (i, j): (i32, i32),
    (i1, k1, i2): (i32, i32, i32),
) {
    let f1 = s.absorption;
    let mut f2 = f1;
    let (j6, v) = heart_uv(s);
    let k3 = i32::from(flag);
    for i6 in (0..((s.max_health + f1) / 2.0).ceil() as i32).rev() {
        let l3 = ((i6 + 1) as f32 / 10.0).ceil() as i32 - 1;
        let i4 = i1 + i6 % 10 * 8;
        let mut j4 = k1 - l3 * i2;
        if i <= 4 {
            j4 += rand.next_int(2);
        }
        if i6 == s.regen_tick {
            j4 -= 2;
        }
        gui.blit(ICONS, i4, j4, 16 + k3 * 9, v, 9, 9);
        if flag {
            pip(gui, i4, j4, i6, j, j6 + 54, j6 + 63, v);
        }
        if f2 > 0.0 {
            let u = if f2 == f1 && f1 % 2.0 == 1.0 {
                153
            } else {
                144
            };
            gui.blit(ICONS, i4, j4, j6 + u, v, 9, 9);
            f2 -= 2.0;
        } else {
            pip(gui, i4, j4, i6, i, j6 + 36, j6 + 45, v);
        }
    }
}

fn draw_food(
    gui: &mut Gui,
    s: &Status,
    rand: &mut JavaRandom,
    update_counter: i32,
    j1: i32,
    k1: i32,
) {
    let k = s.food.clamp(0, 20);
    // `Potion.hunger`: the background tile is the 13th 9-px column and the food pips sit 36 px right.
    let (bg, shift) = if s.hunger { (16 + 13 * 9, 36) } else { (16, 0) };
    for k6 in 0..10 {
        let mut l6 = k1;
        if s.saturation <= 0.0 && update_counter % (k * 3 + 1) == 0 {
            l6 = k1 + (rand.next_int(3) - 1);
        }
        let k8 = j1 - k6 * 8 - 9;
        gui.blit(ICONS, k8, l6, bg, 27, 9, 9);
        pip(gui, k8, l6, k6, k, 52 + shift, 61 + shift, 27);
    }
}

fn draw_mount(gui: &mut Gui, health: f32, max: f32, j1: i32, k1: i32) {
    let i7 = health.ceil() as i32;
    let mut j8 = ((max + 0.5) as i32 / 2).min(30);
    let mut i9 = k1;
    let mut j9 = 0;
    while j8 > 0 {
        let k9 = j8.min(10);
        j8 -= k9;
        for l9 in 0..k9 {
            let k10 = j1 - l9 * 8 - 9;
            gui.blit(ICONS, k10, i9, 52, 9, 9, 9);
            pip(gui, k10, i9, l9, i7 - j9, 88, 97, 9);
        }
        i9 -= 10;
        j9 += 20;
    }
}

fn draw_air(gui: &mut Gui, s: &Status, j1: i32, j2: i32) {
    let max = f64::from(s.max_air.max(1));
    let l6 = f64::from(s.air);
    let k7 = ((l6 - 2.0) * 10.0 / max).ceil() as i32;
    let i8 = (l6 * 10.0 / max).ceil() as i32 - k7;
    for l8 in 0..k7 + i8 {
        let u = if l8 < k7 { 16 } else { 25 };
        gui.blit(ICONS, j1 - l8 * 8 - 9, j2, u, 18, 9, 9);
    }
}

/// `renderExpBar`: the green bar and the outlined level number.
pub fn draw_xp(gui: &mut Gui, s: &Status) {
    let (w, h) = (gui.scale.width, gui.scale.height);
    let x = w / 2 - 91;
    gui.set_color(0xFFFF_FFFF);
    let k = (s.xp_bar * 183.0) as i32;
    let y = h - 32 + 3;
    gui.blit(ICONS, x, y, 0, 64, 182, 5);
    if k > 0 {
        gui.blit(ICONS, x, y, 0, 69, k, 5);
    }
    if s.xp_level > 0 {
        let t = s.xp_level.to_string();
        let (l1, i1) = ((w - gui.width(&t)) / 2, h - 31 - 4);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            gui.text_plain(&t, (l1 + dx) as f32, (i1 + dy) as f32, 0);
        }
        gui.text_plain(&t, l1 as f32, i1 as f32, 8_453_920);
    }
}

/// `renderHorseJumpBar`.
pub fn draw_jump(gui: &mut Gui, power: f32) {
    let (w, h) = (gui.scale.width, gui.scale.height);
    let x = w / 2 - 91;
    gui.set_color(0xFFFF_FFFF);
    let j = (power * 183.0) as i32;
    let y = h - 32 + 3;
    gui.blit(ICONS, x, y, 0, 84, 182, 5);
    if j > 0 {
        gui.blit(ICONS, x, y, 0, 89, j, 5);
    }
}

#[cfg(test)]
mod tests {
    use super::JavaRandom;

    /// `new Random(42).nextInt(10)` is 0 and `new Random(0).nextInt(2)` is 1 in Java.
    #[test]
    fn java_random_matches() {
        assert_eq!(JavaRandom::new(42).next_int(10), 0);
        assert_eq!(JavaRandom::new(0).next_int(2), 1);
    }
}
