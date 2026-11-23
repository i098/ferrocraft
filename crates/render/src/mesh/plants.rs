//! Plant-like block models baked from the vanilla 1.8.9 model JSON (`models/block/cross.json`,
//! `crop.json` and the models built on them): elements are boxes with one or more faces, an optional
//! rescaled rotation and explicit texel UVs, exactly as `FaceBakery` bakes them.

use super::{face_uv, position_random, Mesher, Pass, Vertex, CORNERS};
use crate::ambient::flammable;
use crate::blocks::{Look, DOWN, EAST, NORTH, SOUTH, WEST};
use world::place::solid_top;

/// A face of an element: direction, UV rectangle `[u1, v1, u2, v2]` in texels, texture slot.
pub(super) type Face = (usize, [f32; 4], usize);

/// `BlockPartRotation` with `rescale`: turn about an axis through `origin`, in degrees.
#[derive(Clone, Copy)]
pub(super) struct Turn {
    pub axis: usize,
    pub origin: [f32; 3],
    pub angle: f32,
}

/// One `elements` entry of a model, in sixteenths of a block.
#[derive(Clone, Copy)]
pub(super) struct Elem<'a> {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub turn: Option<Turn>,
    pub faces: &'a [Face],
}

pub(super) const FULL: [f32; 4] = [0.0, 0.0, 16.0, 16.0];
const Y45: Option<Turn> = Some(Turn {
    axis: 1,
    origin: [8.0; 3],
    angle: 45.0,
});

/// `models/block/cross.json`: two planes crossed on the diagonals.
static CROSS: [Elem; 2] = [
    Elem {
        from: [0.8, 0.0, 8.0],
        to: [15.2, 16.0, 8.0],
        turn: Y45,
        faces: &[(NORTH, FULL, 0), (SOUTH, FULL, 0)],
    },
    Elem {
        from: [8.0, 0.0, 0.8],
        to: [8.0, 16.0, 15.2],
        turn: Y45,
        faces: &[(WEST, FULL, 0), (EAST, FULL, 0)],
    },
];

/// `models/block/crop.json`: four planes at 4 and 12 px, one pixel below the block floor.
static CROP: [Elem; 4] = [
    Elem {
        from: [4.0, -1.0, 0.0],
        to: [4.0, 15.0, 16.0],
        turn: None,
        faces: &[(WEST, FULL, 0), (EAST, FULL, 0)],
    },
    Elem {
        from: [12.0, -1.0, 0.0],
        to: [12.0, 15.0, 16.0],
        turn: None,
        faces: &[(WEST, FULL, 0), (EAST, FULL, 0)],
    },
    Elem {
        from: [0.0, -1.0, 4.0],
        to: [16.0, 15.0, 4.0],
        turn: None,
        faces: &[(NORTH, FULL, 0), (SOUTH, FULL, 0)],
    },
    Elem {
        from: [0.0, -1.0, 12.0],
        to: [16.0, 15.0, 12.0],
        turn: None,
        faces: &[(NORTH, FULL, 0), (SOUTH, FULL, 0)],
    },
];

/// `double_sunflower_top.json`: the upper half of the stem (texels 8..16) and the tilted head.
static SUNFLOWER_TOP: [Elem; 3] = [
    Elem {
        from: [0.8, 0.0, 8.0],
        to: [15.2, 8.0, 8.0],
        turn: Y45,
        faces: &[
            (NORTH, [0.0, 8.0, 16.0, 16.0], 0),
            (SOUTH, [0.0, 8.0, 16.0, 16.0], 0),
        ],
    },
    Elem {
        from: [8.0, 0.0, 0.8],
        to: [8.0, 8.0, 15.2],
        turn: Y45,
        faces: &[
            (WEST, [0.0, 8.0, 16.0, 16.0], 0),
            (EAST, [0.0, 8.0, 16.0, 16.0], 0),
        ],
    },
    Elem {
        from: [9.6, -1.0, 1.0],
        to: [9.6, 15.0, 15.0],
        turn: Some(Turn {
            axis: 2,
            origin: [8.0; 3],
            angle: 22.5,
        }),
        faces: &[(WEST, FULL, 1), (EAST, FULL, 2)],
    },
];

