//! `AchievementList`, `StatFileWriter`'s achievement queries and `GuiAchievements`, the
//! achievement tree opened from the Escape menu. Ported from the 1.8.9 client; the table comes
//! from the real `AchievementList` of the oracle jar.

use std::collections::HashMap;

use render::JavaRandom;
use world::inventory::ItemStack;

use super::{
    centered, default_background, draw_buttons, key, key_name, press, release_all, wrap, Button,
    Canvas, Ctx, Effect, Metrics, Mods, Nav, Screen, WHITE,
};

/// One `Achievement`: its place in the tree, parent, frame style and item icon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Achievement {
    /// The id after `achievement.` in the stat name and the language keys.
    pub key: &'static str,
    pub column: i32,
    pub row: i32,
    pub parent: Option<&'static str>,
    /// `getSpecial`: the rounded frame.
    pub challenge: bool,
    pub icon: (i16, i16),
}

const fn a(
    key: &'static str,
    column: i32,
    row: i32,
    parent: Option<&'static str>,
    challenge: bool,
    id: i16,
    damage: i16,
) -> Achievement {
    Achievement {
        key,
        column,
        row,
        parent,
        challenge,
        icon: (id, damage),
    }
}

/// `AchievementList.achievementList`, in registration order: a parent comes before its children.
pub const ACHIEVEMENTS: [Achievement; 34] = [
    a("openInventory", 0, 0, None, false, 340, 0),
    a("mineWood", 2, 1, Some("openInventory"), false, 17, 0),
    a("buildWorkBench", 4, -1, Some("mineWood"), false, 58, 0),
    a("buildPickaxe", 4, 2, Some("buildWorkBench"), false, 270, 0),
    a("buildFurnace", 3, 4, Some("buildPickaxe"), false, 61, 0),
    a("acquireIron", 1, 4, Some("buildFurnace"), false, 265, 0),
    a("buildHoe", 2, -3, Some("buildWorkBench"), false, 290, 0),
    a("makeBread", -1, -3, Some("buildHoe"), false, 297, 0),
    a("bakeCake", 0, -5, Some("buildHoe"), false, 354, 0),
    a(
        "buildBetterPickaxe",
        6,
        2,
        Some("buildPickaxe"),
        false,
        274,
        0,
    ),
    a("cookFish", 2, 6, Some("buildFurnace"), false, 350, 0),
    a("onARail", 2, 3, Some("acquireIron"), true, 66, 0),
    a("buildSword", 6, -1, Some("buildWorkBench"), false, 268, 0),
    a("killEnemy", 8, -1, Some("buildSword"), false, 352, 0),
    a("killCow", 7, -3, Some("buildSword"), false, 334, 0),
    a("flyPig", 9, -3, Some("killCow"), true, 329, 0),
    a("snipeSkeleton", 7, 0, Some("killEnemy"), true, 261, 0),
    a("diamonds", -1, 5, Some("acquireIron"), false, 56, 0),
    a("diamondsToYou", -1, 2, Some("diamonds"), false, 264, 0),
    a("portal", -1, 7, Some("diamonds"), false, 49, 0),
    a("ghast", -4, 8, Some("portal"), true, 370, 0),
    a("blazeRod", 0, 9, Some("portal"), false, 369, 0),
    a("potion", 2, 8, Some("blazeRod"), false, 373, 0),
    a("theEnd", 3, 10, Some("blazeRod"), true, 381, 0),
    a("theEnd2", 4, 13, Some("theEnd"), true, 122, 0),
    a("enchantments", -4, 4, Some("diamonds"), false, 116, 0),
    a("overkill", -4, 1, Some("enchantments"), true, 276, 0),
    a("bookcase", -3, 6, Some("enchantments"), false, 47, 0),
    a("breedCow", 7, -5, Some("killCow"), false, 296, 0),
    a("spawnWither", 7, 12, Some("theEnd2"), false, 397, 1),
    a("killWither", 7, 10, Some("spawnWither"), false, 399, 0),
    a("fullBeacon", 7, 8, Some("killWither"), true, 138, 0),
    a("exploreAllBiomes", 4, 8, Some("theEnd"), true, 313, 0),
    a(
        "overpowered",
        6,
        4,
        Some("buildBetterPickaxe"),
        true,
        322,
        1,
    ),
];

/// `AchievementList.minDisplayColumn` and its three siblings.
const MIN_COLUMN: i32 = -4;
const MIN_ROW: i32 = -5;
const MAX_COLUMN: i32 = 9;
const MAX_ROW: i32 = 13;

impl Achievement {
    fn by_key(key: &str) -> Option<&'static Achievement> {
        ACHIEVEMENTS.iter().find(|a| a.key == key)
    }

    fn parent(&self) -> Option<&'static Achievement> {
        self.parent.and_then(Achievement::by_key)
    }

    fn stat_name(&self) -> String {
        format!("achievement.{}", self.key)
    }
}

