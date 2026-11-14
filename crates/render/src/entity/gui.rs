//! `GuiInventory.drawEntityOnScreen`: one entity drawn into a GUI rectangle. The model is the world
//! mesh of [`entity::build`], projected on the CPU with the vanilla matrix
//! `translate(x, y, 50) scale(-s, s, s) rotateZ(180) rotateX(tilt)` and lit with the two lights of
//! `enableStandardItemLighting`, so it joins the overlay quad list in draw order.

use super::{build_unshaded, EntityDraw, Skins};
use crate::{Blend, Quad};
use std::borrow::Cow;

/// Name under which the renderer registers the entity texture atlas as an overlay texture.
pub(crate) const ATLAS: &str = "<entity atlas>";

/// One `drawEntityOnScreen` call, in window pixels.
#[derive(Clone, Debug)]
pub(crate) struct GuiEntity {
    /// Feet position of the model (`posX`, `posY`).
    pub origin: [f32; 2],
    /// Pixels per block (`scale`, times the GUI scale factor).
    pub scale: f32,
    /// `mouseX` and `mouseY` of the call: the offset of the cursor from the model, in GUI units.
    pub mouse: [f32; 2],
    /// The entity; only its look (skin, armor, limbs, skin parts) is read, not its position or turn.
    pub draw: EntityDraw,
}

/// Turns of the model for a cursor offset, in degrees: body yaw, head yaw and pitch.
/// `drawEntityOnScreen` sets `renderYawOffset`, `rotationYaw` (and the head) and `rotationPitch`.
pub fn turns(mouse: [f32; 2]) -> (f32, f32, f32) {
    let yaw = (f64::from(mouse[0] / 40.0).atan() as f32) * 20.0;
    let pitch = -(f64::from(mouse[1] / 40.0).atan() as f32) * 20.0;
    (yaw, yaw * 2.0, pitch)
}

type V3 = [f32; 3];

/// `enableStandardItemLighting`: the directions toward `LIGHT0` and `LIGHT1` in eye space, after the
/// call's `rotate(135, 0, 1, 0)` and the `scale(-1, 1, 1) rotateZ(180)` already on the stack.
fn lights() -> [V3; 2] {
    let (s, c) = 135f32.to_radians().sin_cos();
    [[0.2, 1.0, -0.7], [-0.2, 1.0, 0.7]].map(|l| {
        let len = dot(l, l).sqrt();
        let (x, y, z) = (l[0] / len, l[1] / len, l[2] / len);
        // Ry(135), then diag(1, -1, 1).
        [x * c + z * s, -y, -x * s + z * c]
    })
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Two lights of 0.6 over the 0.4 ambient term, for a unit normal in eye space.
fn brightness(normal: V3) -> f32 {
    let lit: f32 = lights().iter().map(|&l| dot(normal, l).max(0.0)).sum();
    (0.4 + 0.6 * lit).min(1.0)
}

fn argb(rgb: [f32; 3], light: f32) -> u32 {
    let ch = |v: f32| ((v * light).clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    0xFF00_0000 | ch(rgb[0]) << 16 | ch(rgb[1]) << 8 | ch(rgb[2])
}

impl crate::Renderer {
    /// Overlay quads of one `drawEntityOnScreen` call, to splice into the quad list in draw order.
    pub fn gui_entity(
        &self,
        origin: [f32; 2],
        scale: f32,
        mouse: [f32; 2],
        draw: &EntityDraw,
    ) -> Vec<Quad> {
        let draw = draw.clone();
        let e = GuiEntity {
            origin,
            scale,
            mouse,
            draw,
        };
        quads(&e, &self.entity_skins)
    }
}

/// The overlay quads of `e`, back to front. Quads that face away are culled.
pub(crate) fn quads(e: &GuiEntity, skins: &Skins) -> Vec<Quad> {
    let (yaw, head_yaw, pitch) = turns(e.mouse);
    let mut draw = e.draw.clone();
    draw.pos = [0.0; 3];
    (draw.yaw, draw.head_yaw, draw.pitch) = (yaw, head_yaw, pitch);
    let mesh = build_unshaded(&[draw], skins);
    // `rotate(-atan(mouseY / 40) * 20, 1, 0, 0)` is the pitch turn above.
    let (st, ct) = pitch.to_radians().sin_cos();
    let tilt = |p: V3| [p[0], p[1] * ct - p[2] * st, p[1] * st + p[2] * ct];
    let mut out: Vec<(f32, Quad)> = Vec::new();
    for tri in mesh.indices.as_chunks::<6>().0 {
        let base = tri[0] as usize;
        let v = &mesh.vertices[base..base + 4];
        let p = [0, 1, 2, 3].map(|i| tilt(v[i].pos));
        let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let len = dot(n, n).sqrt();
        // Eye z is the tilted z; eye y is the tilted y turned upside down by `rotateZ(180)`.
        if len == 0.0 || n[2] <= 0.0 {
            continue;
        }
        let light = brightness([n[0] / len, -n[1] / len, n[2] / len]);
        let depth = p.iter().map(|q| q[2]).sum::<f32>();
        out.push((
            depth,
            Quad {
                tex: Cow::Borrowed(ATLAS),
                blend: Blend::Alpha,
                pos: p.map(|q| [e.origin[0] + e.scale * q[0], e.origin[1] - e.scale * q[1]]),
                uv: [0, 1, 2, 3].map(|i| v[i].uv),
                color: [0, 1, 2, 3]
                    .map(|i| argb([v[i].color[0], v[i].color[1], v[i].color[2]], light)),
            },
        ));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, q)| q).collect()
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_offset_turns_body_head_and_pitch() {
        // atan(1) = 45 degrees, times 20 for the body, 40 for the head.
        let (body, head, pitch) = turns([40.0, 40.0]);
        let quarter = std::f32::consts::FRAC_PI_4;
        assert!((body - quarter * 20.0).abs() < 1e-4);
        assert!((head - quarter * 40.0).abs() < 1e-4);
        assert!((pitch + quarter * 20.0).abs() < 1e-4);
        assert_eq!(turns([0.0, 0.0]), (0.0, 0.0, -0.0));
    }

    #[test]
    fn offsets_saturate_like_atan() {
        let (body, _, pitch) = turns([1.0e9, -1.0e9]);
        assert!((body - 90f32.to_radians() * 20.0).abs() < 1e-3);
        assert!((pitch - 90f32.to_radians() * 20.0).abs() < 1e-3);
    }
}