impl Turn {
    /// `FaceBakery.rotateVertex`: scale the two other axes by `1 / cos(angle)`, then turn.
    fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let (a, b) = match self.axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        let rad = self.angle.to_radians();
        let k = 1.0 / rad.cos();
        let (ra, rb) = ((p[a] - self.origin[a]) * k, (p[b] - self.origin[b]) * k);
        let mut out = p;
        out[a] = self.origin[a] + ra * rad.cos() - rb * rad.sin();
        out[b] = self.origin[b] + ra * rad.sin() + rb * rad.cos();
        out
    }
}

/// How a baked model is drawn: textures by slot, tint, brightness, quarter turns of the blockstate
/// `y` rotation and the block offset.
pub(super) struct Draw<'a> {
    pub tex: &'a [&'a str],
    pub rgb: [f32; 3],
    pub light: [f32; 2],
    pub shade: f32,
    pub quarter_turns: u8,
    pub offset: [f32; 3],
    /// Mirror the texels of every face (`*_flip` models).
    pub flip: bool,
}

/// `BlockModelRenderer.renderModelStandardQuads` hashes X and Z for models without ambient occlusion.
fn plant_offset(id: u16, [x, _, z]: [i32; 3]) -> [f32; 3] {
    if !matches!(id, 31 | 37 | 38 | 175) {
        return [0.0; 3];
    }
    let i = position_random(x, 0, z);
    let f = |shift: u32| ((i >> shift & 15) as f32 / 15.0 - 0.5) * 0.5;
    let dy = if id == 31 {
        ((i >> 20 & 15) as f32 / 15.0 - 1.0) * 0.2
    } else {
        0.0
    };
    [f(16), dy, f(24)]
}

impl Mesher<'_> {
    pub(super) fn cross(&mut self, at: [i32; 3], state: u16, l: &Look) {
        let (id, meta) = (state >> 4, state & 15);
        match id {
            51 => return self.fire(at),
            104 | 105 => return self.stem(at, id, meta & 7, l),
            _ => {}
        }
        let (mut tex, mut tint) = (l.tex[0], l.tint);
        let mut sunflower = false;
        if id == 175 && meta & 8 != 0 {
            let lower = self.st(at, [0, -1, 0]);
            (tex, tint) = crate::blocks::double_plant_top(lower);
            sunflower = lower & 7 == 0;
        }
        let world = [
            self.hood.origin.0 * 16 + at[0],
            at[1],
            self.hood.origin.1 * 16 + at[2],
        ];
        let draw = Draw {
            tex: &[
                tex,
                "double_plant_sunflower_back",
                "double_plant_sunflower_front",
            ],
            rgb: self.base_color(l, tint, at),
            light: self.light_at(at, [0; 3]),
            shade: 1.0,
            quarter_turns: 0,
            offset: plant_offset(id, world),
            flip: false,
        };
        self.draw_model(at, plant_model(id, sunflower), &draw);
    }

    /// Emit every face of `elems` as a one-sided quad, as the model baker does.
    pub(super) fn draw_model(&mut self, at: [i32; 3], elems: &[Elem], d: &Draw) {
        for e in elems {
            for &(dir, uv, slot) in e.faces {
                let verts = CORNERS[dir].map(|c| self.model_vertex(at, e, (dir, uv, slot), c, d));
                self.push_quad(Pass::Opaque, verts, false);
            }
        }
    }

    fn model_vertex(&self, at: [i32; 3], e: &Elem, face: Face, c: [f32; 3], d: &Draw) -> Vertex {
        let (pos, uv) = model_point(e, face, c, d);
        Vertex {
            pos: [0, 1, 2].map(|a| at[a] as f32 + pos[a]),
            uv: self.atlas.uv(d.tex[face.2], uv),
            color: [
                d.rgb[0] * d.shade,
                d.rgb[1] * d.shade,
                d.rgb[2] * d.shade,
                1.0,
            ],
            light: d.light,
        }
    }
}

