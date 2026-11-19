use super::*;
use crate::mesh::{face_uv, CORNERS};

fn bake(model: &str, spec: &str) -> Vec<Quad> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("fc-models-{}-{n}", std::process::id()));
    let models = dir.join("assets/minecraft/models/block");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(models.join("m.json"), model).unwrap();
    let mut source = AssetSource::Dir(dir.clone());
    let mut loader = Loader {
        source: &mut source,
        models: HashMap::new(),
        names: Vec::new(),
    };
    let spec = serde_json::from_str(spec).unwrap();
    let quads = loader.bake(&spec);
    std::fs::remove_dir_all(dir).ok();
    quads
}

fn sorted(mut v: Vec<([i32; 3], [i32; 2])>) -> Vec<([i32; 3], [i32; 2])> {
    v.sort();
    v
}

fn q(v: f32) -> i32 {
    (v * 1024.0).round() as i32
}

/// A full cube with no `uv` must sample what the cube mesher samples (`face_uv` over `CORNERS`).
#[test]
fn default_uv_of_a_cube_matches_the_cube_faces() {
    let faces = ["up", "down", "north", "south", "west", "east"]
        .map(|f| format!(r##""{f}": {{"texture": "#t"}}"##))
        .join(",");
    let json = format!(
        r##"{{"textures": {{"t": "blocks/stone"}}, "elements": [{{"from": [0,0,0], "to": [16,16,16], "faces": {{{faces}}}}}]}}"##
    );
    let quads = bake(&json, r#"{"model": "m"}"#);
    assert_eq!(quads.len(), 6);
    for (f, quad) in quads.iter().enumerate() {
        let got = sorted(
            (0..4)
                .map(|i| (quad.pos[i].map(q), quad.uv[i].map(q)))
                .collect(),
        );
        let want = sorted(
            CORNERS[f]
                .iter()
                .map(|c| (c.map(q), face_uv(f, *c).map(q)))
                .collect(),
        );
        assert_eq!(got, want, "face {f}");
    }
}

/// `uv` is taken from the JSON, `rotation` shifts the corners, and a blockstate `y` turns the face.
#[test]
fn explicit_uv_rotation_and_blockstate_turn_are_kept() {
    let json = r##"{"textures": {"t": "blocks/stone"}, "elements": [{"from": [4,2,11], "to": [6,7,13],
        "faces": {"north": {"uv": [7,6,9,11], "texture": "#t"},
                  "up": {"uv": [0,0,8,8], "rotation": 90, "texture": "#t", "cullface": "up"}}}]}"##;
    let quads = bake(json, r#"{"model": "m", "y": 90}"#);
    let north = quads
        .iter()
        .find(|q| q.tex.as_ref() == "stone" && q.cull.is_none())
        .unwrap();
    // North turned a quarter clockwise seen from above is east.
    assert_eq!(north.face, 5);
    let us: Vec<f32> = north.uv.iter().map(|uv| uv[0] * 16.0).collect();
    let vs: Vec<f32> = north.uv.iter().map(|uv| uv[1] * 16.0).collect();
    assert_eq!(
        (
            us.iter().cloned().fold(99.0, f32::min),
            us.iter().cloned().fold(0.0, f32::max)
        ),
        (7.0, 9.0)
    );
    assert_eq!(
        (
            vs.iter().cloned().fold(99.0, f32::min),
            vs.iter().cloned().fold(0.0, f32::max)
        ),
        (6.0, 11.0)
    );
    // Up face, rotation 90: vertex 0 takes uv corner 1 (u0, v1 -> index 1 gives u0, v3).
    let up = quads.iter().find(|q| q.cull.is_some()).unwrap();
    assert_eq!(up.face, 0);
    assert_eq!(up.uv[0], [0.0, 0.5]);
}

/// Issue #547 oracle: a cocoa pod side samples [11,4,15,9].
#[test]
fn explicit_uv_of_cocoa_side_is_kept() {
    let cocoa = r##"{"textures": {"t": "blocks/cocoa_stage_2"}, "elements": [{"from": [6,2,1], "to": [10,9,5],
        "faces": {"north": {"uv": [11,4,15,9], "texture": "#t"}}}]}"##;
    let quads = bake(cocoa, r#"{"model": "m"}"#);
    assert!(has_uv(&quads, "cocoa_stage_2", [11.0, 4.0, 15.0, 9.0]));
}