/// The statistics the server sent (`StatFileWriter`), by stat name such as `achievement.mineWood`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerStats(HashMap<String, i32>);

impl PlayerStats {
    /// `S37PacketStatistics`: the server value replaces the stored one.
    pub fn set(&mut self, name: &str, value: i32) {
        self.0.insert(name.to_owned(), value);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// `StatFileWriter.readStat`.
    pub fn read(&self, name: &str) -> i32 {
        self.0.get(name).copied().unwrap_or(0)
    }

    /// `hasAchievementUnlocked`.
    pub fn has(&self, a: &Achievement) -> bool {
        self.read(&a.stat_name()) > 0
    }

    /// `canUnlockAchievement`: no parent, or the parent is unlocked.
    pub fn can_unlock(&self, a: &Achievement) -> bool {
        a.parent().map_or(true, |p| self.has(p))
    }

    /// `func_150874_c`: 0 when unlocked, else how many locked parents lie before the first
    /// unlocked ancestor.
    pub fn depth(&self, a: &Achievement) -> i32 {
        if self.has(a) {
            return 0;
        }
        let mut n = 0;
        let mut p = a.parent();
        while let Some(parent) = p.filter(|p| !self.has(p)) {
            p = parent.parent();
            n += 1;
        }
        n
    }
}

const BACKGROUND: &str = "textures/gui/achievement/achievement_background.png";
/// `StatsListener.PROGRESS_BAR_STAGES`.
const PROGRESS: [&str; 7] = [
    "oooooo", "Oooooo", "oOoooo", "ooOooo", "oooOoo", "ooooOo", "oooooO",
];
const DONE: i32 = 1;
const WIDTH: i32 = 256;
const HEIGHT: i32 = 202;

/// `String.hashCode`.
fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(i32::from(u)))
}

/// `GlStateManager.color(f, f, f, 1)` as a tint.
fn gray(f: f32) -> u32 {
    let g = (f * 255.0).round() as u32;
    0xFF00_0000 | g << 16 | g << 8 | g
}

/// `Gui.drawTexturedModalRect` on the 256 x 256 background sheet.
fn sheet(c: &mut dyn Canvas, x: i32, y: i32, (u, v): (i32, i32), (w, h): (i32, i32), tint: u32) {
    let uv = [u, v, u + w, v + h].map(|n| n as f32 / 256.0);
    c.blit(BACKGROUND, x as f32, y as f32, w as f32, h as f32, uv, tint);
}

/// `Gui.drawHorizontalLine`.
fn hline(c: &mut dyn Canvas, x0: i32, x1: i32, y: i32, argb: u32) {
    c.fill(x0.min(x1), y, x0.max(x1) + 1, y + 1, argb);
}

/// `Gui.drawVerticalLine`.
fn vline(c: &mut dyn Canvas, x: i32, y0: i32, y1: i32, argb: u32) {
    c.fill(x, y0.min(y1) + 1, x + 1, y0.max(y1), argb);
}

/// The tile area as a clip rectangle. Vanilla draws the tiles with `GL_GEQUAL` over the menu
/// background's depth, then the links and icons with `GL_LEQUAL`, so only the tiles let them show.
struct Clip<'a> {
    inner: &'a mut dyn Canvas,
    /// `[x0, y0, x1, y1]` in the pushed coordinates.
    area: [f32; 4],
}

type Vertex = ([f32; 2], [f32; 2], [f32; 4]);

fn unpack(argb: u32) -> [f32; 4] {
    argb.to_be_bytes().map(f32::from)
}

fn pack(c: [f32; 4]) -> u32 {
    u32::from_be_bytes(c.map(|v| v.round().clamp(0.0, 255.0) as u8))
}

/// Sutherland-Hodgman against one side of the rectangle; `keep` tells the inside.
fn clip_side(poly: Vec<Vertex>, axis: usize, edge: f32, keep: fn(f32, f32) -> bool) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for (n, a) in poly.iter().enumerate() {
        let b = &poly[(n + 1) % poly.len()];
        let (ain, bin) = (keep(a.0[axis], edge), keep(b.0[axis], edge));
        if ain {
            out.push(*a);
        }
        if ain != bin {
            let t = (edge - a.0[axis]) / (b.0[axis] - a.0[axis]);
            let mix = |p: f32, q: f32| p + (q - p) * t;
            out.push((
                [mix(a.0[0], b.0[0]), mix(a.0[1], b.0[1])],
                [mix(a.1[0], b.1[0]), mix(a.1[1], b.1[1])],
                [0, 1, 2, 3].map(|i| mix(a.2[i], b.2[i])),
            ));
        }
    }
    out
}