/// Position (relative to the block, with the block offset) and sprite uv of corner `c` of a model face.
fn model_point(e: &Elem, (dir, uv, _): Face, c: [f32; 3], d: &Draw) -> ([f32; 3], [f32; 2]) {
    let mut p = [0, 1, 2].map(|a| if c[a] > 0.5 { e.to[a] } else { e.from[a] });
    if let Some(t) = e.turn {
        p = t.apply(p);
    }
    let p = turn_model(p.map(|v| v / 16.0), d.quarter_turns);
    let [fu, fv] = face_uv(dir, c);
    let (u1, u2) = if d.flip {
        (uv[2], uv[0])
    } else {
        (uv[0], uv[2])
    };
    let pos = [0, 1, 2].map(|a| p[a] + d.offset[a]);
    (
        pos,
        [
            (u1 + (u2 - u1) * fu) / 16.0,
            (uv[1] + (uv[3] - uv[1]) * fv) / 16.0,
        ],
    )
}

/// The quads of the cross or crop model of block `id` at block `at` (with the vanilla model offset):
/// positions relative to the block and sprite uv. Both windings, so a plane shows from either side.
pub(crate) fn cross_quads(id: u16, at: [i32; 3]) -> Vec<[([f32; 3], [f32; 2]); 4]> {
    let d = Draw {
        tex: &[],
        rgb: [1.0; 3],
        light: [0.0; 2],
        shade: 1.0,
        quarter_turns: 0,
        offset: plant_offset(id, at),
        flip: false,
    };
    let d = &d;
    plant_model(id, false)
        .iter()
        .flat_map(|e| {
            e.faces
                .iter()
                .map(move |&f| CORNERS[f.0].map(|c| model_point(e, f, c, d)))
        })
        .collect()
}

/// The model of a plain plant block: crops, the sunflower head, or the cross.
fn plant_model(id: u16, sunflower: bool) -> &'static [Elem<'static>] {
    match id {
        59 | 115 | 141 | 142 => &CROP,
        _ if sunflower => &SUNFLOWER_TOP,
        _ => &CROSS,
    }
}

/// Blockstate `y` rotation: clockwise seen from above about the block centre.
fn turn_model(p: [f32; 3], quarter_turns: u8) -> [f32; 3] {
    (0..quarter_turns).fold(p, |[x, y, z], _| [1.0 - z, y, x])
}

/// Directions of `Plane.HORIZONTAL`: north, east, south, west.
const HORIZONTAL: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

