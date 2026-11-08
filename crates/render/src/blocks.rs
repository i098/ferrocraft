//! Block id (the high 12 bits of a 1.8 block state) to look: texture names per face and a flat color.

/// How a block takes part in meshing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Not drawn and never hides a neighbour (air, plants, torches, rails and similar).
    Skip,
    /// Full cube that hides its neighbours.
    Opaque,
    /// Full cube with see-through texels (leaves, glass, ice): neighbours stay visible.
    Cutout,
    /// Water: blended, hides only other water.
    Water,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    None,
    Grass,
}

#[derive(Clone, Copy)]
pub struct Look {
    pub kind: Kind,
    pub tint: Tint,
    /// Flat color used when no textures are loaded.
    pub color: [f32; 3],
    /// Texture names for top, bottom and the four sides (`textures/blocks/<name>.png`).
    pub top: &'static str,
    pub bottom: &'static str,
    pub side: &'static str,
}

/// Blocks that are not full cubes in 1.8.9 and that this renderer does not draw.
const SKIP: &[u16] = &[
    0, 6, 26, 27, 28, 30, 31, 32, 34, 36, 37, 38, 39, 40, 50, 51, 55, 59, 63, 64, 65, 66, 68, 69,
    70, 71, 72, 75, 76, 77, 83, 85, 90, 92, 93, 94, 96, 101, 102, 104, 105, 106, 107, 111, 113,
    115, 117, 118, 119, 120, 131, 132, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150,
    154, 157, 160, 171, 175,
];

fn rgb(hex: u32) -> [f32; 3] {
    [
        (hex >> 16 & 255) as f32 / 255.0,
        (hex >> 8 & 255) as f32 / 255.0,
        (hex & 255) as f32 / 255.0,
    ]
}

fn cube(color: u32, name: &'static str) -> Look {
    Look {
        kind: Kind::Opaque,
        tint: Tint::None,
        color: rgb(color),
        top: name,
        bottom: name,
        side: name,
    }
}

fn cube3(color: u32, top: &'static str, bottom: &'static str, side: &'static str) -> Look {
    Look {
        top,
        bottom,
        side,
        ..cube(color, top)
    }
}

fn cutout(color: u32, name: &'static str) -> Look {
    Look {
        kind: Kind::Cutout,
        ..cube(color, name)
    }
}

type Entry = (u16, fn() -> Look);
const TABLE: &[Entry] = &[
    (1, || cube(0x7d7d7d, "stone")),
    (2, || Look {
        tint: Tint::Grass,
        ..cube3(0x5d9a3a, "grass_top", "dirt", "grass_side")
    }),
    (3, || cube(0x866043, "dirt")),
    (4, || cube(0x7a7a7a, "cobblestone")),
    (5, || cube(0x9f8150, "planks_oak")),
    (7, || cube(0x333333, "bedrock")),
    (8, || Look {
        kind: Kind::Water,
        ..cube(0x2f5bd6, "water_still")
    }),
    (9, || Look {
        kind: Kind::Water,
        ..cube(0x2f5bd6, "water_still")
    }),
    (10, || cube(0xe8650c, "lava_still")),
    (11, || cube(0xe8650c, "lava_still")),
    (12, || cube(0xdbd3a0, "sand")),
    (13, || cube(0x857f7e, "gravel")),
    (14, || cube(0x8f8b79, "gold_ore")),
    (15, || cube(0x88807b, "iron_ore")),
    (16, || cube(0x6c6c6c, "coal_ore")),
    (17, || {
        cube3(0x665132, "log_oak_top", "log_oak_top", "log_oak")
    }),
    (18, || Look {
        tint: Tint::Grass,
        ..cutout(0x3b7a24, "leaves_oak")
    }),
    (19, || cube(0xc9c24a, "sponge")),
    (20, || cutout(0xc0e8ee, "glass")),
    (21, || cube(0x6b7488, "lapis_ore")),
    (22, || cube(0x2742a0, "lapis_block")),
    (24, || {
        cube3(
            0xdbd3a0,
            "sandstone_top",
            "sandstone_bottom",
            "sandstone_normal",
        )
    }),
    (35, || cube(0xe0e0e0, "wool_colored_white")),
    (41, || cube(0xf9ec4e, "gold_block")),
    (42, || cube(0xdddddd, "iron_block")),
    (43, || cube(0xa8a8a8, "stone_slab_top")),
    (44, || cube(0xa8a8a8, "stone_slab_top")),
    (45, || cube(0x965a4a, "brick")),
    (46, || cube3(0xdb2e1f, "tnt_top", "tnt_bottom", "tnt_side")),
    (47, || {
        cube3(0x6d5636, "planks_oak", "planks_oak", "bookshelf")
    }),
    (48, || cube(0x5f7a5f, "cobblestone_mossy")),
    (49, || cube(0x14121f, "obsidian")),
    (56, || cube(0x8a9a9d, "diamond_ore")),
    (57, || cube(0x62ece0, "diamond_block")),
    (58, || {
        cube3(
            0x7a5a33,
            "crafting_table_top",
            "planks_oak",
            "crafting_table_side",
        )
    }),
    (73, || cube(0x8a6e6e, "redstone_ore")),
    (74, || cube(0x8a6e6e, "redstone_ore")),
    (78, || cube(0xf5fafa, "snow")),
    (79, || cutout(0x9cc6ff, "ice")),
    (80, || cube(0xf5fafa, "snow")),
    (81, || {
        cube3(0x0d6a1c, "cactus_top", "cactus_bottom", "cactus_side")
    }),
    (82, || cube(0xa0a6b3, "clay")),
    (86, || {
        cube3(0xc77a1a, "pumpkin_top", "pumpkin_top", "pumpkin_side")
    }),
    (87, || cube(0x6f3636, "netherrack")),
    (88, || cube(0x54402f, "soul_sand")),
    (89, || cube(0xf0d08a, "glowstone")),
    (95, || cutout(0xc0e8ee, "glass")),
    (98, || cube(0x7b7b7b, "stonebrick")),
    (102, || cutout(0xc0e8ee, "glass")),
    (112, || cube(0x2d1519, "nether_brick")),
    (129, || cube(0x7f9a86, "emerald_ore")),
    (133, || cube(0x2fc15a, "emerald_block")),
    (159, || cube(0x965e43, "hardened_clay")),
    (161, || Look {
        tint: Tint::Grass,
        ..cutout(0x3b7a24, "leaves_oak")
    }),
    (162, || {
        cube3(0x665132, "log_oak_top", "log_oak_top", "log_oak")
    }),
    (172, || cube(0x965e43, "hardened_clay")),
    (173, || cube(0x131313, "coal_block")),
];

pub fn look(id: u16) -> Look {
    if SKIP.contains(&id) {
        return Look {
            kind: Kind::Skip,
            ..cube(0, "stone")
        };
    }
    match TABLE.binary_search_by_key(&id, |e| e.0) {
        Ok(i) => (TABLE[i].1)(),
        Err(_) => {
            // Unknown full cube: stable gray-brown from the id so different blocks stay distinguishable.
            let h = (id as u32).wrapping_mul(2654435761);
            cube(0x606060 + (h >> 8 & 0x3f3f3f), "stone")
        }
    }
}

/// Every texture name the table can ask for, for atlas building.
pub fn texture_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = (0..256u16)
        .map(look)
        .filter(|l| l.kind != Kind::Skip)
        .flat_map(|l| [l.top, l.bottom, l.side])
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}
