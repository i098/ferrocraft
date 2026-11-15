//! Boats and minecarts as the renderers read them: the metadata of `EntityBoat` and `EntityMinecart`
//! (indexes 17 to 22), the block a cart shows, and the rail snap of `MinecartRenderer.render`
//! (`EntityMinecart.getPosOffset` and `getPos`).

use crate::World;

/// Metadata a boat or minecart sends: hit timer (17), hit direction (18), damage taken (19), the
/// display tile as `id | meta << 12` (20), its offset (21) and the custom tile flag (22), plus the
/// minecart type of the spawn packet and the furnace flag (16).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleMeta {
    pub hit_timer: Option<i32>,
    pub hit_direction: Option<i32>,
    pub damage: Option<f32>,
    pub tile: Option<i32>,
    pub tile_offset: Option<i32>,
    pub custom_tile: Option<u8>,
    pub powered: Option<u8>,
    pub cart: Option<u8>,
}

impl VehicleMeta {
    pub(crate) fn merge(&mut self, new: VehicleMeta) {
        self.hit_timer = new.hit_timer.or(self.hit_timer);
        self.hit_direction = new.hit_direction.or(self.hit_direction);
        self.damage = new.damage.or(self.damage);
        self.tile = new.tile.or(self.tile);
        self.tile_offset = new.tile_offset.or(self.tile_offset);
        self.custom_tile = new.custom_tile.or(self.custom_tile);
        self.powered = new.powered.or(self.powered);
        self.cart = new.cart.or(self.cart);
    }

    /// `MinecartEntity.getDisplayBlock` and `getDisplayBlockOffset`: the block state as
    /// `id << 4 | meta` (0 for none) and the offset in sixteenths.
    pub fn display_block(&self) -> (u16, i32) {
        if self.custom_tile == Some(1) {
            let tile = self.tile.unwrap_or(0);
            let state = (tile & 4095) << 4 | (tile >> 12 & 15);
            return (state as u16, self.tile_offset.unwrap_or(0));
        }
        match self.cart {
            // `getDefaultDisplayBlock`: chest facing north, furnace (lit when powered), TNT, spawner,
            // hopper and command block; the offset is 6, 8 for the chest and 1 for the hopper.
            Some(1) => (54 << 4 | 2, 8),
            Some(2) => (
                if self.powered.unwrap_or(0) & 1 != 0 {
                    62
                } else {
                    61
                } << 4
                    | 2,
                6,
            ),
            Some(3) => (46 << 4, 6),
            Some(4) => (52 << 4, 6),
            Some(5) => (154 << 4, 1),
            Some(6) => (137 << 4, 6),
            _ => (0, 6),
        }
    }
}

/// The hit wobble of `BoatRenderer.render` and `MinecartRenderer.render`, in degrees about x:
/// `sin(t) * t * damage / 10 * direction` while `t`, the hit timer less the partial tick, is above 0,
/// with the damage taken less the partial tick floored at 0.
pub fn hit_wobble(timer: i32, damage: f32, direction: i32, partial: f32) -> f32 {
    let t = timer as f32 - partial;
    if t > 0.0 {
        crate::sin(t) * t * (damage - partial).max(0.0) / 10.0 * direction as f32
    } else {
        0.0
    }
}

/// The minecart type of a `SpawnObject`: object 10 carries it in `data`; the old ids 11 and 12 are
/// the chest and furnace carts.
pub fn cart_type(kind: i8, data: i32) -> Option<u8> {
    match kind {
        10 => Some(data as u8),
        11 => Some(1),
        12 => Some(2),
        _ => None,
    }
}

/// `EntityMinecart.matrix`, indexed by rail shape: the two block offsets of the rail's ends.
const ENDS: [[[i32; 3]; 2]; 10] = [
    [[0, 0, -1], [0, 0, 1]],
    [[-1, 0, 0], [1, 0, 0]],
    [[-1, -1, 0], [1, 0, 0]],
    [[-1, 0, 0], [1, -1, 0]],
    [[0, 0, -1], [0, -1, 1]],
    [[0, -1, -1], [0, 0, 1]],
    [[0, 0, 1], [1, 0, 0]],
    [[0, 0, 1], [-1, 0, 0]],
    [[0, 0, -1], [-1, 0, 0]],
    [[0, 0, -1], [1, 0, 0]],
];