impl Mesher<'_> {
    /// `BlockStem` with `getActualState`: the growth model, or the fruit model turned towards the fruit.
    fn stem(&mut self, at: [i32; 3], id: u16, age: u16, l: &Look) {
        let crop = if id == 104 { 86 } else { 103 };
        let facing = HORIZONTAL.iter().position(|&d| self.st(at, d) >> 4 == crop);
        let (top, texel) = match facing {
            Some(_) => (7.0, 8.0),
            None => (age as f32 * 2.0 + 1.0, age as f32 * 2.0 + 2.0),
        };
        let (a, b) = (
            [
                (NORTH, [0.0, 0.0, 16.0, texel], 0),
                (SOUTH, [16.0, 0.0, 0.0, texel], 0),
            ],
            [
                (WEST, [0.0, 0.0, 16.0, texel], 0),
                (EAST, [16.0, 0.0, 0.0, texel], 0),
            ],
        );
        let upper = [
            (NORTH, [9.0, 0.0, 0.0, 16.0], 1),
            (SOUTH, [0.0, 0.0, 9.0, 16.0], 1),
        ];
        let elems = [
            Elem {
                from: [0.0, -1.0, 8.0],
                to: [16.0, top, 8.0],
                turn: Y45,
                faces: &a,
            },
            Elem {
                from: [8.0, -1.0, 0.0],
                to: [8.0, top, 16.0],
                turn: Y45,
                faces: &b,
            },
            Elem {
                from: [0.0, 0.0, 8.0],
                to: [9.0, 16.0, 8.0],
                turn: None,
                faces: &upper,
            },
        ];
        let connected = if id == 104 {
            "pumpkin_stem_connected"
        } else {
            "melon_stem_connected"
        };
        let draw = Draw {
            tex: &[
                if id == 104 {
                    "pumpkin_stem_disconnected"
                } else {
                    l.tex[0]
                },
                connected,
            ],
            // `BlockStem.getRenderColor`: red age*32, green 255 - age*8, blue age*4.
            rgb: [age * 32, 255 - age * 8, age * 4].map(|c| c as f32 / 255.0),
            light: self.light_at(at, [0; 3]),
            // The model keeps `shade`; its planes are baked with a north/south normal.
            shade: 0.8,
            // `blockstates/*_stem.json`: west 0, north 90, east 180, south 270.
            quarter_turns: facing.map_or(0, |i| [1, 2, 3, 0][i]),
            offset: [0.0; 3],
            flip: false,
        };
        let n = if facing.is_some() { 3 } else { 2 };
        self.draw_model(at, &elems[..n], &draw);
    }

    /// `BlockFire.getActualState` and `blockstates/fire.json`.
    fn fire(&mut self, at: [i32; 3]) {
        let world = [
            self.hood.origin.0 * 16 + at[0],
            at[1],
            self.hood.origin.1 * 16 + at[2],
        ];
        let below = self.st(at, [0, -1, 0]);
        let mut draw = Draw {
            tex: &["fire_layer_0", "fire_layer_1"],
            rgb: [1.0; 3],
            light: self.light_at(at, [0; 3]),
            shade: 1.0,
            quarter_turns: 0,
            offset: [0.0; 3],
            flip: false,
        };
        if solid_top(below) || flammable(below >> 4) {
            return self.draw_model(at, &FIRE_FLOOR, &draw);
        }
        let burns = |d: [i32; 3]| flammable(self.st(at, d) >> 4);
        let mask = HORIZONTAL
            .iter()
            .enumerate()
            .fold(0u8, |m, (i, &d)| m | u8::from(burns(d)) << i);
        let upper = fire_upper(burns([0, 1, 0]), world);
        let [x, y, z] = world;
        // `getActualState`: `flip` is (x+y+z)&1 and `alt` is (x/2+y/2+z/2)&1, which picks the second layer.
        draw.flip = (x + y + z) & 1 == 1;
        draw.tex = if (x / 2 + y / 2 + z / 2) & 1 == 1 {
            &["fire_layer_1"]
        } else {
            &["fire_layer_0"]
        };
        match fire_sides(mask) {
            None => self.draw_model(at, upper.unwrap_or(&FIRE_FLOOR), &draw),
            Some((sides, turns)) => {
                draw.quarter_turns = turns;
                self.draw_model(at, sides, &draw);
                if let Some(u) = upper {
                    draw.flip = false;
                    self.draw_model(at, u, &draw);
                }
            }
        }
    }
}

/// The canopy model over a burning block above: `fire_u1` or `fire_u2` by position parity.
fn fire_upper(burning_above: bool, [x, y, z]: [i32; 3]) -> Option<&'static [Elem<'static>]> {
    match (burning_above, (x + y + z) & 1 == 1) {
        (false, _) => None,
        (true, true) => Some(&FIRE_UPPER_1),
        (true, false) => Some(&FIRE_UPPER_2),
    }
}

/// The side models `fire_n`, `fire_ne`, `fire_ns`, `fire_nse` and `fire_nsew`, with the quarter turns
/// that bring the north-based model onto `mask` (bit 0 north, 1 east, 2 south, 3 west).
fn fire_sides(mask: u8) -> Option<(&'static [Elem<'static>], u8)> {
    let models: [(u8, &[Elem]); 5] = [
        (0b0001, &FIRE_N),
        (0b0011, &FIRE_NE),
        (0b0101, &FIRE_NS),
        (0b0111, &FIRE_NSE),
        (0b1111, &FIRE_NSEW),
    ];
    models.into_iter().find_map(|(base, model)| {
        (0..4u8).find_map(|r| {
            let turned = (base << r | base >> (4 - r)) & 15;
            (turned == mask).then_some((model, r))
        })
    })
}

