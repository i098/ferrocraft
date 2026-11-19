//! Flowing liquid tops: `BlockLiquid.getFlowVector` and the flow angle `BlockFluidRenderer` turns the
//! flow texture by.

use super::Mesher;
use crate::blocks::material_solid;
use world::shape::blocks_movement;

/// `Plane.HORIZONTAL`: north, east, south, west.
const HORIZONTAL: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1.0e-4 {
        [0.0; 3]
    } else {
        v.map(|c| c / len)
    }
}

impl Mesher<'_> {
    /// `BlockLiquid.getFlowDirection`: the angle of the flow, or `None` (vanilla's -1000) for still liquid.
    pub(super) fn flow_angle(
        &self,
        at: [i32; 3],
        state: u16,
        same: &dyn Fn(u16) -> bool,
    ) -> Option<f32> {
        let v = self.flow_vector(at, state, same);
        (v[0] != 0.0 || v[2] != 0.0).then(|| (v[2].atan2(v[0]) - std::f64::consts::FRAC_PI_2) as f32)
    }

    fn flow_vector(&self, at: [i32; 3], state: u16, same: &dyn Fn(u16) -> bool) -> [f64; 3] {
        // `getEffectiveFlowDecay`: -1 for another material, else the level with falling liquid as 0.
        let decay = |d: [i32; 3]| {
            let s = self.st(at, d);
            match (same(s), s & 15) {
                (false, _) => -1,
                (true, m) if m >= 8 => 0,
                (true, m) => m as i32,
            }
        };
        let own = decay([0; 3]);
        let mut v = [0.0f64; 3];
        for d in HORIZONTAL {
            let j = decay(d);
            let k = if j < 0 {
                let beside = self.st(at, d);
                if blocks_movement(beside >> 4) {
                    continue;
                }
                match decay([d[0], -1, d[2]]) {
                    below if below >= 0 => below - (own - 8),
                    _ => continue,
                }
            } else {
                j - own
            };
            v[0] += f64::from(d[0] * k);
            v[2] += f64::from(d[2] * k);
        }
        if state & 15 >= 8 && HORIZONTAL.iter().any(|&d| self.liquid_wall(at, d, same)) {
            v = normalize(v);
            v[1] -= 6.0;
        }
        normalize(v)
    }

    /// `BlockLiquid.isBlockSolid` on the cell beside the liquid and the one above it.
    fn liquid_wall(&self, at: [i32; 3], d: [i32; 3], same: &dyn Fn(u16) -> bool) -> bool {
        [0, 1].iter().any(|&dy| {
            let s = self.st(at, [d[0], dy, d[2]]);
            !same(s) && s >> 4 != 79 && material_solid(s)
        })
    }
}

/// `BlockFluidRenderer`: texel coordinates of a top corner, `8 +- (sin, cos) * 4` per axis (a 16 texel tile).
pub(super) fn flow_uv(angle: f32, c: [f32; 3]) -> [f32; 2] {
    let (s, k) = (angle.sin() * 0.25, angle.cos() * 0.25);
    let [u, v] = match (c[0] > 0.5, c[2] > 0.5) {
        (false, false) => [-k - s, -k + s],
        (false, true) => [-k + s, k + s],
        (true, true) => [k + s, k - s],
        (true, false) => [k - s, -k - s],
    };
    [0.5 + u, 0.5 + v]
}
