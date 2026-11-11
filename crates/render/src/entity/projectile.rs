//! Shot and thrown objects: the arrow model, item sprites that face the camera, the fishing bobber
//! and its line. Object ids are the 1.8.9 spawn-object types.

use super::{skin::Skin, EntityDraw, Skins, V3};
use crate::mesh::{MeshData, Vertex};
use std::f32::consts::PI;

pub const ARROW: i8 = 60;
pub const SNOWBALL: i8 = 61;
pub const EGG: i8 = 62;
pub const FIREBALL: i8 = 63;
pub const SMALL_FIREBALL: i8 = 64;
pub const ENDER_PEARL: i8 = 65;
pub const POTION: i8 = 73;
pub const FISHING_BOBBER: i8 = 90;

/// Whether `build` draws this object type itself.
pub fn handles(id: i8) -> bool {
    matches!(
        id,
        ARROW | SNOWBALL | EGG | FIREBALL | SMALL_FIREBALL | ENDER_PEARL | POTION | FISHING_BOBBER
    )
}

/// Meshes of one projectile at its world position.
pub fn build(out: &mut MeshData, id: i8, d: &EntityDraw, skins: &Skins) {
    // (sprite, width in blocks, rise above the feet)
    let (skin, size, rise) = match id {
        ARROW => return arrow(out, d, skins),
        SNOWBALL => (Skin::Snowball, 0.25, 0.0),
        EGG => (Skin::Egg, 0.25, 0.0),
        ENDER_PEARL => (Skin::EnderPearl, 0.25, 0.0),
        POTION => (Skin::SplashPotion, 0.25, 0.0),
        // `ProjectileRenderer`: a 1x1 sprite scaled by 2 (large) or 0.5 (small), raised a quarter.
        FIREBALL => (Skin::FireCharge, 2.0, 0.5),
        SMALL_FIREBALL => (Skin::FireCharge, 0.5, 0.125),
        // `FishingBobberRenderer`: scale 0.5 of a unit quad.
        _ => (Skin::Bobber, 0.5, 0.0),
    };
    billboard(out, d, skins, skin, size, rise);
    if let (FISHING_BOBBER, Some((hand, eye))) = (id, d.line) {
        line(out, d, hand, eye, skins);
    }
}

/// One light-independent vertex; the caller sets the light afterwards.
fn vertex(pos: V3, uv: [f32; 2]) -> Vertex {
    Vertex {
        pos,
        uv,
        color: [1.0, 1.0, 1.0, 1.0],
        light: [1.0, 1.0],
    }
}