/// Vanilla `BlockFaceUV`: vertex `i` takes uv corner `(i + rotation / 90) % 4` of (u1,v1) (u1,v2) (u2,v2) (u2,v1);
/// a blockstate `y` turns the face but never the uv. Covers the repeater torch, a mirrored door face and a rotated piston shaft.
#[test]
fn repeater_door_and_piston_faces_follow_vanilla_uv_rules() {
    let face = |tex: &str, uv: &str, rot: u16| {
        format!(
            r##"{{"textures": {{"t": "blocks/{tex}"}}, "elements": [{{"from": [0,0,0], "to": [4,12,4],
            "faces": {{"north": {{"uv": {uv}, "rotation": {rot}, "texture": "#t"}}}}}}]}}"##
        )
    };
    let corners = |quads: &[Quad]| quads[0].uv.map(|c| [c[0] * 16.0, c[1] * 16.0]);

    let repeater = bake(
        &face("redstone_torch_on", "[7,6,9,11]", 0),
        r#"{"model": "m"}"#,
    );
    assert_eq!(
        corners(&repeater),
        [[7.0, 6.0], [7.0, 11.0], [9.0, 11.0], [9.0, 6.0]]
    );

    let door = bake(
        &face("door_wood_lower", "[3,0,0,16]", 0),
        r#"{"model": "m", "y": 90}"#,
    );
    assert_eq!(door[0].face, 5);
    assert_eq!(
        corners(&door),
        [[3.0, 0.0], [3.0, 16.0], [0.0, 16.0], [0.0, 0.0]]
    );

    let piston = bake(&face("piston_side", "[0,0,4,12]", 270), r#"{"model": "m"}"#);
    assert_eq!(
        corners(&piston),
        [[4.0, 0.0], [0.0, 0.0], [0.0, 12.0], [4.0, 12.0]]
    );
}

