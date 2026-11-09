//! Block state (`id << 4 | meta`) to look: kind, texture names per face, tint, light opacity and emission.

/// Face order used everywhere: up, down, north (-z), south (+z), west (-x), east (+x).
pub const UP: usize = 0;
pub const DOWN: usize = 1;
pub const NORTH: usize = 2;
pub const SOUTH: usize = 3;
pub const WEST: usize = 4;
pub const EAST: usize = 5;

/// How a block takes part in meshing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Not drawn and never hides a neighbour (air and blocks this renderer does not model).
    Skip,
    /// Full cube that hides its neighbours.
    Opaque,
    /// Full cube with see-through texels (leaves, glass, ice): neighbours stay visible.
    Cutout,
    /// Water: blended, hides only other water.
    Water,
    /// Lava: opaque texture, hides only other lava.
    Lava,
    /// Two or four crossed quads (flowers, grass, crops).
    Cross,
    /// Built from boxes in `shapes.rs` (slabs, stairs, fences, torches, doors, ...).
    Shape,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    None,
    Grass,
    Foliage,
    Birch,
    Spruce,
    Lily,
    Redstone(u8),
}

#[derive(Clone, Copy)]
pub struct Look {
    pub kind: Kind,
    /// Tint of the up face (all faces for leaves).
    pub tint: Tint,
    /// Flat color used when no textures are loaded.
    pub color: [f32; 3],
    /// Texture names (`textures/blocks/<name>.png`) in `UP, DOWN, NORTH, SOUTH, WEST, EAST` order.
    pub tex: [&'static str; 6],
}

macro_rules! colors {
    ($p:literal) => {
        [
            concat!($p, "white"),
            concat!($p, "orange"),
            concat!($p, "magenta"),
            concat!($p, "light_blue"),
            concat!($p, "yellow"),
            concat!($p, "lime"),
            concat!($p, "pink"),
            concat!($p, "gray"),
            concat!($p, "silver"),
            concat!($p, "cyan"),
            concat!($p, "purple"),
            concat!($p, "blue"),
            concat!($p, "brown"),
            concat!($p, "green"),
            concat!($p, "red"),
            concat!($p, "black"),
        ]
    };
}
pub const WOOL: [&str; 16] = colors!("wool_colored_");
const GLASS: [&str; 16] = colors!("glass_");
const PANE_TOP: [&str; 16] = colors!("glass_pane_top_");
const CLAY: [&str; 16] = colors!("hardened_clay_stained_");
const PLANKS: [&str; 6] = [
    "planks_oak",
    "planks_spruce",
    "planks_birch",
    "planks_jungle",
    "planks_acacia",
    "planks_big_oak",
];

/// Grass, foliage and water colors of the plains biome; chunk biome data is not passed to the renderer yet.
pub const GRASS_COLOR: u32 = 0x91bd59;
pub const FOLIAGE_COLOR: u32 = 0x77ab2f;
pub const BIRCH_COLOR: u32 = 0x80a755;
pub const SPRUCE_COLOR: u32 = 0x619961;
pub const LILY_COLOR: u32 = 0x208030;

pub fn rgb(hex: u32) -> [f32; 3] {
    [
        (hex >> 16 & 255) as f32 / 255.0,
        (hex >> 8 & 255) as f32 / 255.0,
        (hex & 255) as f32 / 255.0,
    ]
}

pub fn tint_color(t: Tint) -> [f32; 3] {
    match t {
        Tint::None => [1.0; 3],
        Tint::Grass => rgb(GRASS_COLOR),
        Tint::Foliage => rgb(FOLIAGE_COLOR),
        Tint::Birch => rgb(BIRCH_COLOR),
        Tint::Spruce => rgb(SPRUCE_COLOR),
        Tint::Lily => rgb(LILY_COLOR),
        // BlockRedstoneWire.colorMultiplier: power 0 is dark red, 15 bright red.
        Tint::Redstone(p) => {
            let f = p as f32 / 15.0;
            [
                f * 0.6 + if p > 0 { 0.4 } else { 0.3 },
                (f * f * 0.7 - 0.5).clamp(0.0, 1.0),
                (f * f * 0.6 - 0.7).clamp(0.0, 1.0),
            ]
        }
    }
}

fn mk(kind: Kind, color: u32, tex: [&'static str; 6]) -> Look {
    Look {
        kind,
        tint: Tint::None,
        color: rgb(color),
        tex,
    }
}

fn cube(color: u32, name: &'static str) -> Look {
    mk(Kind::Opaque, color, [name; 6])
}

fn cube3(color: u32, top: &'static str, bottom: &'static str, side: &'static str) -> Look {
    mk(Kind::Opaque, color, [top, bottom, side, side, side, side])
}

/// Cube whose horizontal-facing side `front` (0 south, 1 west, 2 north, 3 east) uses its own texture.
fn faced(
    color: u32,
    top: &'static str,
    side: &'static str,
    front: &'static str,
    face: usize,
) -> Look {
    let mut l = cube3(color, top, top, side);
    l.tex[face] = front;
    l
}

/// Furnace-like block: meta 2 north, 3 south, 4 west, 5 east.
fn furnace_like(
    meta: u16,
    color: u32,
    top: &'static str,
    side: &'static str,
    front: &'static str,
) -> Look {
    let face = match meta {
        2 => NORTH,
        4 => WEST,
        5 => EAST,
        _ => SOUTH,
    };
    faced(color, top, side, front, face)
}

fn cutout(color: u32, name: &'static str) -> Look {
    Look {
        kind: Kind::Cutout,
        ..cube(color, name)
    }
}

fn leaves(meta: u16, big: bool) -> Look {
    let (name, tint) = match (big, meta & 3) {
        (false, 0) => ("leaves_oak", Tint::Foliage),
        (false, 1) => ("leaves_spruce", Tint::Spruce),
        (false, 2) => ("leaves_birch", Tint::Birch),
        (false, _) => ("leaves_jungle", Tint::Foliage),
        (true, 0) => ("leaves_acacia", Tint::Foliage),
        (true, _) => ("leaves_big_oak", Tint::Foliage),
    };
    Look {
        tint,
        ..cutout(0x3b7a24, name)
    }
}

fn log(meta: u16, second: bool) -> Look {
    let (side, top) = match (second, meta & 3) {
        (false, 0) => ("log_oak", "log_oak_top"),
        (false, 1) => ("log_spruce", "log_spruce_top"),
        (false, 2) => ("log_birch", "log_birch_top"),
        (false, _) => ("log_jungle", "log_jungle_top"),
        (true, 0) => ("log_acacia", "log_acacia_top"),
        (true, _) => ("log_big_oak", "log_big_oak_top"),
    };
    // Bits 2-3: 0 up/down axis, 1 east/west, 2 north/south, 3 bark on every side.
    match meta >> 2 & 3 {
        0 => cube3(0x665132, top, top, side),
        1 => mk(Kind::Opaque, 0x665132, [side, side, side, side, top, top]),
        2 => mk(Kind::Opaque, 0x665132, [side, side, top, top, side, side]),
        _ => cube(0x665132, side),
    }
}

/// Texture names of a slab or double slab variant: top/bottom and side.
fn slab_tex(meta: u16) -> (&'static str, &'static str, &'static str) {
    match meta & 7 {
        0 => ("stone_slab_top", "stone_slab_top", "stone_slab_side"),
        1 => ("sandstone_top", "sandstone_bottom", "sandstone_normal"),
        2 => ("planks_oak", "planks_oak", "planks_oak"),
        3 => ("cobblestone", "cobblestone", "cobblestone"),
        4 => ("brick", "brick", "brick"),
        5 => ("stonebrick", "stonebrick", "stonebrick"),
        6 => ("nether_brick", "nether_brick", "nether_brick"),
        _ => (
            "quartz_block_top",
            "quartz_block_bottom",
            "quartz_block_side",
        ),
    }
}

/// The textured, shape-less look of every plank-like stair, wall and fence material.
const STAIR_TEX: &[(&[u16], &str)] = &[
    (&[53, 85, 107], "planks_oak"),
    (&[67], "cobblestone"),
    (&[108], "brick"),
    (&[109], "stonebrick"),
    (&[114, 113, 139], "nether_brick"),
    (&[128], "sandstone_normal"),
    (&[134, 188, 183], "planks_spruce"),
    (&[135, 189, 184], "planks_birch"),
    (&[136, 190, 185], "planks_jungle"),
    (&[156], "quartz_block_side"),
    (&[163, 192, 187], "planks_acacia"),
    (&[164, 191, 186], "planks_big_oak"),
    (&[180], "red_sandstone_normal"),
];

fn stair_tex(id: u16) -> [&'static str; 6] {
    let t = STAIR_TEX
        .iter()
        .find(|(ids, _)| ids.contains(&id))
        .map_or("stone", |&(_, t)| t);
    [t; 6]
}

fn shape(color: u32, tex: [&'static str; 6]) -> Look {
    mk(Kind::Shape, color, tex)
}

fn cross(color: u32, name: &'static str, tint: Tint) -> Look {
    Look {
        tint,
        ..mk(Kind::Cross, color, [name; 6])
    }
}

/// Ids drawn by `shapes.rs` or as crosses (kept in sync with `look`).
pub fn look(state: u16) -> Look {
    let (id, meta) = (state >> 4, state & 15);
    look_0(id, meta)
}

fn look_0(id: u16, meta: u16) -> Look {
    match id {
        0 => mk(Kind::Skip, 0, ["stone"; 6]),
        1 => cube(
            0x7d7d7d,
            [
                "stone",
                "stone_granite",
                "stone_granite_smooth",
                "stone_diorite",
                "stone_diorite_smooth",
                "stone_andesite",
                "stone_andesite_smooth",
                "stone",
            ][meta as usize & 7],
        ),
        2 => Look {
            tint: Tint::Grass,
            ..cube3(0x5d9a3a, "grass_top", "dirt", "grass_side")
        },
        3 => match meta {
            1 => cube(0x866043, "coarse_dirt"),
            2 => cube3(0x866043, "dirt_podzol_top", "dirt", "dirt_podzol_side"),
            _ => cube(0x866043, "dirt"),
        },
        4 => cube(0x7a7a7a, "cobblestone"),
        5 | 125 => cube(0x9f8150, PLANKS[meta as usize % 6]),
        6 => cross(
            0x4a8a30,
            [
                "sapling_oak",
                "sapling_spruce",
                "sapling_birch",
                "sapling_jungle",
                "sapling_acacia",
                "sapling_roofed_oak",
            ][(meta as usize & 7) % 6],
            Tint::None,
        ),
        _ => look_1(id, meta),
    }
}

fn look_1(id: u16, meta: u16) -> Look {
    match id {
        7 => cube(0x333333, "bedrock"),
        8 | 9 => mk(
            Kind::Water,
            0x2f5bd6,
            [
                "water_still",
                "water_still",
                "water_flow",
                "water_flow",
                "water_flow",
                "water_flow",
            ],
        ),
        10 | 11 => mk(
            Kind::Lava,
            0xe8650c,
            [
                "lava_still",
                "lava_still",
                "lava_flow",
                "lava_flow",
                "lava_flow",
                "lava_flow",
            ],
        ),
        12 => cube(0xdbd3a0, if meta == 1 { "red_sand" } else { "sand" }),
        13 => cube(0x857f7e, "gravel"),
        14 => cube(0x8f8b79, "gold_ore"),
        15 => cube(0x88807b, "iron_ore"),
        16 => cube(0x6c6c6c, "coal_ore"),
        17 => log(meta, false),
        _ => look_2(id, meta),
    }
}

fn look_2(id: u16, meta: u16) -> Look {
    match id {
        18 => leaves(meta, false),
        19 => cube(0xc9c24a, if meta == 1 { "sponge_wet" } else { "sponge" }),
        20 => cutout(0xc0e8ee, "glass"),
        21 => cube(0x6b7488, "lapis_ore"),
        22 => cube(0x2742a0, "lapis_block"),
        23 => furnace_like(
            meta,
            0x6e6e6e,
            "furnace_top",
            "furnace_side",
            "dispenser_front_horizontal",
        ),
        24 => cube3(
            0xdbd3a0,
            "sandstone_top",
            "sandstone_bottom",
            ["sandstone_normal", "sandstone_carved", "sandstone_smooth"][(meta as usize).min(2)],
        ),
        25 => cube3(0x6d5636, "noteblock", "noteblock", "noteblock"),
        26 => shape(
            0xa02a2a,
            [
                "bed_feet_top",
                "planks_oak",
                "bed_feet_side",
                "bed_feet_side",
                "bed_feet_side",
                "bed_feet_side",
            ],
        ),
        _ => look_3(id, meta),
    }
}

fn look_3(id: u16, meta: u16) -> Look {
    match id {
        27 => shape(
            0xa89061,
            [if meta & 8 != 0 {
                "rail_golden_powered"
            } else {
                "rail_golden"
            }; 6],
        ),
        28 => shape(
            0x80705a,
            [if meta & 8 != 0 {
                "rail_detector_powered"
            } else {
                "rail_detector"
            }; 6],
        ),
        29 | 33 => cube3(
            0x9a8260,
            "piston_top_normal",
            "piston_bottom",
            "piston_side",
        ),
        30 => cross(0xe0e0e0, "web", Tint::None),
        31 => match meta {
            0 => cross(0x4a8a30, "deadbush", Tint::None),
            2 => cross(0x4a8a30, "fern", Tint::Grass),
            _ => cross(0x4a8a30, "tallgrass", Tint::Grass),
        },
        32 => cross(0x7a5a33, "deadbush", Tint::None),
        34 => mk(Kind::Skip, 0, ["stone"; 6]),
        _ => look_4(id, meta),
    }
}

fn look_4(id: u16, meta: u16) -> Look {
    match id {
        35 => cube(0xe0e0e0, WOOL[meta as usize]),
        37 => cross(0xf0e020, "flower_dandelion", Tint::None),
        38 => cross(
            0xd02020,
            [
                "flower_rose",
                "flower_blue_orchid",
                "flower_allium",
                "flower_houstonia",
                "flower_tulip_red",
                "flower_tulip_orange",
                "flower_tulip_white",
                "flower_tulip_pink",
                "flower_oxeye_daisy",
            ][(meta as usize).min(8)],
            Tint::None,
        ),
        39 => cross(0x967a64, "mushroom_brown", Tint::None),
        40 => cross(0xc04040, "mushroom_red", Tint::None),
        41 => cube(0xf9ec4e, "gold_block"),
        42 => cube(0xdddddd, "iron_block"),
        43 => {
            let (t, b, s) = slab_tex(meta);
            if meta == 8 {
                cube(0xa8a8a8, "stone_slab_top")
            } else {
                cube3(0xa8a8a8, t, b, s)
            }
        }
        _ => look_5(id, meta),
    }
}

fn look_5(id: u16, meta: u16) -> Look {
    match id {
        44 | 126 | 182 => {
            let (t, b, s) = match id {
                126 => (
                    PLANKS[(meta as usize & 7) % 6],
                    PLANKS[(meta as usize & 7) % 6],
                    PLANKS[(meta as usize & 7) % 6],
                ),
                182 => (
                    "red_sandstone_top",
                    "red_sandstone_bottom",
                    "red_sandstone_normal",
                ),
                _ => slab_tex(meta),
            };
            shape(0xa8a8a8, [t, b, s, s, s, s])
        }
        45 => cube(0x965a4a, "brick"),
        46 => cube3(0xdb2e1f, "tnt_top", "tnt_bottom", "tnt_side"),
        47 => cube3(0x6d5636, "planks_oak", "planks_oak", "bookshelf"),
        48 => cube(0x5f7a5f, "cobblestone_mossy"),
        49 => cube(0x14121f, "obsidian"),
        50 => shape(0xf0c040, ["torch_on"; 6]),
        _ => look_6(id, meta),
    }
}

fn look_6(id: u16, meta: u16) -> Look {
    match id {
        51 => cross(0xe06010, "fire_layer_0", Tint::None),
        52 => cutout(0x1a2a40, "mob_spawner"),
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => {
            shape(0x9f8150, stair_tex(id))
        }
        54 | 146 => shape(0x8a5a2a, ["planks_oak"; 6]),
        55 => Look {
            tint: Tint::Redstone(meta as u8),
            ..shape(0xa00000, ["redstone_dust_cross"; 6])
        },
        56 => cube(0x8a9a9d, "diamond_ore"),
        57 => cube(0x62ece0, "diamond_block"),
        58 => cube3(
            0x7a5a33,
            "crafting_table_top",
            "planks_oak",
            "crafting_table_side",
        ),
        59 => cross(
            0x80a030,
            [
                "wheat_stage_0",
                "wheat_stage_1",
                "wheat_stage_2",
                "wheat_stage_3",
                "wheat_stage_4",
                "wheat_stage_5",
                "wheat_stage_6",
                "wheat_stage_7",
            ][meta as usize & 7],
            Tint::None,
        ),
        _ => look_7(id, meta),
    }
}

fn look_7(id: u16, meta: u16) -> Look {
    match id {
        60 => shape(
            0x6b4a2a,
            [
                if meta > 0 {
                    "farmland_wet"
                } else {
                    "farmland_dry"
                },
                "dirt",
                "dirt",
                "dirt",
                "dirt",
                "dirt",
            ],
        ),
        61 | 62 => furnace_like(
            meta,
            0x6e6e6e,
            "furnace_top",
            "furnace_side",
            if id == 62 {
                "furnace_front_on"
            } else {
                "furnace_front_off"
            },
        ),
        _ => look_8(id, meta),
    }
}

fn look_8(id: u16, meta: u16) -> Look {
    match id {
        64 | 71 | 193..=197 => shape(
            0x9f8150,
            [match id {
                64 => "door_wood_lower",
                71 => "door_iron_lower",
                193 => "door_spruce_lower",
                194 => "door_birch_lower",
                195 => "door_jungle_lower",
                196 => "door_acacia_lower",
                _ => "door_dark_oak_lower",
            }; 6],
        ),
        65 => shape(0x9a7a40, ["ladder"; 6]),
        66 => shape(
            0x80705a,
            [if meta >= 6 {
                "rail_normal_turned"
            } else {
                "rail_normal"
            }; 6],
        ),
        _ => look_9(id, meta),
    }
}

fn look_9(id: u16, meta: u16) -> Look {
    match id {
        70 => shape(0x7d7d7d, ["stone"; 6]),
        72 => shape(0x9f8150, ["planks_oak"; 6]),
        73 => cube(0x8a6e6e, "redstone_ore"),
        74 => cube(0x8a6e6e, "redstone_ore"),
        75 => shape(0x502010, ["redstone_torch_off"; 6]),
        76 => shape(0xd02010, ["redstone_torch_on"; 6]),
        78 => shape(0xf5fafa, ["snow"; 6]),
        79 => cutout(0x9cc6ff, "ice"),
        80 => cube(0xf5fafa, "snow"),
        81 => shape(
            0x0d6a1c,
            [
                "cactus_top",
                "cactus_bottom",
                "cactus_side",
                "cactus_side",
                "cactus_side",
                "cactus_side",
            ],
        ),
        _ => look_10(id, meta),
    }
}

fn look_10(id: u16, meta: u16) -> Look {
    match id {
        82 => cube(0xa0a6b3, "clay"),
        83 => cross(0x80b060, "reeds", Tint::Grass),
        84 => cube3(0x6d5636, "jukebox_top", "jukebox_side", "jukebox_side"),
        85 | 113 | 188..=192 => shape(0x9f8150, stair_tex(id)),
        86 => faced(
            0xc77a1a,
            "pumpkin_top",
            "pumpkin_side",
            "pumpkin_face_off",
            pumpkin_front(meta),
        ),
        87 => cube(0x6f3636, "netherrack"),
        88 => cube(0x54402f, "soul_sand"),
        89 => cube(0xf0d08a, "glowstone"),
        91 => faced(
            0xc77a1a,
            "pumpkin_top",
            "pumpkin_side",
            "pumpkin_face_on",
            pumpkin_front(meta),
        ),
        92 => shape(
            0xf0e0e0,
            [
                "cake_top",
                "cake_bottom",
                "cake_side",
                "cake_side",
                "cake_side",
                "cake_side",
            ],
        ),
        _ => look_11(id, meta),
    }
}

fn look_11(id: u16, meta: u16) -> Look {
    match id {
        93 | 94 => shape(
            0x909090,
            [["repeater_off", "repeater_on"][usize::from(id == 94)]; 6],
        ),
        95 => cutout(0xc0e8ee, GLASS[meta as usize]),
        96 | 167 => shape(
            0x9f8150,
            [["iron_trapdoor", "trapdoor"][usize::from(id == 96)]; 6],
        ),
        97 => cube(
            0x7d7d7d,
            [
                "stone",
                "cobblestone",
                "stonebrick",
                "stonebrick_mossy",
                "stonebrick_cracked",
                "stonebrick_carved",
            ][meta.min(5) as usize],
        ),
        98 => cube(
            0x7b7b7b,
            [
                "stonebrick",
                "stonebrick_mossy",
                "stonebrick_cracked",
                "stonebrick_carved",
            ][meta as usize & 3],
        ),
        99 | 100 => {
            let tex = mushroom_tex(id, meta);
            let mut l = cube(0x967a64, "mushroom_block_inside");
            l.tex = tex;
            l
        }
        101 => shape(0xa0a0a0, ["iron_bars"; 6]),
        _ => look_12(id, meta),
    }
}

/// Vanilla `BlockHugeMushroom` variants: 1-9 are cap corners/edges/centre, 10 stem sides, 14 all cap, 15 all stem.
fn mushroom_tex(id: u16, meta: u16) -> [&'static str; 6] {
    const INSIDE: &str = "mushroom_block_inside";
    let skin = if id == 99 {
        "mushroom_block_skin_brown"
    } else {
        "mushroom_block_skin_red"
    };
    let stem = "mushroom_block_skin_stem";
    let mut t = [INSIDE; 6];
    match meta {
        1..=9 => {
            t[UP] = skin;
            let (row, col) = ((meta - 1) / 3, (meta - 1) % 3);
            // Rows 0-2 are north, middle, south; columns 0-2 are west, middle, east.
            match row {
                0 => t[NORTH] = skin,
                2 => t[SOUTH] = skin,
                _ => {}
            }
            match col {
                0 => t[WEST] = skin,
                2 => t[EAST] = skin,
                _ => {}
            }
        }
        10 => {
            for f in [NORTH, SOUTH, WEST, EAST] {
                t[f] = stem;
            }
        }
        14 => t = [skin; 6],
        15 => t = [stem; 6],
        _ => {}
    }
    t
}

fn look_12(id: u16, meta: u16) -> Look {
    match id {
        102 => shape(0xc0e8ee, ["glass"; 6]),
        103 => cube3(0x8aa030, "melon_top", "melon_top", "melon_side"),
        104 | 105 => cross(0x80a030, "melon_stem_disconnected", Tint::Foliage),
        106 => Look {
            tint: Tint::Foliage,
            ..shape(0x3b7a24, ["vine"; 6])
        },
        107 | 183..=187 => shape(0x9f8150, stair_tex(id)),
        110 => cube3(0x6f6a7a, "mycelium_top", "dirt", "mycelium_side"),
        111 => Look {
            tint: Tint::Lily,
            ..shape(0x208030, ["waterlily"; 6])
        },
        112 => cube(0x2d1519, "nether_brick"),
        115 => cross(
            0x7a1a20,
            [
                "nether_wart_stage_0",
                "nether_wart_stage_1",
                "nether_wart_stage_1",
                "nether_wart_stage_2",
            ][meta as usize & 3],
            Tint::None,
        ),
        116 => shape(
            0x7a2030,
            [
                "enchanting_table_top",
                "enchanting_table_bottom",
                "enchanting_table_side",
                "enchanting_table_side",
                "enchanting_table_side",
                "enchanting_table_side",
            ],
        ),
        _ => look_13(id, meta),
    }
}

fn look_13(id: u16, meta: u16) -> Look {
    match id {
        120 => shape(
            0x7a8a60,
            [
                "endframe_top",
                "end_stone",
                "endframe_side",
                "endframe_side",
                "endframe_side",
                "endframe_side",
            ],
        ),
        121 => cube(0xe0e0a0, "end_stone"),
        123 => cube(0x6a4a2a, "redstone_lamp_off"),
        124 => cube(0xf0d08a, "redstone_lamp_on"),
        129 => cube(0x7f9a86, "emerald_ore"),
        130 => shape(0x1a1a2a, ["obsidian"; 6]),
        133 => cube(0x2fc15a, "emerald_block"),
        137 => cube(0xc08040, "command_block"),
        139 => shape(
            0x7a7a7a,
            if meta == 1 {
                ["cobblestone_mossy"; 6]
            } else {
                ["cobblestone"; 6]
            },
        ),
        _ => look_14(id, meta),
    }
}

fn look_14(id: u16, meta: u16) -> Look {
    match id {
        140 => shape(0x7a3a2a, ["flower_pot"; 6]),
        141 => cross(
            0x80a030,
            [
                "carrots_stage_0",
                "carrots_stage_0",
                "carrots_stage_1",
                "carrots_stage_1",
                "carrots_stage_2",
                "carrots_stage_2",
                "carrots_stage_2",
                "carrots_stage_3",
            ][meta as usize & 7],
            Tint::None,
        ),
        142 => cross(
            0x80a030,
            [
                "potatoes_stage_0",
                "potatoes_stage_0",
                "potatoes_stage_1",
                "potatoes_stage_1",
                "potatoes_stage_2",
                "potatoes_stage_2",
                "potatoes_stage_2",
                "potatoes_stage_3",
            ][meta as usize & 7],
            Tint::None,
        ),
        151 => shape(
            0x9f8150,
            [
                "daylight_detector_top",
                "daylight_detector_side",
                "daylight_detector_side",
                "daylight_detector_side",
                "daylight_detector_side",
                "daylight_detector_side",
            ],
        ),
        152 => cube(0xaa1010, "redstone_block"),
        153 => cube(0x7a4a4a, "quartz_ore"),
        _ => look_15(id, meta),
    }
}

fn look_15(id: u16, meta: u16) -> Look {
    match id {
        155 => match meta {
            1 => cube3(
                0xe8e2d8,
                "quartz_block_chiseled_top",
                "quartz_block_chiseled_top",
                "quartz_block_chiseled",
            ),
            2 => cube3(
                0xe8e2d8,
                "quartz_block_lines_top",
                "quartz_block_lines_top",
                "quartz_block_lines",
            ),
            3 => mk(
                Kind::Opaque,
                0xe8e2d8,
                [
                    "quartz_block_lines",
                    "quartz_block_lines",
                    "quartz_block_lines",
                    "quartz_block_lines",
                    "quartz_block_lines_top",
                    "quartz_block_lines_top",
                ],
            ),
            4 => mk(
                Kind::Opaque,
                0xe8e2d8,
                [
                    "quartz_block_lines",
                    "quartz_block_lines",
                    "quartz_block_lines_top",
                    "quartz_block_lines_top",
                    "quartz_block_lines",
                    "quartz_block_lines",
                ],
            ),
            _ => cube3(
                0xe8e2d8,
                "quartz_block_top",
                "quartz_block_bottom",
                "quartz_block_side",
            ),
        },
        158 => furnace_like(
            meta,
            0x6e6e6e,
            "furnace_top",
            "furnace_side",
            "dropper_front_horizontal",
        ),
        159 => cube(0x965e43, CLAY[meta as usize]),
        160 => shape(0xc0e8ee, [GLASS[meta as usize]; 6]),
        161 => leaves(meta, true),
        _ => look_16(id, meta),
    }
}

fn look_16(id: u16, meta: u16) -> Look {
    match id {
        162 => log(meta, true),
        165 => cutout(0x80c070, "slime"),
        168 => cube(
            0x5a9a8a,
            [
                "prismarine_rough",
                "prismarine_bricks",
                "prismarine_dark",
                "prismarine_rough",
            ][meta as usize & 3],
        ),
        169 => cube(0xc0e8e0, "sea_lantern"),
        170 => match meta >> 2 & 3 {
            0 => cube3(0xc0a020, "hay_block_top", "hay_block_top", "hay_block_side"),
            1 => mk(
                Kind::Opaque,
                0xc0a020,
                [
                    "hay_block_side",
                    "hay_block_side",
                    "hay_block_side",
                    "hay_block_side",
                    "hay_block_top",
                    "hay_block_top",
                ],
            ),
            _ => mk(
                Kind::Opaque,
                0xc0a020,
                [
                    "hay_block_side",
                    "hay_block_side",
                    "hay_block_top",
                    "hay_block_top",
                    "hay_block_side",
                    "hay_block_side",
                ],
            ),
        },
        171 => shape(0xe0e0e0, [WOOL[meta as usize]; 6]),
        172 => cube(0x965e43, "hardened_clay"),
        _ => look_17(id, meta),
    }
}

fn look_17(id: u16, meta: u16) -> Look {
    match id {
        173 => cube(0x131313, "coal_block"),
        174 => cube(0x9cc6ff, "ice_packed"),
        175 => match meta & 7 {
            0 => cross(0xf0e020, "double_plant_sunflower_bottom", Tint::None),
            1 => cross(0xc060c0, "double_plant_syringa_bottom", Tint::None),
            2 => cross(0x4a8a30, "double_plant_grass_bottom", Tint::Grass),
            3 => cross(0x4a8a30, "double_plant_fern_bottom", Tint::Grass),
            4 => cross(0xd02020, "double_plant_rose_bottom", Tint::None),
            _ => cross(0xf0c0d0, "double_plant_paeonia_bottom", Tint::None),
        },
        179 => cube3(
            0xa85a2a,
            "red_sandstone_top",
            "red_sandstone_bottom",
            [
                "red_sandstone_normal",
                "red_sandstone_carved",
                "red_sandstone_smooth",
            ][(meta as usize).min(2)],
        ),
        _ => look_18(id, meta),
    }
}

fn look_18(id: u16, _meta: u16) -> Look {
    match id {
        181 => cube3(
            0xa85a2a,
            "red_sandstone_top",
            "red_sandstone_bottom",
            "red_sandstone_normal",
        ),
        _ => mk(Kind::Skip, 0, ["stone"; 6]),
    }
}

pub fn pane_top(meta: u16) -> &'static str {
    PANE_TOP[meta as usize & 15]
}

/// Upper-half texture of a door from its lower-half texture name.
pub fn door_upper(lower: &'static str) -> &'static str {
    match lower {
        "door_wood_lower" => "door_wood_upper",
        "door_iron_lower" => "door_iron_upper",
        "door_spruce_lower" => "door_spruce_upper",
        "door_birch_lower" => "door_birch_upper",
        "door_jungle_lower" => "door_jungle_upper",
        "door_acacia_lower" => "door_acacia_upper",
        _ => "door_dark_oak_upper",
    }
}

fn pumpkin_front(meta: u16) -> usize {
    match meta & 3 {
        0 => SOUTH,
        1 => WEST,
        2 => NORTH,
        _ => EAST,
    }
}

/// Light blocked by the block (0 transparent, 15 opaque), as `Block.getLightOpacity` in 1.8.9 (255 reads as 15).
pub fn light_opacity(state: u16) -> u8 {
    match state >> 4 {
        0
        | 6
        | 10..=11
        | 20
        | 26..=29
        | 31..=34
        | 36..=40
        | 50..=52
        | 54..=55
        | 59
        | 63..=66
        | 68..=72
        | 75..=78
        | 81
        | 83
        | 85
        | 90
        | 92..=96
        | 101..=102
        | 104..=107
        | 111
        | 115..=120
        | 122
        | 127
        | 130..=132
        | 138..=151
        | 154
        | 157
        | 160
        | 165..=167
        | 171
        | 175..=178
        | 183..=197 => 0,
        18 | 30 | 161 => 1,
        8..=9 | 79 => 3,
        _ => 15,
    }
}

/// Whether the block shows the light of its neighbours, as `Block.getUseNeighborBrightness` in 1.8.9.
pub fn uses_neighbor_light(state: u16) -> bool {
    matches!(
        state >> 4,
        6 | 10..=11 | 20 | 26..=29 | 31..=34 | 36..=40 | 43..=44 | 50..=55 | 59..=60 | 63..=72
            | 75..=78 | 81 | 83 | 85 | 90 | 92..=96 | 101..=102 | 104..=109 | 111 | 113..=120
            | 122 | 125..=128 | 130..=132 | 134..=136 | 138..=151 | 154 | 156..=157 | 160
            | 163..=167 | 171 | 175..=178 | 180..=197
    )
}

/// Block light emitted (0-15), as `Block.getLightValue` in 1.8.9.
pub fn emission(state: u16) -> u8 {
    match state >> 4 {
        10 | 11 | 89 | 91 | 169 | 124 | 51 | 138 | 119 => 15,
        62 => 13,
        50 => 14,
        90 => 11,
        74 | 150 => 9,
        76 | 130 => 7,
        39 | 117 | 120 | 122 => 1,
        _ => 0,
    }
}

/// `look` for every block state, computed once; the mesher calls it for each block of a chunk.
pub fn cached(state: u16) -> &'static Look {
    static TABLE: std::sync::LazyLock<Vec<Look>> =
        std::sync::LazyLock::new(|| (0..4096u16).map(look).collect());
    &TABLE[state as usize & 4095]
}

/// Texture and tint of the upper half of a double plant whose lower half has `variant` metadata.
pub fn double_plant_top(variant: u16) -> (&'static str, Tint) {
    match variant & 7 {
        0 => ("double_plant_sunflower_top", Tint::None),
        1 => ("double_plant_syringa_top", Tint::None),
        2 => ("double_plant_grass_top", Tint::Grass),
        3 => ("double_plant_fern_top", Tint::Grass),
        4 => ("double_plant_rose_top", Tint::None),
        _ => ("double_plant_paeonia_top", Tint::None),
    }
}

/// Blocks that count as full cubes for ambient occlusion and face culling.
pub fn is_opaque_cube(state: u16) -> bool {
    cached(state).kind == Kind::Opaque
}

/// Every texture name any state can ask for, for atlas building.
pub fn texture_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = (0..4096u16)
        .map(look)
        .filter(|l| l.kind != Kind::Skip)
        .flat_map(|l| l.tex)
        .chain([
            "grass_side_overlay",
            "grass_side_snowed",
            "door_wood_upper",
            "door_iron_upper",
            "door_spruce_upper",
            "door_birch_upper",
            "door_jungle_upper",
            "door_acacia_upper",
            "door_dark_oak_upper",
            "glass_pane_top",
            "vine",
            "double_plant_sunflower_top",
            "double_plant_syringa_top",
            "double_plant_grass_top",
            "double_plant_fern_top",
            "double_plant_rose_top",
            "double_plant_paeonia_top",
            "double_plant_sunflower_front",
            "double_plant_sunflower_back",
        ])
        .collect();
    names.extend(PANE_TOP);
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mushroom_and_infested_blocks_pick_texture_by_meta() {
        let t = look(99 << 4 | 1).tex;
        assert_eq!(
            [t[UP], t[NORTH], t[WEST], t[SOUTH]],
            [
                "mushroom_block_skin_brown",
                "mushroom_block_skin_brown",
                "mushroom_block_skin_brown",
                "mushroom_block_inside"
            ]
        );
        assert_eq!(look(100 << 4 | 10).tex[EAST], "mushroom_block_skin_stem");
        assert_eq!(look(100 << 4 | 15).tex[UP], "mushroom_block_skin_stem");
        assert_eq!(look(97 << 4 | 1).tex[UP], "cobblestone");
        assert_eq!(look(97 << 4 | 5).tex[UP], "stonebrick_carved");
    }

    #[test]
    fn colored_blocks_pick_texture_by_meta() {
        assert_eq!(look(35 << 4 | 14).tex[UP], "wool_colored_red");
        assert_eq!(
            look(159 << 4 | 3).tex[UP],
            "hardened_clay_stained_light_blue"
        );
    }

    #[test]
    fn log_axis_moves_the_end_texture() {
        // East/west log: the ends face -x/+x.
        let l = look(17 << 4 | 4);
        assert_eq!((l.tex[EAST], l.tex[UP]), ("log_oak_top", "log_oak"));
    }

    #[test]
    fn furnace_front_follows_meta() {
        assert_eq!(look(61 << 4 | 5).tex[EAST], "furnace_front_off");
        assert_eq!(look(61 << 4 | 5).tex[SOUTH], "furnace_side");
    }
}
