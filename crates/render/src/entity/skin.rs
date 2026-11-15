//! One texture for every entity model: 128x64 slots (the iron golem and horse textures are 128x128 and
//! take two). The fixed slots come first, then one plain white slot, then [`PLAYER_SLOTS`] slots for downloaded
//! player skins and [`CAPE_SLOTS`] for downloaded capes. [`Skins::rgba`] stacks them in one column; the
//! texture itself cuts that column into several, so that no side passes 8192 texels.

use crate::assets::{decode_png, AssetSource, Image};

pub(super) mod horse;

/// Height of one slot; most textures are 64 wide, so they fill the left half of a row.
pub const SLOT: u32 = 64;
/// Width of the atlas, for the 256 wide ender dragon texture.
const WIDTH: u32 = 256;
/// The largest side of the atlas texture; 8192 is the limit every GPU has to offer.
const MAX_SIDE: u32 = 8192;

/// Texture slots in atlas order; the path is relative to `assets/minecraft/textures/`. The twelve
/// armor layers (see [`Skin::armor`]) come before the elder guardian.
const SKINS: [&str; 92] = [
    "entity/steve.png",
    "entity/alex.png",
    "entity/zombie/zombie.png",
    "entity/skeleton/skeleton.png",
    "entity/zombie_pigman.png",
    "entity/creeper/creeper.png",
    "entity/pig/pig.png",
    "entity/cow/cow.png",
    "entity/arrow.png",
    "items/snowball.png",
    "items/egg.png",
    "items/ender_pearl.png",
    "items/fireball.png",
    "particle/particles.png",
    "items/potion_bottle_splash.png",
    "models/armor/leather_layer_1.png",
    "models/armor/leather_layer_2.png",
    "models/armor/chainmail_layer_1.png",
    "models/armor/chainmail_layer_2.png",
    "models/armor/iron_layer_1.png",
    "models/armor/iron_layer_2.png",
    "models/armor/diamond_layer_1.png",
    "models/armor/diamond_layer_2.png",
    "models/armor/gold_layer_1.png",
    "models/armor/gold_layer_2.png",
    "models/armor/leather_layer_1_overlay.png",
    "models/armor/leather_layer_2_overlay.png",
    "entity/guardian_elder.png",
    "entity/villager/villager.png",
    "entity/villager/farmer.png",
    "entity/villager/librarian.png",
    "entity/villager/priest.png",
    "entity/villager/smith.png",
    "entity/villager/butcher.png",
    "entity/iron_golem.png",
    // The second half of the 128x128 golem texture.
    "",
    "entity/silverfish.png",
    "entity/chicken.png",
    "entity/sheep/sheep.png",
    "entity/sheep/sheep_fur.png",
    "entity/spider/spider.png",
    "entity/spider/cave_spider.png",
    "entity/enderman/enderman.png",
    "entity/spider_eyes.png",
    "entity/enderman/enderman_eyes.png",
    "entity/armorstand/wood.png",
    "entity/slime/slime.png",
    "entity/slime/magmacube.png",
    "entity/blaze.png",
    "entity/ghast/ghast.png",
    "entity/ghast/ghast_shooting.png",
    "entity/bat.png",
    "items/fireworks.png",
    "entity/wolf/wolf.png",
    "entity/wolf/wolf_tame.png",
    "entity/wolf/wolf_angry.png",
    "entity/wolf/wolf_collar.png",
    "entity/cat/ocelot.png",
    "entity/cat/black.png",
    "entity/cat/red.png",
    "entity/cat/siamese.png",
    "entity/rabbit/brown.png",
    "entity/rabbit/white.png",
    "entity/rabbit/black.png",
    "entity/rabbit/white_splotched.png",
    "entity/rabbit/gold.png",
    "entity/rabbit/salt.png",
    "entity/rabbit/caerbannog.png",
    "entity/rabbit/toast.png",
    "entity/boat.png",
    "entity/minecart.png",
    "entity/squid.png",
    "entity/endercrystal/endercrystal.png",
    "entity/cow/mooshroom.png",
    "items/ender_eye.png",
    "items/experience_bottle.png",
    "entity/wither/wither.png",
    "entity/wither/wither_invulnerable.png",
    "entity/snowman.png",
    "entity/witch.png",
    // The second half of the 64x128 witch texture.
    "",
    "entity/lead_knot.png",
    "entity/endermite.png",
    "entity/guardian.png",
    "entity/enderdragon/dragon.png",
    // The other three slots of the 256x256 dragon texture.
    "",
    "",
    "",
    "entity/enderdragon/dragon_eyes.png",
    "",
    "",
    "",
];
/// Index of the first mob texture after the elder guardian; these are copied as plain sprites.
const MOBS_FIRST: usize = 28;
/// Slots of the eye layer textures; their black counts as transparent (additive blending).
const EYES: [usize; 3] = [43, 44, 88];
/// Slot of `slime.png` and the size of its outer gel region (top left, texels): the entity pass has
/// no blending, so [`Skins::screen_door`] punches every other texel of the gel, which is 78% opaque.
const SLIME_SLOT: usize = 46;
/// Skins of the slots from [`SLIME_SLOT`] on, in order.
const SLIME_SLOT_SKINS: [Option<Skin>; 46] = [
    Some(Skin::Slime),
    Some(Skin::MagmaCube),
    Some(Skin::Blaze),
    Some(Skin::Ghast),
    Some(Skin::GhastShooting),
    Some(Skin::Bat),
    Some(Skin::Firework),
    Some(Skin::Wolf),
    Some(Skin::WolfTame),
    Some(Skin::WolfAngry),
    Some(Skin::WolfCollar),
    Some(Skin::Ocelot),
    Some(Skin::CatBlack),
    Some(Skin::CatRed),
    Some(Skin::CatSiamese),
    Some(Skin::RabbitBrown),
    Some(Skin::RabbitWhite),
    Some(Skin::RabbitBlack),
    Some(Skin::RabbitWhiteSplotched),
    Some(Skin::RabbitGold),
    Some(Skin::RabbitSalt),
    Some(Skin::RabbitCaerbannog),
    Some(Skin::RabbitToast),
    Some(Skin::Boat),
    Some(Skin::Minecart),
    Some(Skin::Squid),
    Some(Skin::EnderCrystal),
    Some(Skin::Mooshroom),
    Some(Skin::EnderEye),
    Some(Skin::XpBottle),
    Some(Skin::Wither),
    Some(Skin::WitherInvulnerable),
    Some(Skin::Snowman),
    Some(Skin::Witch),
    // The second half of the 64x128 witch texture.
    None,
    Some(Skin::LeadKnot),
    Some(Skin::Endermite),
    Some(Skin::Guardian),
    Some(Skin::Dragon),
    // The other three slots of the 256x256 dragon texture.
    None,
    None,
    None,
    Some(Skin::DragonEyes),
    None,
    None,
    None,
];
const SLIME_GEL: (usize, usize) = (32, 16);
/// Index of the first armor layer in `SKINS`.
const ARMOR_FIRST: u8 = 15;
/// Slot of `guardian_elder.png`, the last one before the mob textures.
const ELDER_SLOT: u32 = MOBS_FIRST as u32 - 1;