impl Clip<'_> {
    /// The part of a convex quad inside the area, as a fan of quads.
    fn clipped(&self, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: [u32; 4]) -> Vec<Vertex> {
        let mut poly: Vec<Vertex> = (0..4).map(|i| (pts[i], uv[i], unpack(argb[i]))).collect();
        let [x0, y0, x1, y1] = self.area;
        let sides: [(usize, f32, fn(f32, f32) -> bool); 4] = [
            (0, x0, |v, e| v >= e),
            (0, x1, |v, e| v <= e),
            (1, y0, |v, e| v >= e),
            (1, y1, |v, e| v <= e),
        ];
        for (axis, edge, keep) in sides {
            if poly.is_empty() {
                break;
            }
            poly = clip_side(poly, axis, edge, keep);
        }
        poly
    }

    fn emit(&mut self, tex: &str, poly: &[Vertex], additive: bool, tint: Option<u32>) {
        for k in 1..poly.len().saturating_sub(1) {
            let v = [poly[0], poly[k], poly[k + 1], poly[k + 1]];
            let pts = v.map(|v| v.0);
            let uv = v.map(|v| v.1);
            let argb = v.map(|v| pack(v.2));
            match (additive, tint) {
                (true, Some(t)) => self.inner.quad_add(tex, pts, uv, t),
                _ => self.inner.quad(tex, pts, uv, argb),
            }
        }
    }
}

impl Metrics for Clip<'_> {
    fn width(&self, s: &str) -> i32 {
        self.inner.width(s)
    }
}

impl Canvas for Clip<'_> {
    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, argb: u32) {
        let [ax0, ay0, ax1, ay1] = self.area;
        let (l, r) = ((x0.min(x1) as f32).max(ax0), (x0.max(x1) as f32).min(ax1));
        let (t, b) = ((y0.min(y1) as f32).max(ay0), (y0.max(y1) as f32).min(ay1));
        if l < r && t < b {
            // The area edges are whole numbers: tile offsets are integers.
            self.inner
                .fill(l as i32, t as i32, r as i32, b as i32, argb);
        }
    }

    fn gradient(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, top: u32, bottom: u32) {
        self.inner.gradient(x0, y0, x1, y1, top, bottom);
    }

    fn blit(&mut self, tex: &str, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], argb: u32) {
        let pts = [[x, y], [x, y + h], [x + w, y + h], [x + w, y]];
        let uvs = [
            [uv[0], uv[1]],
            [uv[0], uv[3]],
            [uv[2], uv[3]],
            [uv[2], uv[1]],
        ];
        let poly = self.clipped(pts, uvs, [argb; 4]);
        self.emit(tex, &poly, false, None);
    }

    fn quad(&mut self, tex: &str, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: [u32; 4]) {
        let poly = self.clipped(pts, uv, argb);
        self.emit(tex, &poly, false, None);
    }

    fn quad_add(&mut self, tex: &str, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: u32) {
        let poly = self.clipped(pts, uv, [argb; 4]);
        self.emit(tex, &poly, true, Some(argb));
    }

    fn text(&mut self, s: &str, x: f32, y: f32, argb: u32, shadow: bool) {
        self.inner.text(s, x, y, argb, shadow);
    }

    fn set_bidi(&mut self, on: bool) {
        self.inner.set_bidi(on);
    }

    fn texture(&mut self, name: &str, png: &[u8]) {
        self.inner.texture(name, png);
    }

    fn push(&mut self, translate: [f32; 2], scale: f32, rotate_deg: f32) {
        self.inner.push(translate, scale, rotate_deg);
    }

    fn pop(&mut self) {
        self.inner.pop();
    }
}

/// `GuiAchievements`.
pub struct Achievements {
    parent: Option<Box<dyn Screen>>,
    buttons: Vec<Button>,
    /// Waiting for the server's `S37PacketStatistics`.
    downloading: bool,
    /// `field_146570_r`: 1 to 2, the zoom out factor.
    zoom: f32,
    /// The scroll position (`field_146569_s`), last tick's target (`field_146567_u`) and the
    /// target (`field_146566_v`), for each axis.
    pos: [f64; 2],
    prev: [f64; 2],
    target: [f64; 2],
    /// `field_146563_h`: 0 when no drag runs, 1 while one does.
    drag: u8,
    last_mouse: (i32, i32),
    left_down: bool,
    /// `Mouse.getDWheel`, collected until the next draw.
    wheel: i32,
}