fn push_quad(out: &mut MeshData, quad: [Vertex; 4]) {
    let base = out.vertices.len() as u32;
    out.vertices.extend_from_slice(&quad);
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A unit-square sprite (`size` blocks wide) centred `rise` above the feet, turned to the viewer as
/// `rotatef(180 - cameraYaw, y)` then `rotatef(-cameraPitch, x)` do.
fn billboard(out: &mut MeshData, d: &EntityDraw, skins: &Skins, skin: Skin, size: f32, rise: f32) {
    let v_off = skins.v_offset(skin);
    let texels = match skin {
        Skin::Bobber => 8.0,
        _ => 16.0,
    };
    let (yaw, pitch): (f32, f32) = (d.view[0], d.view[1]);
    let tex = [Skins::width() as f32, Skins::height() as f32];
    let corner = |x: f32, y: f32, u: f32, v: f32| {
        let p = rot_y(
            rot_x([x * size, y * size + rise, 0.0], (-pitch).to_radians()),
            (180.0 - yaw).to_radians(),
        );
        vertex(
            [
                (d.pos[0] + f64::from(p[0])) as f32,
                (d.pos[1] + f64::from(p[1])) as f32,
                (d.pos[2] + f64::from(p[2])) as f32,
            ],
            [u * texels / tex[0], (v * texels + v_off) / tex[1]],
        )
    };
    push_quad(
        out,
        [
            corner(-0.5, -0.5, 0.0, 1.0),
            corner(0.5, -0.5, 1.0, 1.0),
            corner(0.5, 0.5, 1.0, 0.0),
            corner(-0.5, 0.5, 0.0, 0.0),
        ],
    );
}

fn rot_x(p: V3, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

fn rot_y(p: V3, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

fn rot_z(p: V3, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

/// `ArrowRenderer`: two end caps and four 16x4 long faces of the 32x32 texture, in 1/16 blocks.
fn arrow(out: &mut MeshData, d: &EntityDraw, skins: &Skins) {
    let v_off = skins.v_offset(Skin::Arrow);
    let tex = [Skins::width() as f32, Skins::height() as f32];
    let uv = |u: f32, v: f32| [u * 32.0 / tex[0], (v * 32.0 + v_off) / tex[1]];
    let place = |p: V3, spin: f32| -> V3 {
        let s = 0.056_25;
        let p = [(p[0] - 4.0) * s, p[1] * s, p[2] * s];
        let p = rot_x(p, spin);
        let p = rot_z(p, d.pitch.to_radians());
        let p = rot_y(p, (d.yaw - 90.0).to_radians());
        [
            (d.pos[0] + f64::from(p[0])) as f32,
            (d.pos[1] + f64::from(p[1])) as f32,
            (d.pos[2] + f64::from(p[2])) as f32,
        ]
    };
    let spin = 45.0_f32.to_radians();
    let (u0, u1, v0, v1) = (0.0, 0.156_25, 5.0 / 32.0, 10.0 / 32.0);
    push_quad(
        out,
        [
            vertex(place([-7.0, -2.0, -2.0], spin), uv(u0, v0)),
            vertex(place([-7.0, -2.0, 2.0], spin), uv(u1, v0)),
            vertex(place([-7.0, 2.0, 2.0], spin), uv(u1, v1)),
            vertex(place([-7.0, 2.0, -2.0], spin), uv(u0, v1)),
        ],
    );
    push_quad(
        out,
        [
            vertex(place([-7.0, 2.0, -2.0], spin), uv(u0, v0)),
            vertex(place([-7.0, 2.0, 2.0], spin), uv(u1, v0)),
            vertex(place([-7.0, -2.0, 2.0], spin), uv(u1, v1)),
            vertex(place([-7.0, -2.0, -2.0], spin), uv(u0, v1)),
        ],
    );
    for i in 1..=4 {
        let spin = spin + i as f32 * PI / 2.0;
        push_quad(
            out,
            [
                vertex(place([-8.0, -2.0, 0.0], spin), uv(0.0, 0.0)),
                vertex(place([8.0, -2.0, 0.0], spin), uv(0.5, 0.0)),
                vertex(place([8.0, 2.0, 0.0], spin), uv(0.5, 5.0 / 32.0)),
                vertex(place([-8.0, 2.0, 0.0], spin), uv(0.0, 5.0 / 32.0)),
            ],
        );
    }
}

/// `FishingBobberRenderer`: 16 segments of a sagging line from the rod tip to the bobber, drawn as
/// thin black ribbons that face the viewer.
fn line(out: &mut MeshData, d: &EntityDraw, hand: [f64; 3], eye: f32, skins: &Skins) {
    let white = skins.v_offset(Skin::White);
    let uv = [
        0.5 / Skins::width() as f32,
        (white + 0.5) / Skins::height() as f32,
    ];
    let dx = (hand[0] - d.pos[0]) as f32;
    let dy = (hand[1] - d.pos[1]) as f32 - 0.25 + eye;
    let dz = (hand[2] - d.pos[2]) as f32;
    let at = |i: u32| -> V3 {
        let f = i as f32 / 16.0;
        [
            (d.pos[0] + f64::from(dx * f)) as f32,
            (d.pos[1] + f64::from(dy * (f * f + f) * 0.5 + 0.25)) as f32,
            (d.pos[2] + f64::from(dz * f)) as f32,
        ]
    };
    // Ribbon half-width, perpendicular to both the segment and the view direction.
    let (yaw, pitch) = (d.view[0].to_radians(), d.view[1].to_radians());
    let look = [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ];
    for i in 0..16 {
        let (a, b) = (at(i), at(i + 1));
        let seg = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let side = super::cross(seg, look);
        let len = super::dot(side, side).sqrt();
        if len < 1e-6 {
            continue;
        }
        let w = 0.006 / len;
        let off = [side[0] * w, side[1] * w, side[2] * w];
        let quad = [
            [a[0] - off[0], a[1] - off[1], a[2] - off[2]],
            [a[0] + off[0], a[1] + off[1], a[2] + off[2]],
            [b[0] + off[0], b[1] + off[1], b[2] + off[2]],
            [b[0] - off[0], b[1] - off[1], b[2] - off[2]],
        ];
        // Black, and visible from both sides.
        let black = |p: V3| Vertex {
            color: [0.0, 0.0, 0.0, 1.0],
            ..vertex(p, uv)
        };
        push_quad(out, quad.map(black));
        push_quad(out, [quad[3], quad[2], quad[1], quad[0]].map(black));
    }
}

/// `FishingBobberRenderer`: where the line leaves the rod, in world space. `first_person` is the
/// local player in the first-person view; every other thrower is drawn from the body instead.
pub struct Thrower {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub body_yaw: f32,
    pub eye_height: f32,
    pub sneaking: bool,
    /// Arm swing progress `0..1`.
    pub swing: f32,
    pub first_person: bool,
}

impl Thrower {
    /// The line start and the extra eye offset the vanilla code adds to the vertical delta.
    pub fn rod_tip(&self) -> ([f64; 3], f32) {
        if self.first_person {
            let s = (self.swing.sqrt() * PI).sin();
            // `Vec3d.rotateX` turns the other way round from `glRotatef`.
            let v = rot_x([-0.36, 0.03, 0.35], self.pitch.to_radians());
            let v = rot_y(v, -self.yaw.to_radians());
            let v = rot_y(v, s * 0.5);
            let v = rot_x(v, s * 0.7);
            (
                [
                    self.pos[0] + f64::from(v[0]),
                    self.pos[1] + f64::from(v[1]),
                    self.pos[2] + f64::from(v[2]),
                ],
                self.eye_height,
            )
        } else {
            let (sin, cos) = self.body_yaw.to_radians().sin_cos();
            (
                [
                    self.pos[0] - f64::from(cos * 0.35 + sin * 0.8),
                    self.pos[1] + f64::from(self.eye_height) - 0.45,
                    self.pos[2] - f64::from(sin * 0.35) + f64::from(cos * 0.8),
                ],
                if self.sneaking { -0.1875 } else { 0.0 },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{build as build_all, Class};

    fn draw(id: i8, line: Option<([f64; 3], f32)>) -> EntityDraw {
        let mut d = super::super::tests::steve();
        d.class = Class::Object(id);
        d.line = line;
        d
    }

    fn count(d: EntityDraw) -> usize {
        build_all(&[d], &Skins::flat(), |_| [1.0, 1.0])
            .vertices
            .len()
    }

    #[test]
    fn the_bobber_line_adds_geometry_and_the_arrow_is_a_model() {
        let bare = count(draw(FISHING_BOBBER, None));
        assert!(bare > 0);
        assert!(count(draw(FISHING_BOBBER, Some(([5.0, 66.0, 5.0], 0.0)))) > bare);
        assert!(count(draw(ARROW, None)) > count(draw(SNOWBALL, None)));
    }

    #[test]
    fn the_first_person_rod_tip_sits_to_the_right_of_the_eye() {
        let t = Thrower {
            pos: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            body_yaw: 0.0,
            eye_height: 1.62,
            sneaking: false,
            swing: 0.0,
            first_person: true,
        };
        let (tip, _) = t.rod_tip();
        assert!(
            (tip[0] + 0.36).abs() < 1e-6 && (tip[2] - 0.35).abs() < 1e-6,
            "{tip:?}"
        );
    }
}