/// Slots from this index on hold sprites and projectile textures, not player-layout skins.
const FIRST_SPRITE: usize = 8;
/// The fishing bobber is the 8x8 cell at (8, 16) of `particles.png` (`FishingBobberRenderer`).
const BOBBER_SLOT: usize = 13;
const BOBBER_CELL: (usize, usize, usize) = (8, 16, 8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skin {
    Steve,
    Alex,
    Zombie,
    Skeleton,
    PigZombie,
    Creeper,
    Pig,
    Cow,
    /// `textures/entity/arrow.png` (32x32).
    Arrow,
    Snowball,
    Egg,
    EnderPearl,
    /// `items/fireball.png`, the fire charge icon.
    FireCharge,
    /// The bobber cell of `particles.png`, cropped to 8x8.
    Bobber,
    SplashPotion,
    /// `textures/entity/guardian_elder.png`, drawn by the curse effect.
    GuardianElder,
    /// `textures/entity/villager/`: 0 villager, then farmer, librarian, priest, smith, butcher.
    Villager(u8),
    IronGolem,
    Silverfish,
    Chicken,
    Sheep,
    SheepFur,
    Spider,
    CaveSpider,
    Enderman,
    SpiderEyes,
    EndermanEyes,
    /// `textures/entity/slime/slime.png`, the body and the outer gel layer.
    Slime,
    MagmaCube,
    Blaze,
    Ghast,
    /// `ghast_shooting.png`, drawn while the ghast charges a fireball.
    GhastShooting,
    /// `textures/entity/bat.png` (64x64).
    Bat,
    /// `textures/entity/wolf/`: wild, tame and angry skins, and the collar layer drawn over a tame wolf.
    Wolf,
    WolfTame,
    WolfAngry,
    WolfCollar,
    /// `textures/entity/cat/`: the wild ocelot and the three tame cats.
    Ocelot,
    CatBlack,
    CatRed,
    CatSiamese,
    /// `textures/entity/rabbit/`: the six types, the killer bunny (`caerbannog`) and the rabbit named Toast.
    RabbitBrown,
    RabbitWhite,
    RabbitBlack,
    RabbitWhiteSplotched,
    RabbitGold,
    RabbitSalt,
    RabbitCaerbannog,
    RabbitToast,
    /// `textures/entity/boat.png` and `minecart.png` (64x32 each).
    Boat,
    Minecart,
    /// `textures/entity/squid.png` (64x32).
    Squid,
    /// `textures/entity/endercrystal/endercrystal.png` (64x32).
    EnderCrystal,
    /// `textures/entity/cow/mooshroom.png` (64x32).
    Mooshroom,
    /// `items/ender_eye.png` and `items/experience_bottle.png`, the `RenderSnowball` sprites.
    EnderEye,
    XpBottle,
    /// `textures/entity/wither/wither.png` and `wither_invulnerable.png` (64x64), the wither skull.
    Wither,
    WitherInvulnerable,
    /// `textures/entity/snowman.png` (64x64).
    Snowman,
    /// `textures/entity/witch.png` (64x128).
    Witch,
    /// `textures/entity/lead_knot.png` (32x32), the knot of a leash on a fence.
    LeadKnot,
    /// `textures/entity/endermite.png` (64x32).
    Endermite,
    /// `textures/entity/guardian.png` (64x64).
    Guardian,
    /// `textures/entity/enderdragon/dragon.png` (256x256, four slots).
    Dragon,
    /// `textures/entity/enderdragon/dragon_eyes.png` (256x256, four slots), the additive eye layer.
    DragonEyes,
    /// `textures/entity/armorstand/wood.png` (64x64).
    ArmorStand,
    /// A horse texture (128x128), by index into [`horse`]'s table.
    Horse(u8),
    /// `items/fireworks.png`, the rocket icon.
    Firework,
    /// A downloaded cape in dynamic slot `n`.
    Cape(u8),
    /// An armor texture, by index into the armor part of the slot table.
    Armor(u8),
    /// Plain white: shapes take their color from the vertex tint.
    White,
    /// A downloaded player skin in dynamic slot `n`.
    Player(u8),
}

/// Fixed slots, the horse textures after them, and the white one.
pub const SLOTS: u32 = SKINS.len() as u32 + horse::SLOTS + 1;
/// Downloaded player skins kept at once; the least recently drawn one is replaced.
pub const PLAYER_SLOTS: usize = 32;
/// Downloaded capes kept at once.
pub const CAPE_SLOTS: usize = 8;
const TOTAL: u32 = SLOTS + (PLAYER_SLOTS + CAPE_SLOTS) as u32;
/// Columns of the atlas texture, so that none of its sides passes [`MAX_SIDE`].
const COLUMNS: u32 = (TOTAL * SLOT).div_ceil(MAX_SIDE);
/// Slots in one column. The horse textures take two slots each from slot `SKINS.len()` on, so the
/// column ends where such a pair ends: its length has the parity of `SKINS.len()`.
const COLUMN_SLOTS: u32 = {
    let odd = SKINS.len() as u32 % 2;
    (TOTAL.div_ceil(COLUMNS) - odd).next_multiple_of(2) + odd
};

pub struct Skins {
    pub rgba: Vec<u8>,
    loaded: [bool; SKINS.len()],
    horse_loaded: [bool; horse::COUNT],
    /// Owner key and last-use tick of each dynamic slot.
    players: [Option<(u128, u64)>; PLAYER_SLOTS],
    capes: [Option<(u128, u64)>; CAPE_SLOTS],
    clock: u64,
}

impl Skin {
    /// The texture of an armor item (`ArmorLayer`): `layer` 1 covers head, chest, arms and boots, 2
    /// the leggings. `overlay` is the undyed part of leather armor. `None` for any other item.
    pub fn armor(item: i16, layer: u8, overlay: bool) -> Option<Skin> {
        let material = match item {
            298..=317 => (item - 298) / 4,
            _ => return None,
        } as u8;
        let layer = layer - 1;
        Some(Skin::Armor(match (overlay, material) {
            (true, 0) => 10 + layer,
            (true, _) => return None,
            // The table orders the materials leather, chainmail, iron, diamond, gold.
            (false, m) => m * 2 + layer,
        }))
    }
}

impl Skins {
    /// Width of the atlas texture in texels.
    pub fn width() -> u32 {
        WIDTH * COLUMNS
    }

    /// Height of the atlas texture in texels.
    pub fn height() -> u32 {
        SLOT * COLUMN_SLOTS
    }

    /// Texels in one row of [`Skins::rgba`], which keeps the slots stacked in one column.
    pub fn row_width() -> u32 {
        WIDTH
    }

    /// Top-left texel of `slot` in the atlas texture. This is the only place that maps slots to the
    /// columns of the texture.
    fn origin(slot: u32) -> [u32; 2] {
        [slot / COLUMN_SLOTS * WIDTH, slot % COLUMN_SLOTS * SLOT]
    }

    /// Position of the slot of [`Skins::v_offset`] in the atlas texture, as a fraction of its size.
    /// Texture coordinates of a model texture add it to their own fractions.
    pub fn uv_offset(v_off: f32) -> [f32; 2] {
        let [x, y] = Self::origin(v_off as u32 / SLOT);
        [
            x as f32 / Self::width() as f32,
            y as f32 / Self::height() as f32,
        ]
    }

    /// [`Skins::rgba`] laid out as the atlas texture.
    pub fn atlas(&self) -> Vec<u8> {
        let (w, row) = (Self::width() as usize * 4, WIDTH as usize * 4);
        let mut out = vec![0; w * Self::height() as usize];
        for slot in 0..TOTAL {
            let [x, y] = Self::origin(slot);
            for r in 0..SLOT as usize {
                let from = (slot as usize * SLOT as usize + r) * row;
                let to = (y as usize + r) * w + x as usize * 4;
                out[to..to + row].copy_from_slice(&self.rgba[from..from + row]);
            }
        }
        out
    }

    /// All slots white: models draw flat-colored.
    pub fn flat() -> Skins {
        Skins {
            rgba: vec![255; (WIDTH * SLOT * TOTAL * 4) as usize],
            loaded: [false; SKINS.len()],
            horse_loaded: [false; horse::COUNT],
            players: [None; PLAYER_SLOTS],
            capes: [None; CAPE_SLOTS],
            clock: 0,
        }
    }

    /// Stores the skin of player `key` in its slot, or in a free or the least recently used one.
    /// Returns the texel rows of the slot that changed.
    pub fn set_player(&mut self, key: u128, img: &Image) -> ([u32; 2], &[u8]) {
        let i = self
            .players
            .iter()
            .position(|p| p.is_some_and(|(k, _)| k == key))
            .or_else(|| self.players.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                (0..PLAYER_SLOTS)
                    .min_by_key(|&i| self.players[i].map_or(0, |p| p.1))
                    .unwrap_or(0)
            });
        self.clock += 1;
        self.players[i] = Some((key, self.clock));
        let slot = SLOTS + i as u32;
        self.blit(slot, img);
        self.fix_alpha(slot);
        (Self::origin(slot), self.slot_rows(slot))
    }

    /// Stores the cape of player `key`, like [`Skins::set_player`]; no skin fixes apply to a cape.
    pub fn set_cape(&mut self, key: u128, img: &Image) -> ([u32; 2], &[u8]) {
        let i = self
            .capes
            .iter()
            .position(|p| p.is_some_and(|(k, _)| k == key))
            .or_else(|| self.capes.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                (0..CAPE_SLOTS)
                    .min_by_key(|&i| self.capes[i].map_or(0, |p| p.1))
                    .unwrap_or(0)
            });
        self.clock += 1;
        self.capes[i] = Some((key, self.clock));
        let slot = SLOTS + (PLAYER_SLOTS + i) as u32;
        self.blit_plain(slot, img);
        (Self::origin(slot), self.slot_rows(slot))
    }

    /// The cape slot of player `key`, when one is stored.
    pub fn cape(&self, key: u128) -> Option<Skin> {
        let i = self
            .capes
            .iter()
            .position(|p| p.is_some_and(|(k, _)| k == key))?;
        Some(Skin::Cape(i as u8))
    }

    fn slot_rows(&self, slot: u32) -> &[u8] {
        let from = (slot * SLOT * WIDTH * 4) as usize;
        &self.rgba[from..from + (SLOT * WIDTH * 4) as usize]
    }

    /// `ImageBufferDownload.parseUserSkin`: the base layers are forced opaque. An outer layer that
    /// is fully opaque is an image editor without alpha, so it becomes transparent.
    fn fix_alpha(&mut self, slot: u32) {
        let base = (slot * SLOT) as usize;
        let px = |x: usize, y: usize| (base + y) * WIDTH as usize * 4 + x * 4 + 3;
        for (x0, y0, x1, y1, opaque) in [
            (0, 0, 32, 16, true),
            (32, 0, 64, 32, false),
            (0, 16, 64, 32, true),
            (0, 32, 16, 48, false),
            (16, 32, 40, 48, false),
            (40, 32, 56, 48, false),
            (0, 48, 16, 64, false),
            (16, 48, 48, 64, true),
            (48, 48, 64, 64, false),
        ] {
            let area = || (x0..x1).flat_map(move |x| (y0..y1).map(move |y| (x, y)));
            if opaque {
                for (x, y) in area() {
                    let i = px(x, y);
                    self.rgba[i] = 255;
                }
            } else if !area().any(|(x, y)| self.rgba[px(x, y)] < 128) {
                for (x, y) in area() {
                    let i = px(x, y);
                    self.rgba[i] = 0;
                }
            }
        }
    }

    /// The slot of player `key`'s skin, when one is stored.
    pub fn player(&self, key: u128) -> Option<Skin> {
        let i = self
            .players
            .iter()
            .position(|p| p.is_some_and(|(k, _)| k == key))?;
        Some(Skin::Player(i as u8))
    }

    /// Marks the skin of player `key` as drawn now.
    pub fn touch(&mut self, key: u128) {
        if let Some(Skin::Player(i)) = self.player(key) {
            self.clock += 1;
            self.players[i as usize] = Some((key, self.clock));
        }
    }

    pub fn from_assets(source: &mut AssetSource) -> Skins {
        let mut s = Skins::flat();
        for (i, name) in SKINS.iter().enumerate().filter(|(_, n)| !n.is_empty()) {
            let path = format!("assets/minecraft/textures/{name}");
            let Some(img) = source.read(&path).and_then(|b| decode_png(&b)) else {
                continue;
            };
            if i == BOBBER_SLOT {
                s.blit_plain(i as u32, &crop(&img, BOBBER_CELL));
            } else if (FIRST_SPRITE..ARMOR_FIRST as usize).contains(&i) || i >= MOBS_FIRST {
                s.blit_plain(i as u32, &img);
                if EYES.contains(&i) {
                    s.clear_black(i as u32, img.height);
                }
                if i == SLIME_SLOT {
                    s.screen_door(i as u32, SLIME_GEL);
                }
            } else {
                s.blit(i as u32, &img);
            }
            s.loaded[i] = true;
        }
        s.load_horses(source);
        s
    }

    /// Copies a sprite into the top-left corner of its slot, with no skin-layout fixes. A sprite
    /// taller than a slot (the iron golem) runs on into the next slot.
    fn blit_plain(&mut self, slot: u32, img: &Image) {
        let base = (slot * SLOT) as usize;
        let w = WIDTH as usize;
        let rows = img.height.max(SLOT as usize);
        self.rgba[base * w * 4..(base + rows) * w * 4].fill(0);
        for y in 0..img.height {
            let n = img.width.min(w);
            let dst = ((base + y) * w) * 4;
            self.rgba[dst..dst + n * 4].copy_from_slice(&img.rgba[y * img.width * 4..][..n * 4]);
        }
    }

    /// Makes every other texel of the top left `w` by `h` region of a slot transparent, in a checkerboard,
    /// so a translucent layer drawn without blending lets what lies behind it show.
    fn screen_door(&mut self, slot: u32, (w, h): (usize, usize)) {
        let base = (slot * SLOT) as usize;
        for y in 0..h {
            for x in (y % 2..w).step_by(2) {
                self.rgba[((base + y) * WIDTH as usize + x) * 4 + 3] = 0;
            }
        }
    }

    /// Makes the black texels of a slot, and of the `rows` texel rows from it, transparent: an eye layer
    /// is drawn additively in vanilla, where black adds nothing.
    fn clear_black(&mut self, slot: u32, rows: usize) {
        let base = (slot * SLOT) as usize * WIDTH as usize * 4;
        for px in self.rgba[base..base + rows.max(SLOT as usize) * WIDTH as usize * 4]
            .as_chunks_mut::<4>()
            .0
        {
            if px[..3] == [0, 0, 0] {
                px[3] = 0;
            }
        }
    }

    /// Copies a skin into its slot. A legacy 64x32 skin also fills the left arm and leg regions
    /// of the 64x64 layout, flipping each face as `ImageBufferDownload` does.
    fn blit(&mut self, slot: u32, img: &Image) {
        let base = (slot * SLOT) as usize;
        let w = WIDTH as usize;
        self.rgba[base * w * 4..(base + SLOT as usize) * w * 4].fill(0);
        for y in 0..img.height.min(SLOT as usize) {
            let n = img.width.min(SLOT as usize);
            let dst = ((base + y) * w) * 4;
            self.rgba[dst..dst + n * 4].copy_from_slice(&img.rgba[y * img.width * 4..][..n * 4]);
        }
        if img.height <= 32 {
            // (source x, source y, destination x, rows) of each 4-wide face; the left limbs sit 32 rows down.
            const FACES: [(usize, usize, usize, usize); 12] = [
                (4, 16, 20, 4),
                (8, 16, 24, 4),
                (0, 20, 24, 12),
                (4, 20, 20, 12),
                (8, 20, 16, 12),
                (12, 20, 28, 12),
                (44, 16, 36, 4),
                (48, 16, 40, 4),
                (40, 20, 40, 12),
                (44, 20, 36, 12),
                (48, 20, 32, 12),
                (52, 20, 44, 12),
            ];
            for (sx, sy, dx, rows) in FACES {
                for y in 0..rows {
                    for x in 0..4 {
                        let from = ((base + sy + y) * w + sx + x) * 4;
                        let to = ((base + sy + 32 + y) * w + dx + 3 - x) * 4;
                        let px: [u8; 4] = self.rgba[from..from + 4].try_into().unwrap();
                        self.rgba[to..to + 4].copy_from_slice(&px);
                    }
                }
            }
        }
    }

    /// Whether the real texture of `skin` was found; a missing one is drawn white with a tint.
    pub fn has(&self, skin: Skin) -> bool {
        matches!(skin, Skin::Player(_) | Skin::Cape(_))
            || slot(skin).is_some_and(|s| self.loaded(s))
    }

    /// Whether fixed slot `s` holds its real texture.
    fn loaded(&self, s: u32) -> bool {
        let s = s as usize;
        match s.checked_sub(SKINS.len()) {
            None => self.loaded[s],
            Some(h) => self.horse_loaded[h / 2],
        }
    }

    /// Key of the slot to sample for `skin`: its row in a column of all slots. Pass it as `v_off`
    /// to `add_box`, or to [`Skins::uv_offset`], never add it to a texel row.
    pub fn v_offset(&self, skin: Skin) -> f32 {
        let s = match slot(skin) {
            Some(s) if s >= SLOTS || self.loaded(s) => s,
            _ => SLOTS - 1,
        };
        (s * SLOT) as f32
    }
}