impl Achievements {
    pub fn new(parent: Box<dyn Screen>) -> Achievements {
        let start = Achievement::by_key("openInventory").expect("openInventory is in the table");
        let x = f64::from(start.column * 24 - 141 / 2 - 12);
        let y = f64::from(start.row * 24 - 141 / 2);
        Achievements {
            parent: Some(parent),
            buttons: Vec::new(),
            downloading: true,
            zoom: 1.0,
            pos: [x, y],
            prev: [x, y],
            target: [x, y],
            drag: 0,
            last_mouse: (0, 0),
            left_down: false,
            wheel: 0,
        }
    }

    /// The scroll limits of `GuiAchievements` in tree pixels.
    const MIN: [i32; 2] = [MIN_COLUMN * 24 - 112, MIN_ROW * 24 - 112];
    const MAX: [i32; 2] = [MAX_COLUMN * 24 - 77, MAX_ROW * 24 - 77];

    fn set_all(&mut self) {
        self.pos = self.target;
        self.prev = self.target;
    }

    /// The drag, the zoom and the scroll limits of `drawScreen`.
    fn update_view(&mut self, mx: i32, my: i32, ctx: &Ctx) {
        let (i, j) = ((ctx.width - WIDTH) / 2, (ctx.height - HEIGHT) / 2);
        let (k, l) = (i + 8, j + 17);
        if self.left_down {
            let inside = (k..k + 224).contains(&mx) && (l..l + 155).contains(&my);
            if self.drag <= 1 && inside {
                if self.drag == 0 {
                    self.drag = 1;
                } else {
                    self.target[0] -= f64::from((mx - self.last_mouse.0) as f32 * self.zoom);
                    self.target[1] -= f64::from((my - self.last_mouse.1) as f32 * self.zoom);
                    self.set_all();
                }
                self.last_mouse = (mx, my);
            }
        } else {
            self.drag = 0;
        }
        let old = self.zoom;
        let wheel = std::mem::take(&mut self.wheel);
        if wheel < 0 {
            self.zoom += 0.25;
        } else if wheel > 0 {
            self.zoom -= 0.25;
        }
        self.zoom = self.zoom.clamp(1.0, 2.0);
        if self.zoom != old {
            let grow = [
                self.zoom * WIDTH as f32 - old * WIDTH as f32,
                self.zoom * HEIGHT as f32 - old * HEIGHT as f32,
            ];
            for (t, g) in self.target.iter_mut().zip(grow) {
                *t -= f64::from(g * 0.5);
            }
            self.set_all();
        }
        for axis in 0..2 {
            self.pos[axis] = self.pos[axis].max(f64::from(Self::MIN[axis]));
            if self.pos[axis] >= f64::from(Self::MAX[axis]) {
                self.pos[axis] = f64::from(Self::MAX[axis] - 1);
            }
        }
    }

    /// `renderIcons`: the tiled background, the links, the frames with their items and the
    /// hover text. `partial` is the part of the tick since the last one.
    fn draw_tree(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, partial: f32, ctx: &Ctx) {
        let view = [0, 1].map(|axis| {
            let at = self.prev[axis] + (self.target[axis] - self.prev[axis]) * f64::from(partial);
            (at.floor() as i32)
                .max(Self::MIN[axis])
                .min(Self::MAX[axis] - 1)
        });
        let [vx, vy] = view;
        let (i, j) = ((ctx.width - WIDTH) / 2, (ctx.height - HEIGHT) / 2);
        let (tx, ty) = (i + 16, j + 17);
        c.push([tx as f32, ty as f32], 1.0 / self.zoom, 0.0);
        let area = self.draw_background(c, vx, vy, ctx);
        let hover = {
            let mut clip = Clip {
                inner: &mut *c,
                area,
            };
            self.draw_links(&mut clip, vx, vy, ctx);
            self.draw_frames(&mut clip, vx, vy, (mx - tx, my - ty), ctx)
        };
        c.pop();
        sheet(c, i, j, (0, 0), (WIDTH, HEIGHT), 0xFFFF_FFFF);
        draw_buttons(&mut self.buttons, c, mx, my);
        let stats = ctx.stats;
        if let Some(a) = hover {
            Self::tooltip(c, a, mx, my, stats, ctx);
        }
    }