/// `BlockRailBase.isRailBlock`: rail, powered rail, detector rail, activator rail.
fn rail_shape(state: u16) -> Option<usize> {
    let meta = usize::from(state & 15);
    match state >> 4 {
        66 => Some(if meta < 10 { meta } else { 0 }),
        27 | 28 | 157 => Some(meta & 7),
        _ => None,
    }
}

/// The rail under `(x, y, z)`: its block y (one lower when the rail is one block below) and shape.
fn rail_at(world: &impl World, x: f64, y: f64, z: f64) -> Option<([i32; 3], usize)> {
    let (i, mut j, k) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    if rail_shape(world.block(i, j - 1, k)).is_some() {
        j -= 1;
    }
    rail_shape(world.block(i, j, k)).map(|shape| ([i, j, k], shape))
}

/// `MinecartEntity.snapPositionToRail`: the point of the rail's centre line nearest `(x, y, z)`.
pub fn snap_to_rail(world: &impl World, x: f64, y: f64, z: f64) -> Option<[f64; 3]> {
    let ([i, j, k], shape) = rail_at(world, x, y, z)?;
    let [a, b] = ENDS[shape];
    let end = |e: [i32; 3]| {
        [
            f64::from(i) + 0.5 + f64::from(e[0]) * 0.5,
            f64::from(j) + 0.0625 + f64::from(e[1]) * 0.5,
            f64::from(k) + 0.5 + f64::from(e[2]) * 0.5,
        ]
    };
    let (p, q) = (end(a), end(b));
    let (dx, dy, dz) = (q[0] - p[0], (q[1] - p[1]) * 2.0, q[2] - p[2]);
    let along = if dx == 0.0 {
        z - f64::from(k)
    } else if dz == 0.0 {
        x - f64::from(i)
    } else {
        ((x - p[0]) * dx + (z - p[2]) * dz) * 2.0
    };
    let mut y = p[1] + dy * along;
    if dy < 0.0 {
        y += 1.0;
    }
    if dy > 0.0 {
        y += 0.5;
    }
    Some([p[0] + dx * along, y, p[2] + dz * along])
}

/// `MinecartEntity.snapPositionToRailWithOffset`: the snap of the point `offset` blocks along the rail.
pub fn snap_with_offset(
    world: &impl World,
    x: f64,
    y: f64,
    z: f64,
    offset: f64,
) -> Option<[f64; 3]> {
    let ([i, j, k], shape) = rail_at(world, x, y, z)?;
    let [a, b] = ENDS[shape];
    let ascending = (2..=5).contains(&shape);
    let mut y = f64::from(j) + if ascending { 1.0 } else { 0.0 };
    let (dx, dz) = (f64::from(b[0] - a[0]), f64::from(b[2] - a[2]));
    let len = (dx * dx + dz * dz).sqrt();
    let x = x + dx / len * offset;
    let z = z + dz / len * offset;
    let at =
        |e: [i32; 3]| (x.floor() as i32 - i, z.floor() as i32 - k) == (e[0], e[2]) && e[1] != 0;
    if at(a) {
        y += f64::from(a[1]);
    } else if at(b) {
        y += f64::from(b[1]);
    }
    snap_to_rail(world, x, y, z)
}

/// Where `MinecartRenderer.render` draws a cart: position, yaw and pitch in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CartPose {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

