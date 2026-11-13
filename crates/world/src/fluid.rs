//! `World.handleMaterialAcceleration` for water: the in-water test and the flow push of
//! `BlockLiquid.getFlowVector` on an entity.

use crate::shape::is_solid_material;

/// The four horizontal neighbours in `EnumFacing.Plane.HORIZONTAL` order: north, east, south, west.
const SIDES: [[i32; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];

fn floor(v: f64) -> i32 {
    v.floor() as i32
}

fn is_water(id: u16) -> bool {
    matches!(id, 8 | 9)
}

/// `BlockLiquid.getEffectiveFlowDecay`: the level of a water block, `None` for other blocks.
fn decay(state: u16) -> Option<i32> {
    is_water(state >> 4).then(|| {
        let level = i32::from(state & 15);
        if level >= 8 {
            0
        } else {
            level
        }
    })
}

/// `BlockLiquid.causesDownwardCurrent` for a block that borders flowing water on `up`/side.
fn downward_current(state: u16, up: bool) -> bool {
    let id = state >> 4;
    if is_water(id) {
        false
    } else if up {
        true
    } else {
        id != 79 && is_solid_material(id)
    }
}

/// `BlockLiquid.getFlowVector` of the water block `state` at `p`.
fn flow_vector(at: &dyn Fn([i32; 3]) -> u16, p: [i32; 3], state: u16) -> [f64; 3] {
    let level = decay(state).unwrap_or(0);
    let mut v = [0.0_f64; 3];
    for [dx, dz] in SIDES {
        let n = [p[0] + dx, p[1], p[2] + dz];
        let ns = at(n);
        let mut j = decay(ns);
        let k = match j {
            Some(j) => j - level,
            None => {
                if crate::shape::blocks_movement(ns >> 4) {
                    0
                } else {
                    j = decay(at([n[0], n[1] - 1, n[2]]));
                    j.map_or(0, |j| j - (level - 8))
                }
            }
        };
        if k != 0 {
            v[0] += f64::from(dx * k);
            v[2] += f64::from(dz * k);
        }
    }
    if state & 15 >= 8 {
        let falls = SIDES.iter().any(|[dx, dz]| {
            let n = [p[0] + dx, p[1], p[2] + dz];
            downward_current(at(n), false) || downward_current(at([n[0], n[1] + 1, n[2]]), false)
        });
        if falls {
            v = normalize(v);
            v[1] -= 6.0;
        }
    }
    normalize(v)
}

/// `Vec3d.normalize`: `MathHelper.sqrt_double` rounds the length to `f32`.
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = f64::from((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() as f32);
    if len < 1.0e-4 {
        [0.0; 3]
    } else {
        v.map(|c| c / len)
    }
}

/// Whether the box `min`..`max` (already shrunk as `handleWaterMovement` does) touches water,
/// and the velocity that the flow adds when the entity is `pushed` by water.
pub fn water_drag(
    at: &dyn Fn([i32; 3]) -> u16,
    min: [f64; 3],
    max: [f64; 3],
    pushed: bool,
) -> (bool, [f64; 3]) {
    let mut inside = false;
    let mut sum = [0.0_f64; 3];
    for x in floor(min[0])..floor(max[0] + 1.0) {
        for y in floor(min[1])..floor(max[1] + 1.0) {
            for z in floor(min[2])..floor(max[2] + 1.0) {
                let state = at([x, y, z]);
                if !is_water(state >> 4) {
                    continue;
                }
                let level = if state & 15 >= 8 { 0 } else { state & 15 };
                let top = f64::from((y + 1) as f32 - f32::from(level + 1) / 9.0);
                if f64::from(floor(max[1] + 1.0)) >= top {
                    inside = true;
                    let f = flow_vector(at, [x, y, z], state);
                    for (s, c) in sum.iter_mut().zip(f) {
                        *s += c;
                    }
                }
            }
        }
    }
    let len = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    if pushed && len > 0.0 {
        let n = normalize(sum);
        return (inside, n.map(|c| c * 0.014));
    }
    (inside, [0.0; 3])
}