    /// The tile loops: sand, bedrock, ores, stone and dirt picked per tile from the player id.
    /// Returns the area the tiles cover: vanilla's depth buffer lets later drawing show only there.
    fn draw_background(&self, c: &mut dyn Canvas, vx: i32, vy: i32, ctx: &Ctx) -> [f32; 4] {
        let (col0, row0) = ((vx + 288) >> 4, (vy + 288) >> 4);
        let (off_x, off_y) = ((vx + 288) % 16, (vy + 288) % 16);
        let step = 16.0 / self.zoom;
        let seed = java_hash(ctx.player_id);
        let (mut i, mut cols) = (0, 0);
        while (i as f32 * step - off_y as f32) < 155.0 {
            let tint = gray(0.6 - (row0 + i) as f32 / 25.0 * 0.3);
            let mut j = 0;
            while (j as f32 * step - off_x as f32) < 224.0 {
                let s = seed
                    .wrapping_add(col0)
                    .wrapping_add(j)
                    .wrapping_add((row0 + i).wrapping_mul(16));
                let mut rand = JavaRandom::new(i64::from(s));
                let n = rand.next_int(1 + row0 + i) + (row0 + i) / 2;
                let tex = if n > 37 || row0 + i == 35 {
                    "textures/blocks/bedrock.png"
                } else if n == 22 {
                    if rand.next_int(2) == 0 {
                        "textures/blocks/diamond_ore.png"
                    } else {
                        "textures/blocks/redstone_ore.png"
                    }
                } else if n == 10 {
                    "textures/blocks/iron_ore.png"
                } else if n == 8 {
                    "textures/blocks/coal_ore.png"
                } else if n > 4 {
                    "textures/blocks/stone.png"
                } else if n > 0 {
                    "textures/blocks/dirt.png"
                } else {
                    "textures/blocks/sand.png"
                };
                let (x, y) = (j * 16 - off_x, i * 16 - off_y);
                c.blit(
                    tex,
                    x as f32,
                    y as f32,
                    16.0,
                    16.0,
                    [0.0, 0.0, 1.0, 1.0],
                    tint,
                );
                j += 1;
            }
            cols = j;
            i += 1;
        }
        [
            -off_x as f32,
            -off_y as f32,
            (cols * 16 - off_x) as f32,
            (i * 16 - off_y) as f32,
        ]
    }

    /// The lines and arrows from each achievement to its parent.
    fn draw_links(&self, c: &mut dyn Canvas, vx: i32, vy: i32, ctx: &Ctx) {
        for a in &ACHIEVEMENTS {
            let Some(p) = a.parent() else { continue };
            let (x, y) = (a.column * 24 - vx + 11, a.row * 24 - vy + 11);
            let (px, py) = (p.column * 24 - vx + 11, p.row * 24 - vy + 11);
            let stats = ctx.stats;
            if stats.depth(a) > 4 {
                continue;
            }
            let color = if stats.has(a) {
                0xFFA0_A0A0
            } else if stats.can_unlock(a) {
                0xFF00_FF00
            } else {
                0xFF00_0000
            };
            hline(c, x, px, y, color);
            vline(c, px, y, py, color);
            // The arrow sheet is drawn with the color the line left set.
            if x > px {
                sheet(c, x - 11 - 7, y - 5, (114, 234), (7, 11), color);
            } else if x < px {
                sheet(c, x + 11, y - 5, (107, 234), (7, 11), color);
            } else if y > py {
                sheet(c, x - 5, y - 11 - 7, (96, 234), (11, 7), color);
            } else if y < py {
                sheet(c, x - 5, y + 11, (96, 241), (11, 7), color);
            }
        }
    }