/// `MinecartRenderer.render`: a cart on a rail sits on its centre line, 0.3 blocks either side
/// give the slope, and the yaw and pitch follow that line. Off a rail the pose is unchanged.
pub fn on_rail(world: &impl World, pose: CartPose) -> CartPose {
    let [x, y, z] = pose.pos;
    let Some(centre) = snap_to_rail(world, x, y, z) else {
        return pose;
    };
    let front = snap_with_offset(world, x, y, z, 0.300_000_011_920_928_96).unwrap_or(centre);
    let back = snap_with_offset(world, x, y, z, -0.300_000_011_920_928_96).unwrap_or(centre);
    let pos = [centre[0], (front[1] + back[1]) / 2.0, centre[2]];
    let dir = [back[0] - front[0], back[1] - front[1], back[2] - front[2]];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len == 0.0 {
        return CartPose { pos, ..pose };
    }
    let (dx, dy, dz) = (dir[0] / len, dir[1] / len, dir[2] / len);
    CartPose {
        pos,
        yaw: (dz.atan2(dx) * 180.0 / std::f64::consts::PI) as f32,
        pitch: (dy.atan() * 73.0) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rails(Vec<([i32; 3], u16)>);

    impl World for Rails {
        fn block(&self, x: i32, y: i32, z: i32) -> u16 {
            self.0
                .iter()
                .find(|(at, _)| *at == [x, y, z])
                .map_or(0, |(_, state)| *state)
        }
    }

    fn rail(at: [i32; 3], meta: u16) -> ([i32; 3], u16) {
        (at, 66 << 4 | meta)
    }

    /// The real `snapPositionToRail` and `snapPositionToRailWithOffset` of a `MinecartEntity` over
    /// every rail shape, in 1.8.9, to the last bit.
    #[test]
    fn rail_snap_matches_the_real_minecart() {
        let mut script = String::from("floor stone\n");
        let mut rails = Vec::new();
        for meta in 0..10u16 {
            let at = [meta as i32 * 4, 64, 0];
            script += &format!("state {} 64 0 66 {meta}\n", at[0]);
            rails.push(rail(at, meta));
        }
        script += "start 0.5 70 0.5 0\n";
        let probes = [(0.31, 64.2, 0.77), (0.9, 64.0, 0.12), (0.5, 65.0, 0.5)];
        let offsets = ["none", "0.30000001192092896", "-0.30000001192092896"];
        let mut cases = Vec::new();
        for meta in 0..10 {
            for (px, py, pz) in probes {
                for offset in offsets {
                    let x = f64::from(meta * 4) + px;
                    script += &format!("rail {x} {py} {pz} {offset}\n");
                    cases.push((meta, x, py, pz, offset));
                }
            }
        }
        let out = oracle::Oracle::get().run(&script).unwrap();
        let out = &out[out.len() - cases.len()..];
        let world = Rails(rails);
        for (line, (meta, x, y, z, offset)) in out.iter().zip(cases) {
            let got = match offset {
                "none" => snap_to_rail(&world, x, y, z),
                o => snap_with_offset(&world, x, y, z, o.parse().unwrap()),
            };
            let want = got.map_or("null".to_owned(), |p| {
                p.map(|c| format!("{:016x}", c.to_bits())).join(" ")
            });
            assert_eq!(*line, want, "shape {meta} at {x} {y} {z} offset {offset}");
        }
    }

    #[test]
    fn a_cart_on_a_slope_pitches_and_off_a_rail_keeps_its_pose() {
        let world = Rails(vec![rail([0, 64, 0], 2)]);
        let on = on_rail(
            &world,
            CartPose {
                pos: [0.5, 64.5, 0.5],
                yaw: 90.0,
                pitch: 0.0,
            },
        );
        assert!(on.pitch.abs() > 10.0 && on.pos[1] > 64.0, "{on:?}");
        let off = CartPose {
            pos: [9.5, 64.5, 0.5],
            yaw: 90.0,
            pitch: 7.0,
        };
        assert_eq!(on_rail(&world, off), off);
    }

    #[test]
    fn the_hit_wobble_runs_only_while_the_timer_does() {
        assert_eq!(hit_wobble(0, 30.0, 1, 0.0), 0.0);
        assert_eq!(hit_wobble(2, 30.0, 1, 2.0), 0.0);
        let w = hit_wobble(8, 20.0, -1, 0.5);
        // vanilla: sin(7.5) = 0.938 (radians), so -0.938 * 7.5 * 19.5 / 10 = -13.718
        assert!((w - -13.718).abs() < 0.01);
        assert_eq!(hit_wobble(8, 0.25, 1, 0.5), 0.0);
    }

    #[test]
    fn display_block_defaults_per_cart_and_follows_the_custom_tile() {
        let chest = VehicleMeta {
            cart: Some(1),
            ..Default::default()
        };
        assert_eq!(chest.display_block(), (54 << 4 | 2, 8));
        let custom = VehicleMeta {
            custom_tile: Some(1),
            tile: Some(3 | 2 << 12),
            tile_offset: Some(4),
            cart: Some(1),
            ..Default::default()
        };
        assert_eq!(custom.display_block(), (3 << 4 | 2, 4));
        assert_eq!(VehicleMeta::default().display_block(), (0, 6));
    }
}