/// Vanilla `ModelRotation`: y 270 turns north to west and x 270 turns north to up (three quarter turns).
#[test]
fn blockstate_turn_of_270_degrees_turns_the_face() {
    let json = r##"{"textures": {"t": "blocks/stone"}, "elements": [{"from": [0,0,0], "to": [16,16,16],
        "faces": {"north": {"texture": "#t"}}}]}"##;
    assert_eq!(bake(json, r#"{"model": "m", "y": 270}"#)[0].face, 4);
    assert_eq!(bake(json, r#"{"model": "m", "x": 270}"#)[0].face, 0);
}

/// Element `rotation` leans a wall torch toward the room: the top moves out by tan(22.5 degrees) per block.
#[test]
fn element_rotation_leans_the_wall_torch() {
    let json = r##"{"textures": {"t": "blocks/torch_on"}, "elements": [{"from": [-1,3.5,0], "to": [1,19.5,16],
        "rotation": {"origin": [0,3.5,8], "axis": "z", "angle": -22.5},
        "faces": {"west": {"uv": [0,0,16,16], "texture": "#t"}}}]}"##;
    let quads = bake(json, r#"{"model": "m"}"#);
    let (bottom, top): (Vec<_>, Vec<_>) = quads[0].pos.iter().partition(|p| p[1] < 0.5);
    let x = |v: &[&[f32; 3]]| v.iter().map(|p| p[0]).sum::<f32>() / v.len() as f32;
    assert!(x(&top) - x(&bottom) > 0.3, "top leans east");
}

fn jar() -> Option<AssetSource> {
    let Some(path) = std::env::var_os("FERROCRAFT_CLIENT_JAR") else {
        eprintln!("FERROCRAFT_CLIENT_JAR not set; skipping");
        return None;
    };
    Some(
        AssetSource::open(std::path::Path::new(&path))
            .expect("FERROCRAFT_CLIENT_JAR set but unreadable"),
    )
}

fn has_uv(quads: &[Quad], tex: &str, uv: [f32; 4]) -> bool {
    quads.iter().any(|q| {
        let lo = |a: usize| q.uv.iter().map(|p| p[a]).fold(9.0, f32::min) * 16.0;
        let hi = |a: usize| q.uv.iter().map(|p| p[a]).fold(-9.0, f32::max) * 16.0;
        &*q.tex == tex && [lo(0), lo(1), hi(0), hi(1)] == uv
    })
}

/// Every state of every modelled block finds a variant in the jar, with textures and quads.
#[test]
fn jar_variants_exist_for_every_state() {
    let Some(mut source) = jar() else { return };
    let models = Models::load(&mut source);
    let nb = |_, _, _| 0u16;
    for id in [
        50u16, 75, 76, 93, 94, 149, 150, 127, 145, 34, 69, 70, 72, 147, 148, 65, 120, 131, 64, 71,
        193, 194, 195, 196, 197, 29, 33, 55, 66, 27, 28, 157,
    ] {
        for meta in 0..16u16 {
            // Meta values a block never uses (torch 0/6.., ladder 0/1, ...) may have no variant.
            let used = match id {
                65 => (2..=5).contains(&meta),
                127 | 145 => meta < 12,
                66 => meta < 10,
                27 | 28 | 157 => meta & 7 < 6,
                _ => true,
            };
            if used {
                let quads = models.quads(id << 4 | meta, &nb);
                assert!(
                    quads.is_some_and(|q| !q.is_empty()),
                    "block {id} meta {meta}"
                );
            }
        }
    }
}

/// `BlockRedstoneWire.getActualState`: a lone dust is a dot; dust beside stone climbs it only when
/// the block above the dust is not solid; a lever beside the dust connects on that side.
#[test]
fn redstone_wire_sides_follow_get_attach_position() {
    let key = |nb: &dyn Fn(i32, i32, i32) -> u16| variant(55 << 4, nb).unwrap().1;
    let wire = 55 << 4;
    let stone = 1 << 4;
    assert_eq!(
        key(&|_, _, _| 0),
        "east=none,north=none,south=none,west=none"
    );
    assert_eq!(
        key(&|x, y, z| if (x, y, z) == (1, 0, 0) { wire } else { 0 }),
        "east=side,north=none,south=none,west=none"
    );
    // Stone to the north with wire on top of it: climbs (`up`) unless a solid block covers the dust.
    let climb = |roof: u16| {
        key(&move |x, y, z| match (x, y, z) {
            (0, 0, -1) => stone,
            (0, 1, -1) => wire,
            (0, 1, 0) => roof,
            _ => 0,
        })
    };
    assert_eq!(climb(0), "east=none,north=up,south=none,west=none");
    assert_eq!(climb(stone), "east=none,north=none,south=none,west=none");
    // Wire one step down beside the dust connects only through a non-solid neighbour.
    assert_eq!(
        key(&|x, y, z| if (x, y, z) == (0, -1, 1) { wire } else { 0 }),
        "east=none,north=none,south=side,west=none"
    );
    // A repeater joins only along its axis; a lever joins from any side.
    let repeater = |facing: u16| {
        key(&move |x, y, z| {
            if (x, y, z) == (1, 0, 0) {
                93 << 4 | facing
            } else {
                0
            }
        })
    };
    assert_eq!(repeater(3), "east=side,north=none,south=none,west=none");
    assert_eq!(repeater(0), "east=none,north=none,south=none,west=none");
    assert_eq!(
        key(&|x, y, z| if (x, y, z) == (-1, 0, 0) { 69 << 4 } else { 0 }),
        "east=none,north=none,south=none,west=side"
    );
}

/// `BlockFaceUV`: vertex `i` takes corner `(i + rotation / 90) % 4` of (u1,v1) (u1,v2) (u2,v2) (u2,v1).
fn corners(uv: [f32; 4], rot: usize) -> [[f32; 2]; 4] {
    let c = [
        [uv[0], uv[1]],
        [uv[0], uv[3]],
        [uv[2], uv[3]],
        [uv[2], uv[1]],
    ];
    std::array::from_fn(|i| c[(i + rot / 90) % 4])
}

/// One face of a vanilla model JSON: texture, face index when the state is not turned, `uv` and `rotation`.
type Expect = (&'static str, Option<usize>, [f32; 4], usize);

fn drawn(quads: &[Quad], (tex, face, uv, rot): Expect) -> bool {
    let want = corners(uv, rot);
    quads.iter().any(|q| {
        &*q.tex == tex
            && face.is_none_or(|f| q.face == f)
            && q.uv.iter().zip(want).all(|(g, w)| {
                (g[0] * 16.0 - w[0]).abs() < 1e-3 && (g[1] * 16.0 - w[1]).abs() < 1e-3
            })
    })
}

fn assert_drawn(models: &Models, state: u16, nb: &dyn Fn(i32, i32, i32) -> u16, faces: &[Expect]) {
    let quads = models.quads(state, nb).unwrap();
    for f in faces {
        assert!(drawn(quads, *f), "state {state:#x}: {f:?} missing");
    }
}

fn jar_models() -> Option<Models> {
    Some(Models::load(&mut jar()?))
}

const NB: &dyn Fn(i32, i32, i32) -> u16 = &|_, _, _| 0;
const UP: usize = 0;
const DOWN: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

/// comparator_unlit.json, repeater_1tick.json, cocoa_age1_s.json and normal_torch_wall.json.
#[test]
fn jar_torch_comparator_repeater_and_cocoa_keep_vanilla_uv() {
    let Some(models) = jar_models() else { return };
    let torch = |t: &'static str| -> Vec<Expect> {
        let side = [NORTH, SOUTH, WEST, EAST].map(|f| (t, Some(f), [7.0, 6.0, 9.0, 11.0], 0));
        let mut v = side.to_vec();
        v.push((t, Some(DOWN), [7.0, 13.0, 9.0, 15.0], 0));
        v.push((t, Some(UP), [7.0, 6.0, 9.0, 8.0], 0));
        v
    };
    assert_drawn(&models, 149 << 4, NB, &torch("redstone_torch_off"));
    assert_drawn(
        &models,
        149 << 4,
        NB,
        &[("redstone_torch_off", Some(NORTH), [7.0, 6.0, 9.0, 8.0], 0)],
    );
    assert_drawn(&models, 93 << 4, NB, &torch("redstone_torch_off"));
    let slab =
        [NORTH, SOUTH, WEST, EAST].map(|f| ("stone_slab_top", Some(f), [0.0, 14.0, 16.0, 16.0], 0));
    assert_drawn(&models, 93 << 4, NB, &slab);
    assert_drawn(
        &models,
        93 << 4,
        NB,
        &[("repeater_off", Some(UP), [0.0, 0.0, 16.0, 16.0], 0)],
    );
    let cocoa =
        [NORTH, SOUTH, WEST, EAST].map(|f| ("cocoa_stage_1", Some(f), [9.0, 4.0, 15.0, 11.0], 0));
    assert_drawn(&models, 127 << 4 | 4, NB, &cocoa);
    // Wall torch facing east: the side planes sample the whole tile, the caps the torch head.
    let wall = [
        ("torch_on", Some(WEST), [0.0, 0.0, 16.0, 16.0], 0),
        ("torch_on", Some(EAST), [0.0, 0.0, 16.0, 16.0], 0),
        ("torch_on", Some(UP), [7.0, 6.0, 9.0, 8.0], 0),
        ("torch_on", Some(DOWN), [7.0, 13.0, 9.0, 15.0], 0),
    ];
    assert_drawn(&models, 50 << 4 | 1, NB, &wall);
}

/// door_bottom(_rh).json and door_top(_rh).json, closed, facing east; the hinge picks the mirrored model.
#[test]
fn jar_doors_keep_vanilla_uv() {
    let Some(models) = jar_models() else { return };
    let side = |t: &'static str, w: [f32; 4], e: [f32; 4]| -> Vec<Expect> {
        vec![
            (t, Some(NORTH), [3.0, 0.0, 0.0, 16.0], 0),
            (t, Some(SOUTH), [0.0, 0.0, 3.0, 16.0], 0),
            (t, Some(WEST), w, 0),
            (t, Some(EAST), e, 0),
        ]
    };
    let plain = ([0.0, 0.0, 16.0, 16.0], [16.0, 0.0, 0.0, 16.0]);
    let mirrored = (plain.1, plain.0);
    for (hinge, (w, e)) in [(0u16, plain), (1, mirrored)] {
        let lower = move |_, dy, _| if dy > 0 { 64 << 4 | 8 | hinge } else { 64 << 4 };
        let mut faces = side("door_wood_lower", w, e);
        faces.push(("door_wood_lower", Some(DOWN), [13.0, 0.0, 16.0, 16.0], 0));
        assert_drawn(&models, 64 << 4, &lower, &faces);
        let upper = move |_, dy, _| if dy < 0 { 64 << 4 } else { 64 << 4 | 8 | hinge };
        let mut faces = side("door_wood_upper", w, e);
        faces.push(("door_wood_lower", Some(UP), [13.0, 0.0, 16.0, 16.0], 0));
        assert_drawn(&models, 64 << 4 | 8 | hinge, &upper, &faces);
    }
}

