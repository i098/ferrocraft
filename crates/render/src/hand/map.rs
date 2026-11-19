//! The held filled map: `ItemRenderer.renderItemMap` (bfn offsets 0-402), `renderPlayerArms`
//! (bfn `b(bet)`, `a(bln)`, `b(bln)`) and the decorations of `MapItemRenderer.Instance.render`
//! (avq$a offsets 218-545).

use super::{arm_draw, chain, ident, item_light, point, rotate, scale, scale3, translate};
use super::{Draw, Hand, Mat, Side};
use crate::mesh::Vertex;
use std::f32::consts::PI;

/// A `MapDecoration` of a map: icon `kind` of `map_icons.png`, a position in half map pixels from
/// the centre and a rotation in sixteenths of a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapIcon {
    pub kind: u8,
    pub x: i8,
    pub z: i8,
    pub rotation: u8,
}

pub const ICONS_TEXTURE: &str = "textures/map/map_icons.png";
pub const BACKGROUND_TEXTURE: &str = "textures/map/map_background.png";

impl MapIcon {
    /// The quad of decoration number `k`, in map pixels, and its uv in `map_icons.png`.
    pub(crate) fn quad(&self, k: usize) -> ([[f32; 3]; 4], [[f32; 2]; 4]) {
        let place = chain(&[
            translate(
                f32::from(self.x) / 2.0 + 64.0,
                f32::from(self.z) / 2.0 + 64.0,
                -0.02,
            ),
            rotate(f32::from(self.rotation) * 360.0 / 16.0, 2),
            scale3([4.0, 4.0, 3.0]),
            translate(-0.125, 0.125, 0.0),
        ]);
        let z = k as f32 * -0.001;
        let corners = [
            [-1.0, 1.0, z],
            [1.0, 1.0, z],
            [1.0, -1.0, z],
            [-1.0, -1.0, z],
        ]
        .map(|p| point(&place, p));
        let (u0, v0) = (
            f32::from(self.kind % 4) / 4.0,
            f32::from(self.kind / 4) / 4.0,
        );
        let (u1, v1) = (u0 + 0.25, v0 + 0.25);
        (corners, [[u0, v0], [u1, v0], [u1, v1], [u0, v1]])
    }
}

/// The texture a part of the held map samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapTex {
    Skin,
    Background,
    Colors,
    Icons,
}

/// `getMapAngleFromPitch`: 1 looking up, 0 looking down past 40 degrees.
fn angle_from_pitch(pitch: f32) -> f32 {
    let f = (1.0 - pitch / 45.0 + 0.1).clamp(0.0, 1.0);
    -(f * PI).cos() * 0.5 + 0.5
}

/// Right arm: `ModelPlayer` with `heldItemRight` 1 turns it by `-PI / 10` about x (`setRotationAngles`)
/// and idles it by `cos(0) * 0.05 + 0.05` about z.
fn right_arm(to_arms: Mat) -> Mat {
    chain(&[
        to_arms,
        rotate(54.0, 1),
        rotate(64.0, 0),
        rotate(-62.0, 2),
        translate(0.25, -0.85, 0.75),
        translate(-5.0 / 16.0, 2.0 / 16.0, 0.0),
        rotate(0.1f32.to_degrees(), 2),
        rotate((-PI / 10.0).to_degrees(), 0),
    ])
}

fn left_arm(to_arms: Mat) -> Mat {
    chain(&[
        to_arms,
        rotate(92.0, 1),
        rotate(45.0, 0),
        rotate(41.0, 2),
        translate(-0.3, -1.1, 0.45),
        translate(5.0 / 16.0, 2.0 / 16.0, 0.0),
        rotate(-0.1f32.to_degrees(), 2),
    ])
}

/// The parts of a held map in eye space (each `Draw.model` is the identity): both arms, the
/// background, the map colours when `colors` and the decorations `icons`.
pub fn parts(
    hand: &Hand,
    skin_height: u32,
    colors: bool,
    icons: &[MapIcon],
) -> Vec<(Draw, MapTex)> {
    let (swing, root) = (hand.swing, hand.swing.sqrt());
    let angle = angle_from_pitch(hand.look.pitch);
    let to_arms = chain(&[
        hand.look.sway(),
        translate(
            -0.4 * (root * PI).sin(),
            0.2 * (root * 2.0 * PI).sin(),
            -0.2 * (swing * PI).sin(),
        ),
        translate(0.0, 0.04, -0.72),
        translate(0.0, hand.lowered * -1.2, 0.0),
        translate(0.0, angle * -0.5, 0.0),
        rotate(90.0, 1),
        rotate(angle * -85.0, 2),
    ]);
    let to_map = chain(&[
        to_arms,
        rotate((swing * swing * PI).sin() * -20.0, 1),
        rotate((root * PI).sin() * -20.0, 2),
        rotate((root * PI).sin() * -80.0, 0),
        scale(0.38),
        rotate(90.0, 1),
        rotate(180.0, 2),
        translate(-1.0, -1.0, 0.0),
        scale(0.015_625),
    ]);
    // Both quads and every decoration face the normal `(0, 0, -1)` set before the background.
    let shade = item_light(&to_map, [0.0, 0.0, -1.0], &hand.look);
    let vertex = |p: [f32; 3], uv: [f32; 2]| Vertex {
        pos: point(&to_map, p),
        uv,
        color: [shade, shade, shade, 1.0],
        light: [1.0, 0.0],
    };
    let quad = |corners: [[f32; 3]; 4], uv: [[f32; 2]; 4]| Draw {
        vertices: (0..4).map(|i| vertex(corners[i], uv[i])).collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        model: ident(),
    };
    let square = |lo: f32, hi: f32, z: f32| {
        quad(
            [[lo, hi, z], [hi, hi, z], [hi, lo, z], [lo, lo, z]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        )
    };
    let skin = |model: Mat, side| {
        let mut d = arm_draw(model, &hand.look, skin_height, side);
        for v in &mut d.vertices {
            v.pos = point(&model, v.pos);
        }
        d.model = ident();
        (d, MapTex::Skin)
    };
    let mut out = vec![
        skin(right_arm(to_arms), Side::Right),
        skin(left_arm(to_arms), Side::Left),
        (square(-7.0, 135.0, 0.0), MapTex::Background),
    ];
    if colors {
        out.push((square(0.0, 128.0, -0.01), MapTex::Colors));
    }
    for (k, icon) in icons.iter().enumerate() {
        let (corners, uv) = icon.quad(k);
        out.push((quad(corners, uv), MapTex::Icons));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_angle_runs_from_flat_to_tilted() {
        assert!((angle_from_pitch(-90.0) - 1.0).abs() < 1e-6);
        assert!(angle_from_pitch(90.0).abs() < 1e-6);
    }

    /// Decoration 0 of type 5 at the map centre: an 8 px square, shifted by `translate(-0.125, 0.125)`
    /// before the scale of 4, whose uv is cell (1, 1) of the 4x4 sheet.
    #[test]
    fn icon_quad_is_centred_and_picks_its_cell() {
        let (c, uv) = MapIcon {
            kind: 5,
            x: 0,
            z: 0,
            rotation: 0,
        }
        .quad(0);
        assert_eq!(uv[0], [0.25, 0.25]);
        assert_eq!(uv[2], [0.5, 0.5]);
        let mean = |i: usize| c.iter().map(|p| p[i]).sum::<f32>() / 4.0;
        assert!((mean(0) - 63.5).abs() < 1e-4 && (mean(1) - 64.5).abs() < 1e-4);
        assert!((c[1][0] - c[0][0] - 8.0).abs() < 1e-4);
    }
}