    /// The frames and item icons; returns the achievement under the mouse.
    fn draw_frames(
        &self,
        c: &mut dyn Canvas,
        vx: i32,
        vy: i32,
        mouse: (i32, i32),
        ctx: &Ctx,
    ) -> Option<&'static Achievement> {
        let stats = ctx.stats;
        let (mx, my) = (mouse.0 as f32 * self.zoom, mouse.1 as f32 * self.zoom);
        let mut hover = None;
        for a in &ACHIEVEMENTS {
            let (x, y) = (a.column * 24 - vx, a.row * 24 - vy);
            if x < -24 || y < -24 || x as f32 > 224.0 * self.zoom || y as f32 > 155.0 * self.zoom {
                continue;
            }
            let depth = stats.depth(a);
            let tint = if stats.has(a) {
                0.75
            } else if stats.can_unlock(a) {
                1.0
            } else if depth < 3 {
                0.3
            } else if depth == 3 {
                0.2
            } else if depth == 4 {
                0.1
            } else {
                continue;
            };
            let frame = if a.challenge { (26, 202) } else { (0, 202) };
            sheet(c, x - 2, y - 2, frame, (26, 26), gray(tint));
            if let Some(icons) = ctx.icons {
                let stack = ItemStack {
                    id: a.icon.0,
                    count: 1,
                    damage: a.icon.1,
                    nbt: None,
                };
                icons.draw(c, &stack, x + 3, y + 3);
            }
            if mx >= x as f32 && mx <= (x + 22) as f32 && my >= y as f32 && my <= (y + 22) as f32 {
                hover = Some(a);
            }
        }
        hover
    }

    /// The text box of the achievement under the mouse.
    fn tooltip(
        c: &mut dyn Canvas,
        a: &Achievement,
        mx: i32,
        my: i32,
        stats: &PlayerStats,
        ctx: &Ctx,
    ) {
        let name = ctx.tr(&a.stat_name());
        let (x, y) = (mx + 12, my - 4);
        let depth = stats.depth(a);
        let line_h = 9;
        let requires = |ctx: &Ctx| {
            let parent = a.parent().map(|p| ctx.tr(&p.stat_name()));
            ctx.texts
                .fmt("achievement.requires", &[parent.as_deref().unwrap_or("")])
        };
        let shown = if stats.can_unlock(a) {
            let w = c.width(&name).max(120);
            let desc = description(a, ctx);
            let lines = wrap(ctx.metrics, &desc, w);
            let mut h = lines.len() as i32 * line_h;
            if stats.has(a) {
                h += 12;
            }
            c.gradient(
                x - 3,
                y - 3,
                x + w + 3,
                y + h + 3 + 12,
                0xC000_0000,
                0xC000_0000,
            );
            for (n, line) in lines.iter().enumerate() {
                c.text(
                    line,
                    x as f32,
                    (y + 12 + n as i32 * line_h) as f32,
                    0xFFA0_A0A0,
                    false,
                );
            }
            if stats.has(a) {
                let taken = ctx.tr("achievement.taken");
                c.text(&taken, x as f32, (y + h + 4) as f32, 0xFF90_90FF, true);
            }
            Some(name)
        } else if depth <= 3 {
            let name = if depth == 3 {
                ctx.tr("achievement.unknown")
            } else {
                name
            };
            let w = c.width(&name).max(120);
            let lines = wrap(ctx.metrics, &requires(ctx), w);
            let h = lines.len() as i32 * line_h;
            c.gradient(
                x - 3,
                y - 3,
                x + w + 3,
                y + h + 12 + 3,
                0xC000_0000,
                0xC000_0000,
            );
            for (n, line) in lines.iter().enumerate() {
                c.text(
                    line,
                    x as f32,
                    (y + 12 + n as i32 * line_h) as f32,
                    0xFF70_5050,
                    false,
                );
            }
            Some(name)
        } else {
            None
        };
        if let Some(name) = shown {
            let color = match (stats.can_unlock(a), a.challenge) {
                (true, true) => 0xFFFF_FF80,
                (true, false) => 0xFFFF_FFFF,
                (false, true) => 0xFF80_8040,
                (false, false) => 0xFF80_8080,
            };
            c.text(&name, x as f32, y as f32, color, true);
        }
    }
}

/// `Achievement.getDescription`: only `openInventory` formats its text, with the inventory key.
fn description(a: &Achievement, ctx: &Ctx) -> String {
    let key = format!("{}.desc", a.stat_name());
    if a.key == "openInventory" {
        let code = ctx
            .settings
            .keys
            .iter()
            .find(|k| k.desc == "key.inventory")
            .map_or(18, |k| k.code);
        ctx.texts.fmt(&key, &[&key_name(code, ctx.texts)])
    } else {
        ctx.tr(&key)
    }
}