/// piston_head.json via piston_head_normal.json: platform and both shaft boxes. `m` maps a model face to the face
/// after the blockstate turn: facing east is y 90 (north to east), facing up is x 270 (north to up, south to down).
fn piston_faces(m: [usize; 6]) -> Vec<Expect> {
    let (s, p) = ("piston_side", "piston_top_normal");
    let (up, down, north, south, west, east) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let mut v = vec![
        (s, Some(down), [0.0, 0.0, 16.0, 4.0], 180),
        (s, Some(up), [0.0, 0.0, 16.0, 4.0], 0),
        (s, Some(west), [0.0, 0.0, 16.0, 4.0], 270),
        (s, Some(east), [0.0, 0.0, 16.0, 4.0], 90),
        (p, Some(north), [0.0, 0.0, 16.0, 16.0], 0),
        (p, Some(south), [0.0, 0.0, 16.0, 16.0], 0),
    ];
    for (a, b) in [
        ([4.0, 0.0, 16.0, 4.0], [16.0, 4.0, 4.0, 0.0]),
        ([0.0, 0.0, 4.0, 4.0], [4.0, 4.0, 0.0, 0.0]),
    ] {
        v.push((s, Some(down), a, 90));
        v.push((s, Some(up), a, 270));
        v.push((s, Some(west), b, 0));
        v.push((s, Some(east), a, 0));
    }
    v
}

