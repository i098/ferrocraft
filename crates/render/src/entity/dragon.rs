//! The ender dragon (mob 63), ported from `ModelDragon` (`EnderDragonModel`: the box tree and
//! `render`, which poses the neck and tail chains from `getMovementOffsets`) and
//! `RenderDragon.rotateCorpse`. The eye layer `LayerEnderDragonEyes` is in [`super::mobs::layers`].
//!
//! The client does not keep the dragon's ring buffer of past yaws and heights or its wing clock
//! (`animTime`) yet. [`build`] uses a steady buffer (every entry is the current yaw at one
//! height, a dragon in level flight) and `animTime` 0. A spawner dragon never ticked, so its buffer
//! is all zero and its `animTime` is 0: that draw is exact.
//!
//! Not ported: the death dissolve (`dragon_exploding.png`), the red hit overlay of `renderModel`
//! and the crystal beam; the client does not track `deathTicks` or the connected crystal.

use super::mobs::model;
use super::{EntityDraw, Model, Part, Skin};
use std::f32::consts::PI;

pub(super) const DRAGON: u8 = 63;

pub(super) fn build(d: &EntityDraw, id: u8) -> Option<Model> {
    (id == DRAGON).then(|| dragon(d))
}

/// `getMovementOffsets(i, partial)`: (yaw, height) of the body `i` ticks back.
type Offsets<'a> = &'a dyn Fn(usize) -> [f64; 2];

/// One parent `ModelRenderer` or GL transform: (translation, rotation) in model pixels.
type Parent = ([f32; 3], [f32; 3]);

/// One `addBox`: (min, size, texture offset, mirror).
type BoxSpec = ([f32; 3], [f32; 3], (f32, f32), bool);

fn dragon(d: &EntityDraw) -> Model {
    let steady = |_| [f64::from(d.yaw), 0.0];
    let mut parts = render(0.0, &steady);
    // `RenderDragon.rotateCorpse`: `translate(0, 0, 1)` after the yaw, which `Xf` applies. The
    // block becomes 16 model pixels; z keeps its sign under the `scale(-1, -1, 1)` of
    // `RendererLivingEntity`.
    // shortcut: the pitch `rotate((y5 - y10) * 10, x)` is 0 for the steady ring buffer; port it
    // with the buffer.
    for p in &mut parts {
        p.parents.push(([0.0, 0.0, 16.0], [0.0; 3]));
    }
    model(Skin::Dragon, parts)
}

/// `ModelDragon.updateRotations`: the angle wrapped into -180..180 degrees.
fn wrap(mut a: f64) -> f32 {
    while a >= 180.0 {
        a -= 360.0;
    }
    while a < -180.0 {
        a += 360.0;
    }
    a as f32
}

/// The boxes of one `ModelRenderer` at `pivot` turned by `rot`.
fn renderer(pivot: [f32; 3], rot: [f32; 3], boxes: &[BoxSpec]) -> Vec<Part> {
    boxes
        .iter()
        .map(|&(min, size, uv, mirror)| Part::new(pivot, min, size, uv).rot(rot).mirrored(mirror))
        .collect()
}

/// `ModelDragon.spine`, drawn once per neck and tail segment.
fn spine(pivot: [f32; 3], rot: [f32; 3]) -> Vec<Part> {
    renderer(
        pivot,
        rot,
        &[
            ([-5.0; 3], [10.0; 3], (192.0, 104.0), false),
            ([-1.0, -9.0, -3.0], [2.0, 4.0, 6.0], (48.0, 0.0), false),
        ],
    )
}

/// The next segment's pivot: 10 pixels along the segment's own axis, as `render` steps it.
fn step([x, y, z]: [f32; 3], [rx, ry, _]: [f32; 3]) -> [f32; 3] {
    let (rx, ry) = (f64::from(rx), f64::from(ry));
    [
        (f64::from(x) - ry.sin() * rx.cos() * 10.0) as f32,
        (f64::from(y) + rx.sin() * 10.0) as f32,
        (f64::from(z) - ry.cos() * rx.cos() * 10.0) as f32,
    ]
}

/// `GlStateManager.scale(-1, 1, 1)` of the second limb pass, folded into the part: a reflected
/// box is a mirrored `ModelBox` over the negated x span, and the y and z rotations change sign.
fn reflect(mut p: Part) -> Part {
    let flip = |(pivot, rot): Parent| -> Parent {
        ([-pivot[0], pivot[1], pivot[2]], [rot[0], -rot[1], -rot[2]])
    };
    (p.pivot, p.rot) = flip((p.pivot, p.rot));
    p.min[0] = -(p.min[0] + p.size[0]);
    p.mirror = !p.mirror;
    p.parents = p.parents.into_iter().map(flip).collect();
    p
}

