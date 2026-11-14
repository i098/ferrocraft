//! Draws in the world that mods changed: name tags and the TNT countdown. Once per frame the engine sends the
//! entities in view to the mod host (`ferrocraft_modhost::anchor`), and draws what comes back as text and plates over
//! the world. A role is asked only when the hook table says a mod changed the vanilla method behind it; the vanilla
//! name tags stay when no mod did, or when the host does not answer in time.

use crate::game::Game;
use ferrocraft_modhost::anchor::{
    Class as SubjectClass, Op, Request, Subject, World, MAX_SUBJECTS, ROLE_COUNTDOWN, ROLE_HITBOX,
    ROLE_LABEL,
};
use ferrocraft_modhost::hooks::Hooks;
use ferrocraft_modhost::ModHost;
use render::{Camera, Class, EntityDraw};
use std::collections::{HashMap, HashSet};
use world::entities::Kind;

/// Vanilla's range for a name tag, and what this module uses for the other roles.
const RANGE: f64 = 64.0;
/// `PrimedTntEntity.fuseTimer` at spawn; the 1.8.9 server never sends it.
const TNT_FUSE: i32 = 80;
const TNT_OBJECT: i8 = 50;

const RENDER: &str = "net/minecraft/client/renderer/entity/Render";
const RENDER_MANAGER: &str = "net/minecraft/client/renderer/entity/RenderManager";
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

/// The roles that a mod changed in the hook table. The hitbox role needs F3+B on as well.
fn roles(h: &Hooks, hitboxes: bool) -> u8 {
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
    let hitbox = hitboxes
        && h.changed(
            RENDER_MANAGER,
            "func_85094_b",
            &format!("{ENTITY_DESC}DDDFF)V"),
        );
    u8::from(label) * ROLE_LABEL
        + u8::from(countdown) * ROLE_COUNTDOWN
        + u8::from(hitbox) * ROLE_HITBOX
}

type Segment = ([f64; 3], [f64; 3], u32);

const WHITE: u32 = 0xffff_ffff;
const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;

/// The 12 edges of the box `lo..hi`, as `RenderGlobal.drawOutlinedBoundingBox` strokes them.
fn box_edges(lo: [f64; 3], hi: [f64; 3], argb: u32) -> impl Iterator<Item = Segment> {
    let corner = move |c: usize| [0, 1, 2].map(|i| if c >> i & 1 == 0 { lo[i] } else { hi[i] });
    (0..8usize).flat_map(move |a| {
        (0..3)
            .filter_map(move |i| (a >> i & 1 == 0).then_some((corner(a), corner(a | 1 << i), argb)))
    })
}

/// `RenderManager.renderDebugBoundingBox` for an entity of `size` standing at `pos` (the white box, the red
/// eye-height box of a living entity, and the blue look line of length 2 from the eyes). `(lo, hi)` is
/// `getEntityBoundingBox`; the red box takes its width from it.
fn vanilla_hitbox(
    pos: [f64; 3],
    (lo, hi): ([f64; 3], [f64; 3]),
    eye: f32,
    living: bool,
    look: [f64; 3],
) -> Vec<Segment> {
    let [x, y, z] = pos;
    let mut lines: Vec<Segment> = box_edges(lo, hi, WHITE).collect();
    let eyes = y + f64::from(eye);
    if living {
        let t = f64::from(0.01f32);
        lines.extend(box_edges(
            [lo[0], eyes - t, lo[2]],
            [hi[0], eyes + t, hi[2]],
            RED,
        ));
    }
    lines.push((
        [x, eyes, z],
        [x + look[0] * 2.0, eyes + look[1] * 2.0, z + look[2] * 2.0],
        BLUE,
    ));
    lines
}

/// `Entity.getVectorForRotation`, which `getLook` uses at the head yaw of a living entity.
fn look_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let rad = 0.017_453_292_f32;
    let (yaw_rad, pitch_rad) = (-yaw * rad - std::f32::consts::PI, -pitch * rad);
    let (c, s) = (yaw_rad.cos(), yaw_rad.sin());
    let (cp, sp) = (-pitch_rad.cos(), pitch_rad.sin());
    [f64::from(s * cp), f64::from(sp), f64::from(c * cp)]
}

