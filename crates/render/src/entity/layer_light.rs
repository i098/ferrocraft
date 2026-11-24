//! Per-layer light of one entity: the body gets the hurt flag, plain layers do not, eye layers take [`EYES_LIGHT`].

use crate::mesh::Vertex;
use std::ops::Range;

/// `LayerSpiderEyes`, `LayerEndermanEyes` and the dragon eyes set light map coordinate 61680: block 240, sky 0.
pub(super) const EYES_LIGHT: [f32; 2] = [0.0, 1.0];
/// The coordinate's U is 240.97, past the edge of the `GL_CLAMP` light map. Linear filtering blends the lit
/// edge texel half-and-half with the black border, so the eyes get half of the block-light colour.
pub(super) const EYES_BORDER_FACTOR: f32 = 0.5;

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
        v.light = EYES_LIGHT;
    }
}