/// Wing, front leg and rear leg of one side (`render`'s limb loop), with their child renderers.
fn limbs(cycle: f32, lift: f32) -> Vec<Part> {
    let wing = (
        [-12.0, 5.0, 2.0],
        [
            0.125 - f64::from(cycle).cos() as f32 * 0.2,
            0.25,
            (f64::from(cycle).sin() + 0.125) as f32 * 0.8,
        ],
    );
    let tip = (
        [-56.0, 0.0, 0.0],
        [
            0.0,
            0.0,
            -((f64::from(cycle + 2.0).sin() + 0.5) as f32) * 0.75,
        ],
    );
    let front = ([-12.0, 20.0, 2.0], [1.3 + lift * 0.1, 0.0, 0.0]);
    let front_tip = ([0.0, 20.0, -1.0], [-0.5 - lift * 0.1, 0.0, 0.0]);
    let front_foot = ([0.0, 23.0, 0.0], [0.75 + lift * 0.1, 0.0, 0.0]);
    let rear = ([-16.0, 16.0, 42.0], [1.0 + lift * 0.1, 0.0, 0.0]);
    let rear_tip = ([0.0, 32.0, -4.0], [0.5 + lift * 0.1, 0.0, 0.0]);
    let rear_foot = ([0.0, 31.0, 4.0], [0.75 + lift * 0.1, 0.0, 0.0]);
    // A child renderer's box goes through its own transform, then each parent's, nearest first.
    let under = |(pivot, rot): Parent, chain: Vec<Parent>, boxes| {
        renderer(pivot, rot, boxes)
            .into_iter()
            .map(move |p| p.under(chain.clone()))
    };
    // The skin boxes are 0 thick: only their top and bottom faces have area.
    let skin = ([-56.0, 0.0, 2.0], [56.0, 0.0, 56.0]);
    under(
        wing,
        vec![],
        &[
            ([-56.0, -4.0, -4.0], [56.0, 8.0, 8.0], (112.0, 88.0), false),
            (skin.0, skin.1, (-56.0, 88.0), false),
        ],
    )
    .chain(under(
        tip,
        vec![wing],
        &[
            ([-56.0, -2.0, -2.0], [56.0, 4.0, 4.0], (112.0, 136.0), false),
            (skin.0, skin.1, (-56.0, 144.0), false),
        ],
    ))
    .chain(under(
        front,
        vec![],
        &[([-4.0; 3], [8.0, 24.0, 8.0], (112.0, 104.0), false)],
    ))
    .chain(under(
        front_tip,
        vec![front],
        &[([-3.0, -1.0, -3.0], [6.0, 24.0, 6.0], (226.0, 138.0), false)],
    ))
    .chain(under(
        front_foot,
        vec![front_tip, front],
        &[([-4.0, 0.0, -12.0], [8.0, 4.0, 16.0], (144.0, 104.0), false)],
    ))
    .chain(under(
        rear,
        vec![],
        &[([-8.0, -4.0, -8.0], [16.0, 32.0, 16.0], (0.0, 0.0), false)],
    ))
    .chain(under(
        rear_tip,
        vec![rear],
        &[([-6.0, -2.0, 0.0], [12.0, 32.0, 12.0], (196.0, 0.0), false)],
    ))
    .chain(under(
        rear_foot,
        vec![rear_tip, rear],
        &[([-9.0, 0.0, -20.0], [18.0, 6.0, 24.0], (112.0, 0.0), false)],
    ))
    .collect()
}