/// The vanilla lines of one draw. A draw past `ids` is the self model of the local player (1.62 high, 1.54 sneaking).
fn draw_hitbox(game: &Game, id: Option<i32>, d: &EntityDraw) -> Vec<Segment> {
    let (eye, living) = match id.map(|id| game.entities.get(id)) {
        None => (1.62 - if d.sneaking { 0.08 } else { 0.0 }, true),
        Some(None) => return Vec::new(),
        // `Entity.getEyeHeight` is 0 for a hanging entity; its box comes from its facing, not its size.
        Some(Some(e)) if matches!(e.kind, Kind::Painting | Kind::Object(71 | 77)) => {
            let look = look_of(false, d.yaw, d.head_yaw, d.pitch);
            return vanilla_hitbox(e.pos, e.bounds(), 0.0, false, look);
        }
        Some(Some(e)) => (
            e.eye_height(),
            matches!(e.kind, Kind::Player | Kind::Mob(_) | Kind::Object(78)),
        ),
    };
    let look = look_of(living, d.yaw, d.head_yaw, d.pitch);
    let half = f64::from(d.size.0 / 2.0);
    let [x, y, z] = d.pos;
    let bounds = (
        [x - half, y, z - half],
        [x + half, y + f64::from(d.size.1), z + half],
    );
    vanilla_hitbox(d.pos, bounds, eye, living, look)
}

/// `EntityLivingBase.getLook` aims by the head yaw; `Entity.getLook` by the body yaw.
fn look_of(living: bool, yaw: f32, head_yaw: f32, pitch: f32) -> [f64; 3] {
    look_vector(if living { head_yaw } else { yaw }, pitch)
}

impl Drawn {
    /// What F3+B draws: for every entity in view that is not invisible, the lines of the mods when a mod changed
    /// the vanilla method and answered for it, else the vanilla ones.
    pub(super) fn hitbox_lines(
        &self,
        game: &Game,
        ids: &[i32],
        draws: &[EntityDraw],
        shown: bool,
    ) -> Vec<Segment> {
        if !shown {
            return Vec::new();
        }
        let modded: HashSet<i32> = self.world.lines.iter().map(|l| l.id).collect();
        let vanilla = draws.iter().enumerate().flat_map(|(i, d)| {
            let id = ids.get(i).copied();
            let skip = d.invisible || id.is_some_and(|id| modded.contains(&id));
            if skip {
                Vec::new()
            } else {
                draw_hitbox(game, id, d)
            }
        });
        self.mod_lines().into_iter().chain(vanilla).collect()
    }

    /// The hitbox lines in world space, one `(from, to, argb)` per segment.
    fn mod_lines(&self) -> Vec<Segment> {
        let at = |feet: &[f64; 3], p: [f32; 3]| [0, 1, 2].map(|i| feet[i] + f64::from(p[i]));
        self.world
            .lines
            .iter()
            .filter_map(|l| Some((self.feet.get(&l.id)?, l)))
            .flat_map(|(feet, l)| {
                l.segments
                    .iter()
                    .map(move |s| (at(feet, s.from), at(feet, s.to), s.argb))
            })
            .collect()
    }