/// The `size` x `size` square at (`x`, `y`) of `img`; missing rows and columns stay transparent.
fn crop(img: &Image, (x, y, size): (usize, usize, usize)) -> Image {
    let mut rgba = vec![0; size * size * 4];
    for row in 0..size.min(img.height.saturating_sub(y)) {
        let n = size.min(img.width.saturating_sub(x));
        let from = ((y + row) * img.width + x) * 4;
        rgba[row * size * 4..][..n * 4].copy_from_slice(&img.rgba[from..from + n * 4]);
    }
    Image {
        width: size,
        height: size,
        rgba,
    }
}

/// Skins in `SKINS` order, before the armor layers.
const FIXED: [Skin; ARMOR_FIRST as usize] = [
    Skin::Steve,
    Skin::Alex,
    Skin::Zombie,
    Skin::Skeleton,
    Skin::PigZombie,
    Skin::Creeper,
    Skin::Pig,
    Skin::Cow,
    Skin::Arrow,
    Skin::Snowball,
    Skin::Egg,
    Skin::EnderPearl,
    Skin::FireCharge,
    Skin::Bobber,
    Skin::SplashPotion,
];

fn slot(skin: Skin) -> Option<u32> {
    mob_slot(skin).or(match skin {
        Skin::Player(i) => Some(SLOTS + u32::from(i)),
        Skin::Cape(i) => Some(SLOTS + PLAYER_SLOTS as u32 + u32::from(i)),
        Skin::White => None,
        Skin::Armor(i) => Some(u32::from(ARMOR_FIRST + i)),
        Skin::Horse(i) => Some(horse::slot(i)),
        Skin::GuardianElder => Some(ELDER_SLOT),
        fixed => FIXED.iter().position(|&s| s == fixed).map(|i| i as u32),
    })
}