const SIDE_Y: [f32; 2] = [1.0, 23.4];
const fn side_plane<'a>(from: [f32; 3], to: [f32; 3], faces: &'a [Face]) -> Elem<'a> {
    Elem {
        from,
        to,
        turn: None,
        faces,
    }
}
const PLANE_N: Elem = side_plane(
    [0.0, SIDE_Y[0], 0.01],
    [16.0, SIDE_Y[1], 0.01],
    &[(SOUTH, FULL, 0), (NORTH, FULL, 0)],
);
const PLANE_S: Elem = side_plane(
    [0.0, SIDE_Y[0], 15.99],
    [16.0, SIDE_Y[1], 15.99],
    &[(SOUTH, FULL, 0), (NORTH, FULL, 0)],
);
const PLANE_E: Elem = side_plane(
    [15.99, SIDE_Y[0], 0.0],
    [15.99, SIDE_Y[1], 16.0],
    &[(EAST, FULL, 0), (WEST, FULL, 0)],
);
// `fire_nsew.json` lists its last west plane from x 15.99 to 0.01, so the baker puts the west face at 15.99.
const PLANE_W: [Elem; 2] = [
    side_plane(
        [0.01, SIDE_Y[0], 0.0],
        [0.01, SIDE_Y[1], 16.0],
        &[(EAST, FULL, 0)],
    ),
    side_plane(
        [15.99, SIDE_Y[0], 0.0],
        [0.01, SIDE_Y[1], 16.0],
        &[(WEST, FULL, 0)],
    ),
];
static FIRE_N: [Elem; 1] = [PLANE_N];
static FIRE_NE: [Elem; 2] = [PLANE_N, PLANE_E];
static FIRE_NS: [Elem; 2] = [PLANE_N, PLANE_S];
static FIRE_NSE: [Elem; 3] = [PLANE_N, PLANE_S, PLANE_E];
static FIRE_NSEW: [Elem; 5] = [PLANE_N, PLANE_S, PLANE_E, PLANE_W[0], PLANE_W[1]];