impl Screen for Achievements {
    fn init(&mut self, ctx: &mut Ctx) {
        // `getNetHandler().addToSendQueue(new C16PacketClientStatus(REQUEST_STATS))`.
        ctx.effects.push(Effect::RequestStats);
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = vec![Button::new(
            DONE,
            w / 2 + 24,
            h / 2 + 74,
            80,
            20,
            ctx.tr("gui.done"),
        )];
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        if self.downloading {
            default_background(c, ctx);
            let (cx, cy) = (ctx.width / 2, ctx.height / 2);
            centered(c, &ctx.tr("multiplayer.downloadingStats"), cx, cy, WHITE);
            let stage = PROGRESS[(ctx.now_ms / 150 % PROGRESS.len() as u64) as usize];
            centered(c, stage, cx, cy + 9 * 2, WHITE);
            return;
        }
        self.update_view(mx, my, ctx);
        default_background(c, ctx);
        // shortcut: the screens get no partial tick, so it comes from the clock at 20 ticks a second.
        let partial = (ctx.now_ms % 50) as f32 / 50.0;
        self.draw_tree(c, mx, my, partial, ctx);
        let (i, j) = ((ctx.width - WIDTH) / 2, (ctx.height - HEIGHT) / 2);
        c.text(
            &ctx.tr("gui.achievements"),
            (i + 15) as f32,
            (j + 5) as f32,
            0xFF40_4040,
            false,
        );
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> Option<Nav> {
        if self.downloading {
            return None;
        }
        self.prev = self.target;
        let d = [self.pos[0] - self.target[0], self.pos[1] - self.target[1]];
        let near = d[0] * d[0] + d[1] * d[1] < 4.0;
        for (t, d) in self.target.iter_mut().zip(d) {
            *t += if near { d } else { d * 0.85 };
        }
        None
    }

    fn key(&mut self, code: i32, _ch: char, _mods: Mods, ctx: &mut Ctx) -> Option<Nav> {
        let inventory = ctx
            .settings
            .keys
            .iter()
            .find(|k| k.desc == "key.inventory")
            .map_or(18, |k| k.code);
        (code == inventory || code == key::ESCAPE).then_some(Nav::Game)
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        if mb == 0 {
            self.left_down = true;
        }
        if press(&mut self.buttons, x, y, mb, ctx)? == DONE && !self.downloading {
            return self.parent.take().map(Nav::Open);
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, b: i32, _ctx: &mut Ctx) {
        if b == 0 {
            self.left_down = false;
        }
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        self.wheel += dir;
    }

    fn pauses(&self) -> bool {
        !self.downloading
    }

    fn stats_ready(&mut self) {
        self.downloading = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parent links, as the real `AchievementList` registers them.
    #[test]
    fn every_parent_exists_and_comes_first() {
        for (n, a) in ACHIEVEMENTS.iter().enumerate() {
            if let Some(p) = a.parent {
                let at = ACHIEVEMENTS.iter().position(|b| b.key == p);
                assert!(at.is_some_and(|at| at < n), "{} -> {p}", a.key);
            }
        }
        let roots: Vec<_> = ACHIEVEMENTS.iter().filter(|a| a.parent.is_none()).collect();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].key, "openInventory");
    }

    /// The table and its display bounds, as the real `AchievementList` registers them.
    #[test]
    fn table_matches_the_vanilla_achievement_list() {
        let lines = oracle::Oracle::get().run("achievements\n").unwrap();
        let (table, bounds) = lines.split_at(lines.len() - 1);
        let want: Vec<String> = ACHIEVEMENTS
            .iter()
            .map(|a| {
                let parent = a
                    .parent
                    .map_or_else(|| "-".to_owned(), |p| format!("achievement.{p}"));
                format!(
                    "achievement.{}|{}|{}|{parent}|{}|{}|{}",
                    a.key, a.column, a.row, a.challenge, a.icon.0, a.icon.1
                )
            })
            .collect();
        assert_eq!(table, want);
        assert_eq!(
            bounds,
            [format!("{MIN_COLUMN} {MIN_ROW} {MAX_COLUMN} {MAX_ROW}")]
        );
    }

    #[test]
    fn display_bounds_match_the_table() {
        let col = |f: fn(i32, i32) -> i32| ACHIEVEMENTS.iter().map(|a| a.column).fold(0, f);
        let row = |f: fn(i32, i32) -> i32| ACHIEVEMENTS.iter().map(|a| a.row).fold(0, f);
        assert_eq!((col(i32::min), row(i32::min)), (MIN_COLUMN, MIN_ROW));
        assert_eq!((col(i32::max), row(i32::max)), (MAX_COLUMN, MAX_ROW));
    }

    #[test]
    fn depth_counts_locked_ancestors() {
        let mut s = PlayerStats::default();
        let get = |k| Achievement::by_key(k).unwrap();
        assert_eq!(s.depth(get("openInventory")), 0);
        assert_eq!(s.depth(get("buildPickaxe")), 3);
        assert!(!s.can_unlock(get("mineWood")));
        s.set("achievement.openInventory", 1);
        s.set("achievement.mineWood", 1);
        assert!(s.can_unlock(get("buildWorkBench")));
        assert_eq!(s.depth(get("buildPickaxe")), 1);
        assert_eq!(s.depth(get("mineWood")), 0);
    }

    /// Values from `java.util.Random` and `String.hashCode` on a JDK.
    #[test]
    fn java_hash_matches_the_jdk() {
        assert_eq!(java_hash("hello wörld✓"), -92601898);
        assert_eq!(java_hash("abc"), 96354);
        let mut r = JavaRandom::new(i64::from(
            java_hash("abc").wrapping_add(3 + 5 + (7 + 2) * 16),
        ));
        assert_eq!(r.next_int(1 + 9 + 4), 12);
        assert_eq!(r.next_int(2), 0);
    }

    /// The depth clip of vanilla keeps the links and icons inside the drawn tiles.
    #[test]
    fn clip_keeps_quads_inside_the_tile_area() {
        use crate::toolkit::{Gui, GuiScale};
        let mut g = Gui::new(GuiScale::new(427, 240, 0, false), None, 0);
        {
            let mut c = crate::GuiCanvas::new(&mut g);
            let mut clip = Clip {
                inner: &mut c,
                area: [0.0, 0.0, 100.0, 50.0],
            };
            // Wholly outside, and straddling the right and bottom edges.
            clip.fill(-20, -20, -1, -1, 0xFFFF_FFFF);
            clip.fill(80, 40, 140, 90, 0xFFFF_FFFF);
        }
        let quads = g.into_quads();
        assert_eq!(quads.len(), 1, "the quad outside the area is dropped");
        let xs: Vec<f32> = quads[0].pos.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = quads[0].pos.iter().map(|p| p[1]).collect();
        assert_eq!(
            (
                xs.iter().cloned().fold(f32::MAX, f32::min),
                ys.iter().cloned().fold(f32::MAX, f32::min)
            ),
            (80.0, 40.0)
        );
        assert_eq!(
            (
                xs.iter().cloned().fold(f32::MIN, f32::max),
                ys.iter().cloned().fold(f32::MIN, f32::max)
            ),
            (100.0, 50.0)
        );
    }

    struct Stub;
    impl Screen for Stub {
        fn init(&mut self, _: &mut Ctx) {}
        fn draw(&mut self, _: &mut dyn Canvas, _: i32, _: i32, _: &mut Ctx) {}
        fn click(&mut self, _: i32, _: i32, _: i32, _: &mut Ctx) -> Option<Nav> {
            None
        }
    }

    struct Font<'a>(crate::toolkit::Metrics<'a>);
    impl crate::screens::Metrics for Font<'_> {
        fn width(&self, s: &str) -> i32 {
            self.0.width(s)
        }
    }

    /// Draws the tree for a player who made it to the pickaxe. `FERROCRAFT_CLIENT_JAR` gives the
    /// 1.8.9 client jar for the textures and text; `ACH_DIR` gets `achievements.png`.
    #[test]
    fn tree_draws_frames_and_links() {
        use crate::inventory::icons::Icons;
        use crate::screens::{Settings, Texts};
        use crate::toolkit::{Gui, GuiScale};
        use std::io::Read;

        let jar = std::env::var_os("FERROCRAFT_CLIENT_JAR").map(std::path::PathBuf::from);
        let read = |name: &str| -> Option<Vec<u8>> {
            let mut z = zip::ZipArchive::new(std::fs::File::open(jar.as_ref()?).ok()?).ok()?;
            let mut out = Vec::new();
            z.by_name(name).ok()?.read_to_end(&mut out).ok()?;
            Some(out)
        };
        let lang = read("assets/minecraft/lang/en_US.lang").unwrap_or_default();
        let texts = Texts::parse(&String::from_utf8_lossy(&lang), "");
        let icons = jar
            .is_some()
            .then(|| Icons::new(|n| read(&format!("assets/minecraft/{n}"))));
        let mut stats = PlayerStats::default();
        for k in [
            "openInventory",
            "mineWood",
            "buildWorkBench",
            "buildPickaxe",
        ] {
            stats.set(&format!("achievement.{k}"), 1);
        }
        let mut r = render::Renderer::new_offscreen(854, 480, jar.as_deref());
        let font = Font(crate::toolkit::Metrics(r.font()));
        let mut settings = Settings::new(std::path::PathBuf::from("/tmp"));
        let mut clip = String::new();
        let mut ctx = Ctx {
            width: 427,
            height: 240,
            texts: &texts,
            settings: &mut settings,
            in_world: true,
            difficulty: None,
            hardcore: false,
            now_ms: 0,
            game_dir: std::path::PathBuf::from("/tmp"),
            effects: Vec::new(),
            clipboard: &mut clip,
            metrics: &font,
            shift: false,
            stats: &stats,
            icons: icons.as_ref(),
            player_id: "Player",
        };
        let mut s = Achievements::new(Box::new(Stub));
        s.init(&mut ctx);
        assert_eq!(ctx.effects, [Effect::RequestStats]);
        let mut g = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
        let mut c = crate::GuiCanvas::new(&mut g);
        s.draw(&mut c, 0, 0, &mut ctx);
        let waiting = g.into_quads().len();
        s.stats_ready();
        let mut g2 = Gui::new(GuiScale::new(854, 480, 0, false), r.font(), 0);
        let mut c2 = crate::GuiCanvas::new(&mut g2);
        s.draw(&mut c2, 214, 100, &mut ctx);
        let quads = g2.into_quads();
        assert!(
            quads.len() > waiting,
            "the tree draws more than the wait text"
        );
        let frames = quads.iter().filter(|q| q.tex == BACKGROUND).count();
        assert!(
            frames > ACHIEVEMENTS.len() / 4,
            "frames and arrows are drawn"
        );
        if let (Some(_), Some(dir)) = (jar, std::env::var_os("ACH_DIR")) {
            let (w, h, rgba) = r.render_to_rgba(&render::Camera::default(), &quads);
            let path = std::path::Path::new(&dir).join("achievements.png");
            render::encode_png(&path, w, h, &rgba).expect("write png");
        }
    }
}