#[test]
fn jar_piston_head_keeps_vanilla_uv() {
    let Some(models) = jar_models() else { return };
    assert_drawn(
        &models,
        34 << 4 | 5,
        NB,
        &piston_faces([UP, DOWN, EAST, WEST, NORTH, SOUTH]),
    );
    assert_drawn(
        &models,
        34 << 4 | 1,
        NB,
        &piston_faces([SOUTH, NORTH, UP, DOWN, WEST, EAST]),
    );
}

/// lever.json / lever_off.json: cobblestone base and handle uv, unpowered and powered.
#[test]
fn jar_lever_keeps_vanilla_uv() {
    let Some(models) = jar_models() else { return };
    let c = "cobblestone";
    let faces = [
        (c, Some(DOWN), [5.0, 4.0, 11.0, 12.0], 0),
        (c, Some(UP), [5.0, 4.0, 11.0, 12.0], 0),
        (c, Some(NORTH), [5.0, 0.0, 11.0, 3.0], 0),
        (c, Some(SOUTH), [5.0, 0.0, 11.0, 3.0], 0),
        (c, Some(WEST), [4.0, 0.0, 12.0, 3.0], 0),
        (c, Some(EAST), [4.0, 0.0, 12.0, 3.0], 0),
        ("lever", Some(DOWN), [7.0, 6.0, 9.0, 8.0], 0),
        ("lever", Some(UP), [7.0, 6.0, 9.0, 8.0], 0),
        ("lever", Some(NORTH), [7.0, 6.0, 9.0, 16.0], 0),
        ("lever", Some(EAST), [7.0, 6.0, 9.0, 16.0], 0),
    ];
    for meta in [5u16, 13] {
        assert_drawn(&models, 69 << 4 | meta, NB, &faces);
    }
}

/// The lever handle leans 45 degrees about x through [8,1,8]: lever_off (+45) toward +z, lever (-45) toward -z.
#[test]
fn jar_lever_handle_leans_with_its_state() {
    let Some(models) = jar_models() else { return };
    let reach = |meta: u16| {
        let quads = models.quads(69 << 4 | meta, NB).unwrap();
        let handle: Vec<_> = quads.iter().filter(|q| &*q.tex == "lever").collect();
        let z = handle.iter().flat_map(|q| q.pos).map(|p| p[2] * 16.0);
        let y = handle.iter().flat_map(|q| q.pos).map(|p| p[1] * 16.0);
        (
            z.clone().fold(99.0, f32::min),
            z.fold(-9.0, f32::max),
            y.fold(-9.0, f32::max),
        )
    };
    // Tip (y 11, z 8 +- 1) turned 45 degrees about (y 1, z 8): y 1 + 7.07 + 0.71, z 8 +- (7.07 + 0.71).
    let (zmin, zmax, ymax) = reach(5);
    assert!(
        zmax > 15.5 && zmin > 6.5 && (ymax - 8.78).abs() < 0.05,
        "off: {zmin} {zmax} {ymax}"
    );
    let (zmin, zmax, ymax) = reach(13);
    assert!(
        zmin < 0.5 && zmax < 9.5 && (ymax - 8.78).abs() < 0.05,
        "on: {zmin} {zmax} {ymax}"
    );
}