const fn lean(from: [f32; 3], to: [f32; 3], axis: usize, angle: f32, face: usize) -> Elem<'static> {
    let faces: &'static [Face] = match face {
        SOUTH => &[(SOUTH, FULL, 0)],
        NORTH => &[(NORTH, FULL, 0)],
        WEST => &[(WEST, FULL, 0)],
        _ => &[(EAST, FULL, 0)],
    };
    Elem {
        from,
        to,
        turn: Some(Turn {
            axis,
            origin: [8.0; 3],
            angle,
        }),
        faces,
    }
}
const fn edge(from: [f32; 3], to: [f32; 3], face: usize) -> Elem<'static> {
    let faces: &'static [Face] = match face {
        SOUTH => &[(SOUTH, FULL, 0)],
        NORTH => &[(NORTH, FULL, 0)],
        WEST => &[(WEST, FULL, 0)],
        _ => &[(EAST, FULL, 0)],
    };
    Elem {
        from,
        to,
        turn: None,
        faces,
    }
}
/// `fire_floor.json`: four planes leaning 22.5 degrees inwards and four flat planes at the block edge.
static FIRE_FLOOR: [Elem; 8] = [
    lean([0.0, 0.0, 8.8], [16.0, 22.4, 8.8], 0, -22.5, SOUTH),
    lean([0.0, 0.0, 7.2], [16.0, 22.4, 7.2], 0, 22.5, NORTH),
    lean([8.8, 0.0, 0.0], [8.8, 22.4, 16.0], 2, -22.5, WEST),
    lean([7.2, 0.0, 0.0], [7.2, 22.4, 16.0], 2, 22.5, EAST),
    edge([0.0, 0.0, 15.99], [16.0, 22.4, 15.99], SOUTH),
    edge([0.0, 0.0, 0.01], [16.0, 22.4, 0.01], NORTH),
    edge([0.01, 0.0, 0.0], [0.01, 22.4, 16.0], WEST),
    edge([15.99, 0.0, 0.0], [15.99, 22.4, 16.0], EAST),
];
const fn canopy(axis: usize, angle: f32, origin: [f32; 3]) -> Elem<'static> {
    Elem {
        from: [0.0, 16.0, 0.0],
        to: [16.0, 16.0, 16.0],
        turn: Some(Turn {
            axis,
            origin,
            angle,
        }),
        faces: &[(DOWN, FULL, 0)],
    }
}
/// `fire_u1.json` and `fire_u2.json`: the flames that climb a burning block above.
static FIRE_UPPER_1: [Elem; 2] = [
    canopy(2, 22.5, [16.0, 16.0, 8.0]),
    canopy(2, -22.5, [0.0, 16.0, 8.0]),
];
static FIRE_UPPER_2: [Elem; 2] = [
    canopy(0, -22.5, [8.0, 16.0, 16.0]),
    canopy(0, 22.5, [8.0, 16.0, 0.0]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_side_masks_pick_the_turned_model() {
        // fire_ne turned one quarter clockwise covers east and south.
        let (model, turns) = fire_sides(0b0110).unwrap();
        assert_eq!((model.len(), turns), (2, 1));
        assert_eq!(fire_sides(0b1111).unwrap().0.len(), 5);
        assert!(fire_sides(0).is_none());
    }

    #[test]
    fn tall_grass_sits_up_to_a_fifth_lower_and_dead_bush_is_centred() {
        for x in -20..20 {
            let [_, dy, _] = plant_offset(31, [x, 64, 3 * x]);
            assert!((-0.2..=0.0).contains(&dy));
            assert_eq!(plant_offset(32, [x, 64, x]), [0.0; 3]);
        }
    }

    #[test]
    fn wheat_vertices_match_vanilla_crop_json() {
        // Vanilla crop.json: four planes at 4 and 12 px, spanning y -1..15 px.
        let quads = cross_quads(59, [0, 64, 0]);
        assert!(!quads.is_empty());
        for v in quads.iter().flatten().map(|(p, _)| p) {
            assert!(v[1] == -1.0 / 16.0 || v[1] == 15.0 / 16.0, "y {}", v[1]);
            assert!(
                [4.0 / 16.0, 12.0 / 16.0].contains(&v[0])
                    || [4.0 / 16.0, 12.0 / 16.0].contains(&v[2])
            );
        }
    }

    #[test]
    fn plant_offset_matches_the_vanilla_standard_renderer() {
        let mut script = String::new();
        let cells = [[0, 4, 3], [0, 64, 3], [-12, 70, 9], [1000, 71, -9]];
        for [x, _, z] in cells {
            // BlockModelRenderer.renderModelStandardQuads uses the position hash with no y term.
            script.push_str(&format!("offset {x} 0 {z}\n"));
        }
        let out = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(out.len(), cells.len() * 6, "{out:?}");
        for (cell, lines) in cells.iter().zip(out.chunks(6)) {
            assert_eq!(
                lines[1..],
                [
                    "tallgrass|XYZ",
                    "deadbush|NONE",
                    "yellow_flower|XZ",
                    "red_flower|XZ",
                    "double_plant|XZ"
                ],
            );
            let h: i64 = lines[0].parse().unwrap();
            let f = |shift: u32| ((h >> shift & 15) as f32 / 15.0 - 0.5) * 0.5;
            assert_eq!(plant_offset(37, *cell), [f(16), 0.0, f(24)]);
            assert_eq!(plant_offset(38, *cell), [f(16), 0.0, f(24)]);
            assert_eq!(plant_offset(175, *cell), [f(16), 0.0, f(24)]);
            assert_eq!(plant_offset(32, *cell), [0.0; 3]);
            let dy = ((h >> 20 & 15) as f32 / 15.0 - 1.0) * 0.2;
            assert_eq!(plant_offset(31, *cell), [f(16), dy, f(24)]);
        }
    }
}