/// Slots of the mob textures that come after the elder guardian, in `SKINS` order.
fn mob_slot(skin: Skin) -> Option<u32> {
    Some(match skin {
        Skin::Villager(i) => MOBS_FIRST as u32 + u32::from(i),
        Skin::IronGolem => 34,
        Skin::Silverfish => 36,
        Skin::Chicken => 37,
        Skin::Sheep => 38,
        Skin::SheepFur => 39,
        Skin::Spider => 40,
        Skin::CaveSpider => 41,
        Skin::Enderman => 42,
        Skin::SpiderEyes => 43,
        Skin::EndermanEyes => 44,
        Skin::ArmorStand => 45,
        _ => {
            return SLIME_SLOT_SKINS
                .iter()
                .position(|&s| s == Some(skin))
                .map(|i| (SLIME_SLOT + i) as u32)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thrown_item_sprites_use_the_slots_of_their_vanilla_textures() {
        let index = |path: &str| SKINS.iter().position(|&n| n == path).unwrap() as u32;
        assert_eq!(slot(Skin::EnderEye), Some(index("items/ender_eye.png")));
        assert_eq!(
            slot(Skin::XpBottle),
            Some(index("items/experience_bottle.png"))
        );
    }

    #[test]
    fn mob_slots_hold_their_own_textures() {
        let index = |path: &str| SKINS.iter().position(|&n| n == path).unwrap() as u32;
        assert_eq!(slot(Skin::Witch), Some(index("entity/witch.png")));
        assert_eq!(slot(Skin::LeadKnot), Some(index("entity/lead_knot.png")));
        assert_eq!(slot(Skin::Endermite), Some(index("entity/endermite.png")));
    }

    #[test]
    fn player_slots_replace_the_least_recently_used_skin() {
        let img = Image {
            width: 64,
            height: 64,
            rgba: vec![255; 64 * 64 * 4],
        };
        let mut skins = Skins::flat();
        for key in 0..PLAYER_SLOTS as u128 {
            skins.set_player(key, &img);
        }
        skins.touch(0);
        skins.set_player(99, &img);
        assert!(skins.player(0).is_some());
        assert!(skins.player(1).is_none());
        assert_eq!(skins.player(99), Some(Skin::Player(1)));
    }

    #[test]
    fn atlas_stays_within_the_gpu_limit_with_all_dynamic_slots() {
        assert!(Skins::width() <= MAX_SIDE && Skins::height() <= MAX_SIDE);
        const { assert!(COLUMNS * COLUMN_SLOTS >= TOTAL) };
    }

    #[test]
    fn two_slot_textures_stay_in_one_column() {
        let golem = slot(Skin::IronGolem).unwrap();
        let horses = (0..horse::COUNT as u32).map(|i| SKINS.len() as u32 + 2 * i);
        for s in std::iter::once(golem).chain(horses) {
            assert_eq!(Skins::origin(s)[0], Skins::origin(s + 1)[0], "slot {s}");
        }
    }

    #[test]
    fn every_slot_has_its_own_rect_inside_the_atlas() {
        let mut seen = std::collections::HashSet::new();
        for s in 0..TOTAL {
            let [x, y] = Skins::origin(s);
            assert!(
                x + WIDTH <= Skins::width() && y + SLOT <= Skins::height(),
                "slot {s}"
            );
            assert!(seen.insert([x, y]), "slot {s} shares a rect");
        }
    }

    #[test]
    fn offsets_of_the_last_slot_point_into_the_second_column() {
        let last = Skins::uv_offset(((TOTAL - 1) * SLOT) as f32);
        let w = Skins::width() as f32;
        assert_eq!(last[0], WIDTH as f32 / w);
        assert_eq!(Skins::uv_offset(0.0), [0.0, 0.0]);
    }

    #[test]
    fn legacy_skin_left_limb_faces_are_flipped_one_by_one() {
        // Every source texel holds its own (x, y), so the copied position is readable.
        let mut rgba = Vec::new();
        for y in 0..32u8 {
            for x in 0..64u8 {
                rgba.extend_from_slice(&[x, y, 0, 255]);
            }
        }
        let img = Image {
            width: 64,
            height: 32,
            rgba,
        };
        let mut skins = Skins::flat();
        skins.blit(0, &img);
        let at = |x: usize, y: usize| {
            let i = (y * WIDTH as usize + x) * 4;
            (skins.rgba[i], skins.rgba[i + 1])
        };
        // ImageBufferDownload: right leg front (4,20) goes to the left leg front (20,52), flipped.
        assert_eq!(at(20, 52), (7, 20));
        assert_eq!(at(23, 63), (4, 31));
        // Right leg right face (0,20) becomes the left leg's left face (24,52).
        assert_eq!(at(24, 52), (3, 20));
        // Right arm front (44,20) goes to the left arm front (36,52); its top (44,16) to (36,48).
        assert_eq!(at(36, 52), (47, 20));
        assert_eq!(at(36, 48), (47, 16));
    }
}
