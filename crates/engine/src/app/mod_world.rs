//! Draws in the world that mods changed: name tags and the TNT countdown. Once per frame the engine sends the
//! entities in view to the mod host (`ferrocraft_modhost::anchor`), and draws what comes back as text and plates over
//! the world. A role is asked only when the hook table says a mod changed the vanilla method behind it; the vanilla
//! name tags stay when no mod did, or when the host does not answer in time.

use crate::game::Game;
use ferrocraft_modhost::anchor::{
    Class as SubjectClass, Op, Request, Subject, World, MAX_SUBJECTS, ROLE_COUNTDOWN, ROLE_LABEL,
};
use ferrocraft_modhost::hooks::Hooks;
use ferrocraft_modhost::ModHost;
use render::{Camera, Class, EntityDraw};
use std::collections::HashMap;

/// Vanilla's range for a name tag, and what this module uses for the other roles.
const RANGE: f64 = 64.0;
/// `PrimedTntEntity.fuseTimer` at spawn; the 1.8.9 server never sends it.
const TNT_FUSE: i32 = 80;
const TNT_OBJECT: i8 = 50;

const RENDER: &str = "net/minecraft/client/renderer/entity/Render";
const RENDER_TNT: &str = "net/minecraft/client/renderer/entity/RenderTNTPrimed";
const ENTITY_DESC: &str = "(Lnet/minecraft/entity/Entity;";

/// What the mods drew for the last frame.
#[derive(Default)]
pub(super) struct Drawn {
    world: World,
    /// Where each entity of the request stood: the origin of its labels.
    feet: HashMap<i32, [f64; 3]>,
    /// The mods own the name tags in this frame, so the vanilla ones are not drawn.
    pub labels_owned: bool,
}

/// The roles that a mod changed in the hook table.
fn roles(h: &Hooks) -> u8 {
    let label = h.changed(
        RENDER,
        "func_147906_a",
        &format!("{ENTITY_DESC}Ljava/lang/String;DDDI)V"),
    );
    let countdown = [
        format!("{ENTITY_DESC}DDDFF)V"),
        "(Lnet/minecraft/entity/item/EntityTNTPrimed;DDDFF)V".to_owned(),
    ]
    .iter()
    .any(|d| h.changed(RENDER_TNT, "func_76986_a", d));
    u8::from(label) * ROLE_LABEL + u8::from(countdown) * ROLE_COUNTDOWN
}

impl Drawn {
    /// Ask the mods what they draw for the entities in `draws`, which pair with `game.entities`.
    pub(super) fn ask(
        mods: Option<&ModHost>,
        game: &Game,
        cam: &Camera,
        draws: &[EntityDraw],
        frac: f64,
        (w, h): (u32, u32),
    ) -> Drawn {
        let Some(mods) = mods.filter(|_| w > 0 && h > 0) else {
            return Drawn::default();
        };
        let owned = roles(&mods.hooks());
        if owned == 0 {
            return Drawn::default();
        }
        let vp = render::view_proj(cam, w as f32 / h as f32);
        let mut subjects: Vec<(f64, Subject)> = game
            .entities
            .iter()
            .zip(draws)
            .filter_map(|(e, d)| subject(owned, e.id, d, cam, &vp, (w, h)))
            .collect();
        subjects.sort_by(|a, b| a.0.total_cmp(&b.0));
        subjects.truncate(MAX_SUBJECTS);
        if subjects.is_empty() {
            return Drawn::default();
        }
        let feet = subjects.iter().map(|(_, s)| (s.id, s.pos)).collect();
        let request = Request {
            partial: frac as f32,
            view: [cam.yaw, cam.pitch],
            camera: cam.pos,
            subjects: subjects.into_iter().map(|(_, s)| s).collect(),
        };
        match mods.anchor(request) {
            Some(world) => Drawn {
                world,
                feet,
                labels_owned: owned & ROLE_LABEL != 0,
            },
            None => Drawn::default(),
        }
    }
}