/// `ModelDragon.render` for wing clock `anim` (`prevAnimTime` eased to `animTime`): every box in
/// draw order (neck, head, jaw, body, right limbs, left limbs, tail), in model pixels before the
/// renderer's transforms.
fn render(anim: f32, off: Offsets) -> Vec<Part> {
    const F5: f32 = 1.5;
    let cycle = anim * PI * 2.0;
    let jaw = (f64::from(cycle).sin() + 1.0) as f32 * 0.2;
    let lift = (f64::from(cycle - 1.0).sin() + 1.0) as f32;
    let lift = (lift * lift * 1.0 + lift * 2.0) * 0.05;
    // `translate(0, f1 - 2, -3)` in blocks, then `rotate(f1 * 2, x)`.
    let outer: Parent = (
        [0.0, (lift - 2.0) * 16.0, -48.0],
        [(lift * 2.0).to_radians(), 0.0, 0.0],
    );

    let base = off(6);
    let f6 = wrap(off(5)[0] - off(10)[0]);
    let f7 = wrap(off(5)[0] + f64::from(f6 / 2.0));
    let mut at = [0.0, 20.0, -12.0];
    let mut parts = Vec::new();
    for i in 0..5 {
        let a = off(5 - i);
        let sway = f64::from(i as f32 * 0.45 + cycle).cos() as f32 * 0.15;
        let rot = [
            sway + (a[1] - base[1]) as f32 * PI / 180.0 * F5 * 5.0,
            wrap(a[0] - base[0]) * PI / 180.0 * F5,
            -wrap(a[0] - f64::from(f7)) * PI / 180.0 * F5,
        ];
        parts.extend(spine(at, rot));
        at = step(at, rot);
    }

    let a = off(0);
    let head_rot = [
        0.0,
        wrap(a[0] - base[0]) * PI / 180.0,
        -wrap(a[0] - f64::from(f7)) * PI / 180.0,
    ];
    parts.extend(renderer(
        at,
        head_rot,
        &[
            ([-6.0, -1.0, -24.0], [12.0, 5.0, 16.0], (176.0, 44.0), false),
            ([-8.0, -8.0, -10.0], [16.0; 3], (112.0, 30.0), false),
            ([-5.0, -12.0, -4.0], [2.0, 4.0, 6.0], (0.0, 0.0), true),
            ([-5.0, -3.0, -22.0], [2.0, 2.0, 4.0], (112.0, 0.0), true),
            ([3.0, -12.0, -4.0], [2.0, 4.0, 6.0], (0.0, 0.0), false),
            ([3.0, -3.0, -22.0], [2.0, 2.0, 4.0], (112.0, 0.0), false),
        ],
    ));
    parts.push(
        Part::new(
            [0.0, 4.0, -8.0],
            [-6.0, 0.0, -16.0],
            [12.0, 4.0, 16.0],
            (176.0, 65.0),
        )
        .rot([jaw, 0.0, 0.0])
        .under(vec![(at, head_rot)]),
    );

    // `translate(0, 1, 0)`, `rotate(-f6 * f5, z)`, `translate(0, -1, 0)` around the body and limbs.
    let group: [Parent; 2] = [
        ([0.0, -16.0, 0.0], [0.0; 3]),
        ([0.0, 16.0, 0.0], [0.0, 0.0, (-f6 * F5 * 1.0).to_radians()]),
    ];
    let mut grouped = renderer(
        [0.0, 4.0, 8.0],
        [0.0; 3],
        &[
            ([-12.0, 0.0, -16.0], [24.0, 24.0, 64.0], (0.0, 0.0), false),
            ([-1.0, -6.0, -10.0], [2.0, 6.0, 12.0], (220.0, 53.0), false),
            ([-1.0, -6.0, 10.0], [2.0, 6.0, 12.0], (220.0, 53.0), false),
            ([-1.0, -6.0, 30.0], [2.0, 6.0, 12.0], (220.0, 53.0), false),
        ],
    );
    let side = limbs(cycle, lift);
    grouped.extend(side.iter().cloned());
    grouped.extend(side.into_iter().map(reflect));
    for p in &mut grouped {
        p.parents.extend(group);
    }
    parts.extend(grouped);

    let base = off(11);
    let mut bend = -(f64::from(cycle).sin() as f32) * 0.0;
    let mut at = [0.0, 10.0, 60.0];
    for k in 0..12 {
        let a = off(12 + k);
        bend = (f64::from(bend)
            + f64::from(k as f32 * 0.45 + cycle).sin() * 0.050_000_000_745_058_06)
            as f32;
        let rot = [
            bend + (a[1] - base[1]) as f32 * PI / 180.0 * F5 * 5.0,
            (wrap(a[0] - base[0]) * F5 + 180.0) * PI / 180.0,
            wrap(a[0] - f64::from(f7)) * PI / 180.0 * F5,
        ];
        parts.extend(spine(at, rot));
        at = step(at, rot);
    }
    for p in &mut parts {
        p.parents.push(outer);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model as drawn, rotate, Class, Xf};
    use super::*;

    const ORACLE: &str = include_str!("dragon_oracle.txt");

    /// The lines of the oracle section whose header starts with `title`.
    fn section(title: &str) -> Vec<&str> {
        ORACLE
            .split("== ")
            .find(|s| s.starts_with(title))
            .unwrap_or_else(|| panic!("no section {title}"))
            .lines()
            .skip(1)
            .collect()
    }

    fn numbers(line: &str) -> Vec<f32> {
        line.split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect()
    }

    /// The ring buffer offsets the oracle printed for a state.
    fn offsets(lines: &[&str]) -> Vec<[f64; 2]> {
        lines
            .iter()
            .filter(|l| l.starts_with("offsets "))
            .map(|l| {
                let n: Vec<f64> = l
                    .split_whitespace()
                    .skip(2)
                    .map(|w| w.parse().unwrap())
                    .collect();
                [n[0], n[1]]
            })
            .collect()
    }

    /// The 8 corners of a part as `emit_part` places them, before the entity transform.
    fn corners(p: &Part) -> Vec<[f32; 3]> {
        (0..8)
            .map(|i| {
                let c = [0, 1, 2].map(|a| p.min[a] + if i >> a & 1 == 1 { p.size[a] } else { 0.0 });
                let mut q = rotate(c, p.rot);
                q = [0, 1, 2].map(|a| q[a] + p.pivot[a]);
                for &(pivot, rot) in &p.parents {
                    q = rotate(q, rot);
                    q = [0, 1, 2].map(|a| q[a] + pivot[a]);
                }
                q
            })
            .collect()
    }

    /// Every oracle box corner is a corner of the matching part, in draw order.
    fn assert_matches(
        lines: &[&str],
        parts: &[Part],
        place: impl Fn([f32; 3]) -> [f32; 3],
        tol: f32,
    ) {
        let boxes: Vec<&&str> = lines
            .iter()
            .filter(|l| !l.starts_with("offsets "))
            .collect();
        assert_eq!(boxes.len(), parts.len());
        for (i, (line, part)) in boxes.iter().zip(parts).enumerate() {
            let n = numbers(line);
            let ours: Vec<[f32; 3]> = corners(part).into_iter().map(&place).collect();
            for want in n.chunks(3) {
                let near = ours
                    .iter()
                    .any(|c| (0..3).all(|a| (c[a] - want[a]).abs() < tol));
                assert!(near, "box {i} ({line}): {want:?} not in {ours:?}");
            }
        }
    }

    /// Oracle: the constructor's tree. Texture size 256x256 and, per box, the corners, the texture
    /// offset and the mirror flag; the fixed pivots of the jaw, body and limbs.
    #[test]
    fn box_layout_matches_the_oracle() {
        let lines = section("EnderDragonModel");
        assert!(ORACLE.contains("textureSize 256x256"));
        let parts = render(0.0, &|_| [0.0; 2]);
        // Draw index of each box of the tree (head, jaw, neck, body, wing, tip, legs).
        let index = [
            10, 11, 12, 13, 14, 15, 16, 0, 1, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
            30,
        ];
        let mut pivot = [0.0f32; 3];
        let mut boxes = 0;
        for line in &lines {
            let n = numbers(line);
            if line.contains(" pivot ") {
                pivot = [n[0], n[1], n[2]];
                continue;
            }
            let p = &parts[index[boxes]];
            boxes += 1;
            let size = [n[3] - n[0], n[4] - n[1], n[5] - n[2]];
            assert_eq!(
                (p.min, p.size, p.uv),
                ([n[0], n[1], n[2]], size, (n[6], n[7])),
                "{line}"
            );
            assert_eq!(p.mirror, line.ends_with("true"), "{line}");
            // The head and neck pivots move every frame; the others are fixed.
            if !line.contains("head.") && !line.contains("neck.") {
                assert_eq!(p.pivot, pivot, "{line}");
            }
        }
        assert_eq!(boxes, index.len());
    }

    /// Oracle: `render` of a never-ticked dragon (the spawner): all-zero ring buffer, `animTime` 0.
    #[test]
    fn never_ticked_pose_matches_the_oracle() {
        let lines = section("never ticked");
        assert!(offsets(&lines).iter().all(|o| *o == [0.0; 2]));
        assert_matches(&lines, &render(0.0, &|_| [0.0; 2]), |c| c, 2e-3);
    }

    /// Oracle: `render` of a turning, climbing dragon mid flap (wing clock 0.3 to 0.36 at partial
    /// tick 0.25), with the oracle's own `getMovementOffsets`.
    #[test]
    fn flight_pose_matches_the_oracle() {
        let lines = section("ring yaw");
        let off = offsets(&lines);
        let parts = render(0.3 + 0.06 * 0.25, &|i| off[i]);
        assert_matches(&lines, &parts, |c| c, 2e-3);
    }

    /// Oracle: the steady ring buffer at yaw 37 through `RenderDragon.rotateCorpse` and the
    /// `scale(-1, -1, 1)` and drop of `RendererLivingEntity` (printed in blocks times 16).
    #[test]
    fn entity_transform_matches_the_oracle() {
        let lines = section("steady ring yaw 37");
        let mut d = steve();
        (d.class, d.yaw, d.pos) = (Class::Mob(DRAGON), 37.0, [0.0; 3]);
        let xf = Xf::new(&d, false);
        let m = drawn(&d);
        assert_eq!(m.skin, Skin::Dragon);
        assert_matches(&lines, &m.parts, |c| xf.world(c).map(|v| v * 16.0), 5e-3);
    }
}