/// tripwire_hook_attached.json for facing south (y 180: north and south swap, west and east swap).
#[test]
fn jar_tripwire_hook_keeps_vanilla_uv() {
    let Some(models) = jar_models() else { return };
    let (h, w, t) = ("trip_wire_source", "planks_oak", "trip_wire");
    let faces: [Expect; 24] = [
        (h, Some(DOWN), [5.0, 3.0, 11.0, 9.0], 0),
        (h, Some(UP), [5.0, 3.0, 11.0, 9.0], 0),
        (h, Some(SOUTH), [5.0, 3.0, 11.0, 4.0], 0),
        (h, Some(NORTH), [5.0, 8.0, 11.0, 9.0], 0),
        (h, Some(EAST), [5.0, 8.0, 11.0, 9.0], 0),
        (h, Some(WEST), [5.0, 3.0, 11.0, 4.0], 0),
        (h, Some(SOUTH), [7.0, 8.0, 9.0, 9.0], 0),
        (h, Some(NORTH), [7.0, 3.0, 9.0, 4.0], 0),
        (h, Some(WEST), [7.0, 8.0, 9.0, 9.0], 0),
        (h, Some(EAST), [7.0, 3.0, 9.0, 4.0], 0),
        (w, Some(UP), [7.0, 2.0, 9.0, 7.0], 0),
        (w, Some(DOWN), [7.0, 9.0, 9.0, 14.0], 0),
        (w, Some(SOUTH), [7.0, 9.0, 9.0, 11.0], 0),
        (w, Some(NORTH), [7.0, 9.0, 9.0, 11.0], 0),
        (w, Some(EAST), [2.0, 9.0, 7.0, 11.0], 0),
        (w, Some(WEST), [9.0, 9.0, 14.0, 11.0], 0),
        (w, Some(DOWN), [6.0, 14.0, 10.0, 16.0], 0),
        (w, Some(UP), [6.0, 0.0, 10.0, 2.0], 0),
        (w, Some(SOUTH), [6.0, 7.0, 10.0, 15.0], 0),
        (w, Some(NORTH), [6.0, 7.0, 10.0, 15.0], 0),
        (w, Some(EAST), [0.0, 7.0, 2.0, 15.0], 0),
        (w, Some(WEST), [14.0, 7.0, 16.0, 15.0], 0),
        (t, Some(DOWN), [0.0, 8.0, 16.0, 6.0], 90),
        (t, Some(UP), [0.0, 6.0, 16.0, 8.0], 90),
    ];
    assert_drawn(&models, 131 << 4 | 4, NB, &faces);
}

fn extent(quads: &[Quad], axis: usize) -> (f32, f32) {
    let v = quads.iter().flat_map(|q| q.pos).map(|p| p[axis] * 16.0);
    (v.clone().fold(99.0, f32::min), v.fold(-9.0, f32::max))
}

/// pressure_plate_up/down (stone, wood, light, heavy), ladder and end_portal_frame_empty/filled: boxes and uv.
#[test]
fn jar_flat_blocks_keep_vanilla_boxes_and_uv() {
    let Some(models) = jar_models() else { return };
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
    for (id, t) in [
        (70u16, "stone"),
        (72, "planks_oak"),
        (147, "gold_block"),
        (148, "iron_block"),
    ] {
        for (meta, top, edge) in [(0u16, 1.0, 15.0), (1, 0.5, 15.5)] {
            let quads = models.quads(id << 4 | meta, NB).unwrap();
            assert!(near(extent(quads, 1), (0.0, top)) && near(extent(quads, 0), (1.0, 15.0)));
            assert!(near(extent(quads, 2), (1.0, 15.0)), "plate {id} {meta}");
            let faces = [
                (t, Some(UP), [1.0, 1.0, 15.0, 15.0], 0),
                (t, Some(DOWN), [1.0, 1.0, 15.0, 15.0], 0),
                (t, Some(NORTH), [1.0, edge, 15.0, 16.0], 0),
                (t, Some(EAST), [1.0, edge, 15.0, 16.0], 0),
            ];
            assert_drawn(&models, id << 4 | meta, NB, &faces);
        }
    }
    let ladder = models.quads(65 << 4 | 2, NB).unwrap();
    assert!(near(extent(ladder, 2), (15.2, 15.2)) && near(extent(ladder, 0), (0.0, 16.0)));
    assert_drawn(
        &models,
        65 << 4 | 2,
        NB,
        &[("ladder", None, [0.0, 0.0, 16.0, 16.0], 0)],
    );
    let frame = models.quads(120 << 4, NB).unwrap();
    assert!(near(extent(frame, 1), (0.0, 13.0)) && near(extent(frame, 0), (0.0, 16.0)));
    let empty = [
        ("end_stone", Some(DOWN), [0.0, 0.0, 16.0, 16.0], 0),
        ("endframe_top", Some(UP), [0.0, 0.0, 16.0, 16.0], 0),
        ("endframe_side", Some(NORTH), [0.0, 3.0, 16.0, 16.0], 0),
    ];
    assert_drawn(&models, 120 << 4, NB, &empty);
    let eye = models.quads(120 << 4 | 4, NB).unwrap();
    assert!(near(extent(eye, 1), (0.0, 16.0)) && near(extent(eye, 0), (0.0, 16.0)));
    let faces = [
        ("endframe_eye", Some(UP), [4.0, 4.0, 12.0, 12.0], 0),
        ("endframe_eye", Some(DOWN), [4.0, 4.0, 12.0, 12.0], 0),
        ("endframe_eye", None, [4.0, 0.0, 12.0, 3.0], 0),
    ];
    assert_drawn(&models, 120 << 4 | 4, NB, &faces);
}

