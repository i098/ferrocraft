use super::*;
use crate::mesh::{face_uv, CORNERS};

fn bake(model: &str, spec: &str) -> Vec<Quad> {
    let dir =
        std::env::temp_dir().join(format!("fc-models-{}-{}", std::process::id(), model.len()));
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

/// Issue #547 oracles: wall torch side samples v 6..16 and a cocoa pod side samples [11,4,15,9].
#[test]
fn explicit_uv_of_wall_torch_and_cocoa_sides_is_kept() {
    let torch = r##"{"textures": {"t": "blocks/torch_on"}, "elements": [{"from": [-1,3.5,7], "to": [1,19.5,9],
        "faces": {"west": {"uv": [7,6,9,16], "texture": "#t"}}}]}"##;
    let quads = bake(torch, r#"{"model": "m"}"#);
    assert!(has_uv(&quads, "torch_on", [7.0, 6.0, 9.0, 16.0]));
    let cocoa = r##"{"textures": {"t": "blocks/cocoa_stage_2"}, "elements": [{"from": [6,2,1], "to": [10,9,5],
        "faces": {"north": {"uv": [11,4,15,9], "texture": "#t"}}}]}"##;
    let quads = bake(cocoa, r#"{"model": "m"}"#);
    assert!(has_uv(&quads, "cocoa_stage_2", [11.0, 4.0, 15.0, 9.0]));
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
    AssetSource::open(std::path::Path::new(&path))
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
        193, 194, 195, 196, 197,
    ] {
        for meta in 0..16u16 {
            // Meta values a block never uses (torch 0/6.., ladder 0/1, ...) may have no variant.
            let used = match id {
                65 => (2..=5).contains(&meta),
                127 | 145 => meta < 12,
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

#[test]
fn jar_models_keep_vanilla_uv() {
    let Some(mut source) = jar() else { return };
    let models = Models::load(&mut source);
    let nb = |_, _, _| 0u16;
    // Comparator rear torch: vanilla uv [7,6,9,11] on every side (the cube default would be [10,9,12,14]).
    let comparator = models.quads(149 << 4, &nb).unwrap();
    assert!(has_uv(
        comparator,
        "redstone_torch_off",
        [7.0, 6.0, 9.0, 11.0]
    ));
    // Cocoa age 1 pod side: vanilla [9,4,15,11] (south facing, no turn).
    let cocoa = models.quads(127 << 4 | 4, &nb).unwrap();
    assert!(has_uv(cocoa, "cocoa_stage_1", [9.0, 4.0, 15.0, 11.0]));
    // Wall torch (facing east): the side planes cover the whole 16 px column.
    let torch = models.quads(50 << 4 | 1, &nb).unwrap();
    assert!(has_uv(torch, "torch_on", [0.0, 0.0, 16.0, 16.0]));
    // Upper door half draws the upper texture; the hinge comes from the upper half.
    let upper = |meta: u16| move |_, dy, _| if dy < 0 { 64 << 4 } else { 64 << 4 | meta };
    let left = models.quads(64 << 4 | 8, &upper(8)).unwrap();
    let right = models.quads(64 << 4 | 8 | 1, &upper(9)).unwrap();
    // The cap (up face) uses the lower texture in the vanilla model; every side uses the upper one.
    assert!(left
        .iter()
        .filter(|q| q.face != 0)
        .all(|q| &*q.tex == "door_wood_upper"));
    assert_ne!(left, right, "hinge side mirrors the door");
    // Piston head platform and shaft come from the model.
    assert!(models.quads(34 << 4 | 2, &nb).unwrap().len() >= 6);
}