/// The request entry for one entity, with its distance, when a mod owns a role for it in range.
fn subject(
    owned: u8,
    id: i32,
    d: &EntityDraw,
    cam: &Camera,
    vp: &[[f32; 4]; 4],
    (w, h): (u32, u32),
) -> Option<(f64, Subject)> {
    let dist: f64 = (0..3)
        .map(|i| (d.pos[i] - cam.pos[i]).powi(2))
        .sum::<f64>()
        .sqrt();
    if dist >= RANGE {
        return None;
    }
    let tnt = matches!(d.class, Class::Object(TNT_OBJECT));
    let mut roles = u8::from(tnt) * (owned & ROLE_COUNTDOWN);
    let mut label = String::new();
    if owned & ROLE_LABEL != 0 {
        let tags = render::nametags(
            std::slice::from_ref(d),
            cam.pos,
            vp,
            cam.fov_deg,
            w as f32,
            h as f32,
        );
        if let Some(tag) = tags.last() {
            roles |= ROLE_LABEL;
            label = tag.text.clone();
        }
    }
    (roles != 0).then(|| {
        let class = match d.class {
            Class::Player { .. } => SubjectClass::Player,
            _ if tnt => SubjectClass::PrimedTnt,
            _ => SubjectClass::Other,
        };
        let fuse = if tnt {
            TNT_FUSE - d.age.floor() as i32
        } else {
            0
        };
        let s = Subject {
            id,
            class,
            roles,
            pos: d.pos,
            size: d.size,
            yaw: d.yaw,
            label_dy: 0.0,
            fuse,
            label,
        };
        (dist, s)
    })
}

/// Text and plates over the world. A label hangs from its entity's feet and faces the camera: one block is
/// `px_per_block / depth` pixels, as for `draw_name_tags`.
pub(super) fn draw_labels(
    g: &mut gui::Gui,
    drawn: &Drawn,
    cam: &Camera,
    (w, h): (u32, u32),
    factor: f32,
) {
    let vp = render::view_proj(cam, w as f32 / h as f32);
    let px_per_block = h as f32 * 0.5 / (cam.fov_deg.to_radians() * 0.5).tan();
    for label in &drawn.world.labels {
        let Some(feet) = drawn.feet.get(&label.id) else {
            continue;
        };
        let p = feet.map(|v| v as f32);
        let c: Vec<f32> = (0..4)
            .map(|r| vp[0][r] * p[0] + vp[1][r] * p[1] + vp[2][r] * p[2] + vp[3][r])
            .collect();
        if c[3] <= 0.0 {
            continue;
        }
        let base = [
            (c[0] / c[3] * 0.5 + 0.5) * w as f32,
            (0.5 - c[1] / c[3] * 0.5) * h as f32,
        ];
        let k = px_per_block / c[3];
        let screen =
            |right: f32, up: f32| [(base[0] + right * k) / factor, (base[1] - up * k) / factor];
        for op in &label.ops {
            match op {
                Op::Quad { pts, argb } => {
                    let s = pts.map(|p| screen(p[0], p[1]));
                    let min = |i: usize| s.iter().map(|p| p[i]).fold(f32::MAX, f32::min);
                    let max = |i: usize| s.iter().map(|p| p[i]).fold(f32::MIN, f32::max);
                    g.fill(
                        min(0) as i32,
                        min(1) as i32,
                        max(0).ceil() as i32,
                        max(1).ceil() as i32,
                        *argb,
                    );
                }
                Op::Text {
                    right,
                    up,
                    scale,
                    argb,
                    shadow,
                    text,
                } => {
                    let at = screen(*right, *up);
                    g.push();
                    g.translate(at[0], at[1]);
                    g.scale(scale * k / factor, scale * k / factor);
                    if *shadow {
                        g.text_plain(
                            text,
                            1.0,
                            1.0,
                            (argb & 0xFF00_0000) | ((argb & 0x00FC_FCFC) >> 2),
                        );
                    }
                    g.text_plain(text, 0.0, 0.0, *argb);
                    g.pop();
                }
            }
        }
    }
}