/// cactus, vine (plane at 15.2, top layer), beacon (glass, obsidian, core) and potted plants from the jar models.
#[test]
fn jar_cactus_vine_beacon_and_pot_keep_vanilla_boxes() {
    let Some(models) = jar_models() else { return };
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
    let cactus = models.quads(81 << 4, NB).unwrap();
    assert_eq!(cactus.len(), 6);
    let side = |q: &&Quad| q.face >= 2;
    let z: Vec<_> = cactus
        .iter()
        .filter(side)
        .map(|q| q.pos[0][2] * 16.0)
        .collect();
    assert!(z.iter().any(|v| (v - 1.0).abs() < 1e-3) && z.iter().any(|v| (v - 15.0).abs() < 1e-3));
    assert!(near(extent(&[cactus[0].clone()], 0), (0.0, 16.0)));

    // Meta 1 is the south side; a full block above adds the top layer (`vine_1u`).
    let open = models.quads(106 << 4 | 1, NB).unwrap();
    assert!(near(extent(open, 2), (15.2, 15.2)) && open.iter().all(|q| q.tint && !q.shade));
    let roofed = models
        .quads(106 << 4 | 1, &|_, dy, _| if dy == 1 { 1 << 4 } else { 0 })
        .unwrap();
    let flat_top = |q: &Quad| q.pos.iter().all(|p| (p[1] * 16.0 - 15.2).abs() < 1e-3);
    assert!(!open.iter().any(flat_top) && roofed.iter().any(flat_top));

    let beacon = models.quads(138 << 4, NB).unwrap();
    assert_eq!(beacon.len(), 18);
    let core: Vec<_> = beacon
        .iter()
        .filter(|q| &*q.tex == "beacon")
        .cloned()
        .collect();
    assert!(near(extent(&core, 1), (3.0, 14.0)) && near(extent(&core, 0), (3.0, 13.0)));
    let base: Vec<_> = beacon
        .iter()
        .filter(|q| &*q.tex == "obsidian")
        .cloned()
        .collect();
    assert!(near(extent(&base, 1), (0.1, 3.0)) && near(extent(&base, 0), (2.0, 14.0)));

    let rose = models
        .quads(crate::blocks::flower_pot_state(38, 0), NB)
        .unwrap();
    assert!(
        rose.iter().any(|q| &*q.tex == "flower_rose") && rose.iter().any(|q| &*q.tex == "dirt")
    );
    let daisy = models
        .quads(crate::blocks::flower_pot_state(38, 8), NB)
        .unwrap();
    assert!(daisy.iter().any(|q| &*q.tex == "flower_oxeye_daisy"));
    let empty = models.quads(140 << 4, NB).unwrap();
    assert!(empty.iter().all(|q| &*q.tex != "flower_rose"));
}

/// A world with `(dx, dz) -> state` entries on the row `y = 0`; air elsewhere.
fn world(blocks: &[((i32, i32), u16)]) -> impl Fn(i32, i32, i32) -> u16 + '_ {
    move |dx, dy, dz| {
        blocks
            .iter()
            .find(|((x, z), _)| dy == 0 && (*x, *z) == (dx, dz))
            .map_or(0, |(_, s)| *s)
    }
}

fn key(state: u16, nb: &dyn Fn(i32, i32, i32) -> u16) -> String {
    super::variants::variant(state, nb).unwrap().1
}

/// `BlockStairs.getActualState` (`func_176307_f` outer, `func_176305_g` inner), read from the 1.8.9 bytecode.
#[test]
fn stairs_corners_follow_the_neighbours() {
    // Oak stairs: meta 0 east, 1 west, 2 south, 3 north; +4 is upside down.
    let (east, west, south, north) = (53 << 4, 53 << 4 | 1, 53 << 4 | 2, 53 << 4 | 3);
    let at = |own: u16, nbs: &[((i32, i32), u16)]| key(own, &world(nbs));
    assert_eq!(at(east, &[]), "facing=east,half=bottom,shape=straight");
    // A north-facing stair in front turns an east stair into an outer corner; the left variant when bottom.
    assert_eq!(
        at(east, &[((1, 0), north)]),
        "facing=east,half=bottom,shape=outer_left"
    );
    assert_eq!(
        at(east, &[((1, 0), south)]),
        "facing=east,half=bottom,shape=outer_right"
    );
    assert_eq!(
        at(east | 4, &[((1, 0), north | 4)]),
        "facing=east,half=top,shape=outer_right"
    );
    // Behind: an inner corner.
    assert_eq!(
        at(east, &[((-1, 0), north)]),
        "facing=east,half=bottom,shape=inner_left"
    );
    assert_eq!(
        at(west, &[((1, 0), south)]),
        "facing=west,half=bottom,shape=inner_left"
    );
    // The other half does not join; an equal stair beside the corner cancels it.
    assert_eq!(
        at(east, &[((1, 0), north | 4)]),
        "facing=east,half=bottom,shape=straight"
    );
    assert_eq!(
        at(east, &[((1, 0), north), ((0, 1), east)]),
        "facing=east,half=bottom,shape=straight"
    );
}