    /// Ask the mods what they draw for the entities in `draws`, which pair with `game.entities`.
    pub(super) fn ask(
        mods: Option<&ModHost>,
        game: &Game,
        cam: &Camera,
        draws: &[EntityDraw],
        frac: f64,
        (w, h): (u32, u32),
        hitboxes: bool,
    ) -> Drawn {
        let Some(mods) = mods.filter(|_| w > 0 && h > 0) else {
            return Drawn::default();
        };
        let owned = roles(&mods.hooks(), hitboxes);
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
    // `RenderManager.doRenderEntity`: vanilla draws the box of an entity that is not invisible.
    if owned & ROLE_HITBOX != 0 && !d.invisible {
        roles |= ROLE_HITBOX;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn span(lines: &[Segment], argb: u32) -> [(f64, f64); 3] {
        let pts: Vec<[f64; 3]> = lines
            .iter()
            .filter(|l| l.2 == argb)
            .flat_map(|l| [l.0, l.1])
            .collect();
        [0, 1, 2].map(|i| {
            let lo = pts.iter().map(|p| p[i]).fold(f64::MAX, f64::min);
            let hi = pts.iter().map(|p| p[i]).fold(f64::MIN, f64::max);
            (lo, hi)
        })
    }

    fn assert_span(got: [(f64, f64); 3], want: [(f64, f64); 3]) {
        for (g, w) in got.iter().zip(want) {
            assert!(near(g.0, w.0) && near(g.1, w.1), "{got:?} != {want:?}");
        }
    }

    /// Oracle: the arithmetic of 1.8.9 `RenderManager.renderDebugBoundingBox` for a player at (1, 64, 2):
    /// width 0.6, height 1.8, eye height 1.62, looking down +z (yaw 0, pitch 0).
    #[test]
    fn a_player_gets_white_and_red_boxes_and_a_blue_look_line() {
        let look = look_vector(0.0, 0.0);
        assert_span(
            [(look[0], look[0]), (look[1], look[1]), (look[2], look[2])],
            [(0.0, 0.0), (0.0, 0.0), (1.0, 1.0)],
        );
        let lines = vanilla_hitbox(
            [1.0, 64.0, 2.0],
            ([0.7, 64.0, 1.7], [1.3, 65.8, 2.3]),
            1.62,
            true,
            look,
        );
        assert_eq!(lines.len(), 12 + 12 + 1);
        assert_span(span(&lines, WHITE), [(0.7, 1.3), (64.0, 65.8), (1.7, 2.3)]);
        assert_span(span(&lines, RED), [(0.7, 1.3), (65.61, 65.63), (1.7, 2.3)]);
        let blue = lines.last().unwrap();
        assert_eq!(blue.2, BLUE);
        assert_span(
            [(blue.0[1], blue.1[1]), (blue.0[2], blue.1[2]), (0.0, 0.0)],
            [(65.62, 65.62), (2.0, 4.0), (0.0, 0.0)],
        );
    }

    /// Oracle: `getVectorForRotation(-30, 90)` worked by hand: x = sin(-270 deg) * -cos(30 deg) = -0.866,
    /// y = sin(30 deg) = 0.5, z = cos(-270 deg) * -cos(30 deg) = 0. A flipped yaw or pitch sign changes it.
    #[test]
    fn the_look_follows_yaw_and_pitch_and_the_head_only_for_living_entities() {
        let look = look_of(true, 0.0, 90.0, -30.0);
        assert_span(
            [(look[0], look[0]), (look[1], look[1]), (look[2], look[2])],
            [(-0.866_025_4, -0.866_025_4), (0.5, 0.5), (0.0, 0.0)],
        );
        assert_eq!(look_of(false, 90.0, 0.0, -30.0), look);
        assert_ne!(look_of(true, 90.0, 0.0, -30.0), look);
    }

    /// A mod that changed the method and drew nothing for entity 1 hides its box; entity 2 keeps the vanilla one.
    #[test]
    fn a_mod_record_without_segments_hides_only_that_entity() {
        let mut game = Game::new();
        for id in [1, 2] {
            game.entities.apply(world::entities::EntityEvent::SpawnMob {
                id,
                kind: 54,
                pos: [0, 2048, 0],
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                vel: [0; 3],
                flags: None,
                name: None,
                health: None,
                size: Default::default(),
            });
        }
        let draws: Vec<EntityDraw> = [1, 2]
            .map(|id| {
                crate::app::entity_draw(
                    game.entities.get(id).unwrap(),
                    1.0,
                    None,
                    None,
                    false,
                    false,
                )
            })
            .into();
        let mut drawn = Drawn::default();
        drawn.world.lines.push(ferrocraft_modhost::anchor::Lines {
            id: 1,
            segments: Vec::new(),
        });
        let lines = drawn.hitbox_lines(&game, &[1, 2], &draws, true);
        assert_eq!(lines.len(), 12 + 12 + 1);
        assert!(drawn.hitbox_lines(&game, &[1, 2], &draws, false).is_empty());
    }

    #[test]
    fn a_non_living_entity_has_no_red_box() {
        let lines = vanilla_hitbox(
            [0.0; 3],
            ([-0.125, 0.0, -0.125], [0.125, 0.25, 0.125]),
            0.2,
            false,
            [0.0, 0.0, 1.0],
        );
        assert_eq!(lines.len(), 13);
        assert!(lines.iter().all(|l| l.2 != RED));
    }
}
