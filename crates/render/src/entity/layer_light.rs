//! Per-layer light of one entity: the body gets the hurt flag, plain layers do not, full-bright layers ignore light.

use crate::mesh::Vertex;
use std::ops::Range;

/// `vertices` is everything built so far; this entity starts at `start`. `bright` is where its full-bright
/// layers begin; `plain` ranges are layers whose `colorsWhenDamaged()` is false and take `plain_level` instead.
pub(super) fn apply(
    vertices: &mut [Vertex],
    start: usize,
    bright: usize,
    plain: Vec<Range<usize>>,
    level: [f32; 2],
    plain_level: [f32; 2],
) {
    let bright = bright.max(start);
    for v in &mut vertices[start..bright] {
        v.light = level;
    }
    for r in plain {
        for v in &mut vertices[r] {
            v.light = plain_level;
        }
    }
    for v in &mut vertices[bright..] {
        v.light = [1.0, 1.0];
    }
}