/// `BlockWall.getActualState` and `BlockFenceGate.getActualState`, and the bed part.
#[test]
fn wall_gate_and_bed_keys() {
    let wall = 139 << 4;
    let blocks = [((0, -1), wall), ((0, 1), wall)];
    let nb = world(&blocks);
    assert_eq!(
        key(wall, &nb),
        "east=false,north=true,south=true,up=false,west=false"
    );
    let covered = |dx, dy, dz| if dy == 1 { 1 << 4 } else { nb(dx, dy, dz) };
    assert!(key(wall, &covered).contains("up=true"));
    // A gate facing south has its hinges on the x axis: only a wall at west or east lowers it.
    let gate = 107 << 4;
    assert!(key(gate, &world(&[((1, 0), wall)])).contains("in_wall=true"));
    assert!(key(gate, &world(&[((0, 1), wall)])).contains("in_wall=false"));
}

/// The raised quarters of a bottom stair, read from the jar's real stair models: the x/z span of every up face at y = 1.
fn raised_tops(models: &Models, state: u16, nb: &dyn Fn(i32, i32, i32) -> u16) -> Vec<[f32; 4]> {
    let quads = models.quads(state, nb).expect("stair model");
    quads
        .iter()
        .filter(|q| q.face == UP && q.pos.iter().all(|p| p[1] == 1.0))
        .map(|q| {
            let f =
                |i: usize, m: fn(f32, f32) -> f32| q.pos.iter().map(|p| p[i]).fold(q.pos[0][i], m);
            [
                f(0, f32::min),
                f(0, f32::max),
                f(2, f32::min),
                f(2, f32::max),
            ]
        })
        .collect()
}

/// Facing east, a north stair at +x raises the north-east quarter (z grows south); one behind raises north-west too.
#[test]
fn stair_corner_models_raise_the_vanilla_quarter() {
    let Some(models) = jar_models() else { return };
    let (east, north) = (53 << 4, 53 << 4 | 3);
    let outer = raised_tops(&models, east, &world(&[((1, 0), north)]));
    assert!(outer.contains(&[0.5, 1.0, 0.0, 0.5]), "{outer:?}");
    assert!(!outer.contains(&[0.5, 1.0, 0.5, 1.0]), "{outer:?}");
    let inner = raised_tops(&models, east, &world(&[((-1, 0), north)]));
    // The inner model raises the whole north half (one element) plus the south-east quarter; the south-west stays low.
    assert!(inner.contains(&[0.0, 1.0, 0.0, 0.5]), "{inner:?}");
    assert!(inner.contains(&[0.5, 1.0, 0.5, 1.0]), "{inner:?}");
    assert!(!inner.contains(&[0.0, 0.5, 0.5, 1.0]), "{inner:?}");
}

/// Vanilla blockstates/bed.json lists only facing and part: every bed meta must find a drawn variant.
#[test]
fn every_bed_state_has_quads() {
    let Some(models) = jar_models() else { return };
    for meta in 0..16u16 {
        let quads = models.quads(26 << 4 | meta, NB);
        assert!(quads.is_some_and(|q| !q.is_empty()), "bed meta {meta}");
    }
}

/// Vanilla `BlockWall.canConnectTo` rejects `Material.gourd` (melon) but joins other full cubes.
#[test]
fn wall_ignores_melon_but_joins_stone() {
    let wall = 139 << 4;
    let melon = key(wall, &world(&[((0, 1), 103 << 4)]));
    assert!(melon.contains("south=false"), "{melon}");
    let stone = key(wall, &world(&[((0, 1), 1 << 4)]));
    assert!(stone.contains("south=true"), "{stone}");
}

/// `uvlock`: the quad takes the uv of its turned position, not the uv of the model face.
#[test]
fn uvlock_uses_the_turned_position() {
    let Some(models) = jar_models() else { return };
    // Stairs facing west are the east model turned 180 degrees with uvlock: the top step keeps the unturned wood grain.
    let quads = models.quads(53 << 4 | 1, NB).unwrap();
    let up = quads.iter().filter(|q| q.face == UP);
    for q in up {
        for (p, uv) in q.pos.iter().zip(q.uv) {
            assert!((uv[0] - p[0]).abs() < 1e-4 && (uv[1] - p[2]).abs() < 1e-4);
        }
    }
}
