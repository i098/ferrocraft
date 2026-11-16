//! Third-person camera distance.

use crate::dig::raycast_dir;

/// `GameRenderer.transformCamera`: the third-person distance. Eight rays leave corners offset by 0.1 around
/// the eye along `-look` (each `World.rayTrace` against block shapes); the shortest hit distance to the eye
/// wins over `max_dist`.
pub fn camera_distance(
    state: &dyn Fn([i32; 3]) -> u16,
    eye: [f64; 3],
    look: [f64; 3],
    max_dist: f64,
) -> f64 {
    let back = look.map(|d| -d);
    (0..8).fold(max_dist, |best, i| {
        let origin: [f64; 3] =
            std::array::from_fn(|a| eye[a] + f64::from(((i >> a) & 1) * 2 - 1) * 0.1);
        match raycast_dir(state, origin, back, max_dist) {
            Some((_, t)) => {
                let d = (0..3)
                    .map(|a| (origin[a] + back[a] * t - eye[a]).powi(2))
                    .sum::<f64>();
                best.min(d.sqrt())
            }
            None => best,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dig::look_vector;

    #[test]
    fn camera_stops_at_a_slab_and_keeps_full_distance_when_clear() {
        // transformCamera: 8 corner rays of the eye, shortest hit wins; a clear path gives max_dist.
        let eye = [0.5, 64.25, 0.5];
        let look = look_vector(0.0, 0.0);
        let slab = |p: [i32; 3]| if p == [0, 64, -2] { 44 << 4 } else { 0 };
        let d = camera_distance(&slab, eye, look, 4.0);
        assert!((d - (1.5f64 * 1.5 + 0.02).sqrt()).abs() < 1e-9, "{d}");
        assert_eq!(camera_distance(&|_| 0, eye, look, 4.0), 4.0);
        // The slab's empty upper half does not stop a camera 0.6 above its top.
        assert_eq!(camera_distance(&slab, [0.5, 64.8, 0.5], look, 4.0), 4.0);
    }
}
