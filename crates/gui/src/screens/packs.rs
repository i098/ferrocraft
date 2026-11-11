//! `GuiScreenResourcePacks` with its two pack lists, and the pack folder of
//! `ResourcePackRepository`.

use super::options::Settings;
use super::{
    blit256, centered, dirt, draw_buttons, press, release_all, text, trim, wrap, Button, Canvas,
    Ctx, Effect, Nav, Screen, ScrollList, YesNo, WHITE,
};
use render::AssetSource;
use std::path::{Path, PathBuf};

const OPEN_FOLDER: i32 = 2;
const DONE: i32 = 1;
const ARROWS: &str = "textures/gui/resource_packs.png";
/// The `pack_format` of 1.8.9.
const FORMAT: i64 = 1;
const ROW_H: i32 = 36;
const LIST_W: i32 = 200;

/// `ResourcePackRepository.Entry`; `path` is `None` for the built-in default pack.
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub name: String,
    pub path: Option<PathBuf>,
    pub format: i64,
    pub description: String,
    pub icon: Option<Vec<u8>>,
}

impl Pack {
    /// `Entry.updateResourcePack`: `None` when `pack.mcmeta` is missing or is not JSON, as vanilla
    /// then drops the pack from the list.
    fn read(path: &Path) -> Option<Pack> {
        let mut src = AssetSource::open(path)?;
        let meta: serde_json::Value = serde_json::from_slice(&src.read("pack.mcmeta")?).ok()?;
        let section = meta.get("pack");
        let description = description(&meta);
        Some(Pack {
            name: path.file_name()?.to_string_lossy().into_owned(),
            path: Some(path.to_path_buf()),
            // Vanilla crashes on a pack without a `pack` section; here it is incompatible instead.
            format: section.and_then(|p| p["pack_format"].as_i64()).unwrap_or(0),
            description,
            icon: src.read("pack.png"),
        })
    }
}

/// `ResourcePackListEntry.getResourcePackDescription` of a parsed `pack.mcmeta`.
fn description(meta: &serde_json::Value) -> String {
    meta.get("pack")
        .and_then(|p| p.get("description"))
        .map_or_else(
            || "\u{a7}cInvalid pack.mcmeta (or missing 'pack' section)".to_owned(),
            |d| {
                ferrocraft_net_client::formatted_text(&ferrocraft_net_client::styled(
                    &d.to_string(),
                ))
            },
        )
}

/// The description of the built-in pack, from the `pack.mcmeta` of the launcher's asset index.
pub fn default_description(mcmeta: &[u8]) -> String {
    serde_json::from_slice(mcmeta).map_or_else(|_| String::new(), |m| description(&m))
}

/// `ResourcePackRepository.updateRepositoryEntriesAll`: `.zip` files and folders with a
/// `pack.mcmeta` in `game_dir/resourcepacks`, by name.
pub fn available(game_dir: &Path) -> Vec<Pack> {
    let Ok(dir) = std::fs::read_dir(game_dir.join("resourcepacks")) else {
        return Vec::new();
    };
    let mut packs: Vec<Pack> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            (p.is_file() && p.extension().is_some_and(|e| e == "zip"))
                || p.join("pack.mcmeta").is_file()
        })
        .filter_map(|p| Pack::read(&p))
        .collect();
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    packs
}

fn names(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// The packs `Minecraft.refreshResources` loads, top first: `resourcePacks` names that exist and
/// are compatible or were accepted as incompatible (`ResourcePackRepository` constructor).
pub fn selected_paths(game_dir: &Path, settings: &Settings) -> Vec<PathBuf> {
    let (all, accepted) = (
        available(game_dir),
        names(&settings.incompatible_resource_packs),
    );
    let mut paths: Vec<PathBuf> = names(&settings.resource_packs)
        .iter()
        .filter_map(|n| all.iter().find(|p| &p.name == n))
        .filter(|p| p.format == FORMAT || accepted.contains(&p.name))
        .filter_map(|p| p.path.clone())
        .collect();
    paths.reverse();
    paths
}

/// `GuiScreenResourcePacks`.
pub struct PacksScreen {
    parent: Option<Box<dyn Screen>>,
    /// `[available, selected]`; selected is top first and ends with the default pack.
    packs: [Vec<Pack>; 2],
    lists: [ScrollList; 2],
    buttons: Vec<Button>,
    changed: bool,
    loaded: bool,
    /// Icons already handed to the canvas.
    icons_sent: bool,
    held: Option<usize>,
    /// The incompatible pack waiting for the confirm screen.
    confirming: Option<usize>,
}

impl PacksScreen {
    pub fn new(parent: Box<dyn Screen>) -> PacksScreen {
        PacksScreen {
            parent: Some(parent),
            packs: [Vec::new(), Vec::new()],
            lists: [
                ScrollList::new(0, 0, 0, 0, ROW_H),
                ScrollList::new(0, 0, 0, 0, ROW_H),
            ],
            buttons: Vec::new(),
            changed: false,
            loaded: false,
            icons_sent: false,
            held: None,
            confirming: None,
        }
    }

    /// `initGui` before the first change: the repository's packs, minus the selected ones.
    fn load(&mut self, ctx: &Ctx) {
        let all = available(&ctx.game_dir);
        let selected = selected_paths(&ctx.game_dir, ctx.settings);
        let (sel, avail): (Vec<Pack>, Vec<Pack>) = all
            .into_iter()
            .partition(|p| p.path.as_ref().is_some_and(|q| selected.contains(q)));
        let mut sel: Vec<Pack> = selected
            .iter()
            .filter_map(|q| sel.iter().find(|p| p.path.as_ref() == Some(q)).cloned())
            .collect();
        sel.push(Pack {
            name: "Default".into(),
            path: None,
            format: FORMAT,
            description: ctx.texts.default_pack.1.clone(),
            icon: ctx.texts.default_pack.0.clone(),
        });
        self.packs = [avail, sel];
        // `Entry.updateResourcePack`: a pack without `pack.png` shows the default pack's icon.
        for p in self.packs.iter_mut().flatten().filter(|p| p.icon.is_none()) {
            p.icon = ctx.texts.default_pack.0.clone();
        }
        self.icons_sent = false;
    }

    fn icon_name(p: &Pack) -> String {
        format!("texturepackicon/{}", p.name)
    }

    /// `ResourcePackListEntry.mousePressed` on entry `i` of list `l`, at `(ex, ey)` in the entry.
    fn press_entry(&mut self, l: usize, i: usize, ex: i32, ey: i32, ctx: &Ctx) -> Option<Nav> {
        if self.packs[l][i].path.is_none() || ex > 32 {
            return None;
        }
        let list = &mut self.packs[l];
        if l == 0 {
            self.changed = true;
            let format = list[i].format;
            if format != FORMAT {
                self.confirming = Some(i);
                let kind = if format > FORMAT { "new" } else { "old" };
                let q = ctx.tr("resourcePack.incompatible.confirm.title");
                let m = ctx.tr(&format!("resourcePack.incompatible.confirm.{kind}"));
                let (y, n) = (ctx.tr("gui.yes"), ctx.tr("gui.no"));
                return Some(Nav::Child(Box::new(move |me| {
                    Box::new(YesNo::new(me, q, m, y, n, 0))
                })));
            }
            let p = list.remove(i);
            self.packs[1].insert(0, p);
        } else if ex < 16 {
            let p = list.remove(i);
            self.packs[0].insert(0, p);
            self.changed = true;
        } else if ey < 16 && i > 0 {
            list.swap(i, i - 1);
            self.changed = true;
        } else if ey > 16 && i + 1 < list.len() && list[i + 1].path.is_some() {
            list.swap(i, i + 1);
            self.changed = true;
        }
        None
    }

    /// `actionPerformed` for Done: save the order bottom first, then reload the resources.
    fn done(&mut self, ctx: &mut Ctx) -> Option<Nav> {
        if self.changed {
            let mut chosen: Vec<&Pack> =
                self.packs[1].iter().filter(|p| p.path.is_some()).collect();
            chosen.reverse();
            let list = |f: &dyn Fn(&&&Pack) -> bool| {
                let n: Vec<&str> = chosen.iter().filter(f).map(|p| p.name.as_str()).collect();
                serde_json::to_string(&n).unwrap_or_default()
            };
            ctx.settings.resource_packs = list(&|_| true);
            ctx.settings.incompatible_resource_packs = list(&|p| p.format != FORMAT);
            ctx.save_settings();
            ctx.effects.push(Effect::ResourcesChanged);
        }
        self.parent.take().map(Nav::Open)
    }

    #[allow(clippy::too_many_arguments)] // the vanilla entry draw signature
    fn draw_entry(
        &self,
        c: &mut dyn Canvas,
        l: usize,
        i: usize,
        x: i32,
        y: i32,
        hover: Option<(i32, i32)>,
        ctx: &Ctx,
    ) {
        let list = &self.packs[l];
        let p = &list[i];
        if p.format != FORMAT {
            c.fill(x - 1, y - 1, x + LIST_W - 9, y + 32 + 1, 0xff770000);
        }
        c.blit(
            &Self::icon_name(p),
            x as f32,
            y as f32,
            32.0,
            32.0,
            [0.0, 0.0, 1.0, 1.0],
            0xffffffff,
        );
        let (mut name, mut desc) = (p.name.clone(), p.description.clone());
        let movable = p.path.is_some();
        if let Some((mx, my)) = hover.filter(|_| movable) {
            c.fill(x, y, x + 32, y + 32, 0xa0909090);
            let (ex, ey) = (mx - x, my - y);
            if p.format != FORMAT {
                name = ctx.tr("resourcePack.incompatible");
                let kind = if p.format > FORMAT { "new" } else { "old" };
                desc = ctx.tr(&format!("resourcePack.incompatible.{kind}"));
            }
            let arrow = |c: &mut dyn Canvas, u: i32, lit: bool| {
                blit256(c, ARROWS, x, y, u, if lit { 32 } else { 0 }, 32, 32);
            };
            if l == 0 {
                arrow(c, 0, ex < 32);
            } else {
                arrow(c, 32, ex < 16);
                let in_right = ex < 32 && ex > 16;
                if i > 0 {
                    arrow(c, 96, in_right && ey < 16);
                }
                if list.get(i + 1).is_some_and(|n| n.path.is_some()) {
                    arrow(c, 64, in_right && ey > 16);
                }
            }
        }
        if ctx.metrics.width(&name) > 157 {
            name = trim(ctx.metrics, &name, 157 - ctx.metrics.width("..."), false) + "...";
        }
        text(c, &name, x + 32 + 2, y + 1, WHITE);
        for (j, line) in wrap(ctx.metrics, &desc, 157).iter().take(2).enumerate() {
            text(c, line, x + 32 + 2, y + 12 + 10 * j as i32, 0x808080);
        }
    }

    /// The list and entry under `(x, y)`, with the point relative to the entry.
    fn entry_at(&self, x: i32, y: i32) -> Option<(usize, usize, i32, i32)> {
        (0..2).find_map(|l| {
            let list = &self.lists[l];
            if y < list.top || y > list.bottom {
                return None;
            }
            let i = list.row_at(x, y, self.packs[l].len())?;
            Some((l, i, x - list.row_x(), y - list.row_y(i)))
        })
    }
}

impl Screen for PacksScreen {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.buttons = vec![
            Button::new(
                OPEN_FOLDER,
                w / 2 - 154,
                h - 48,
                150,
                20,
                ctx.tr("resourcePack.openFolder"),
            ),
            Button::new(DONE, w / 2 + 4, h - 48, 150, 20, ctx.tr("gui.done")),
        ];
        if !self.changed && !self.loaded {
            self.load(ctx);
            self.loaded = true;
        }
        // `GuiResourcePackList`: header 1.5 lines of the font, rows of 36.
        let header = (9.0 * 1.5) as i32;
        for (l, x) in [w / 2 - 4 - LIST_W, w / 2 + 4].into_iter().enumerate() {
            self.lists[l] = ScrollList::new(LIST_W, h, 32, h - 55 + 4, ROW_H).at(x, header);
        }
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        if !self.icons_sent {
            for p in self.packs.iter().flatten() {
                if let Some(png) = &p.icon {
                    c.texture(&Self::icon_name(p), png);
                }
            }
            self.icons_sent = true;
        }
        dirt(c, 0, 0, ctx.width, ctx.height, 0, 0xff404040);
        let hovered = self.entry_at(mx, my).map(|(l, i, ..)| (l, i));
        let titles = [
            "resourcePack.available.title",
            "resourcePack.selected.title",
        ];
        for (l, key) in titles.into_iter().enumerate() {
            let n = self.packs[l].len();
            if self.held == Some(l) {
                self.lists[l].drag(my, n);
            }
            self.lists[l].draw_back(c, n);
            let list = &self.lists[l];
            let (x, top) = (list.row_x(), list.top + 4 - list.scroll as i32);
            let title = format!("\u{a7}n\u{a7}l{}", ctx.tr(key));
            let tx = x + LIST_W / 2 - c.width(&title) / 2;
            c.text(
                &title,
                tx as f32,
                (list.top + 3).min(top) as f32,
                WHITE,
                false,
            );
            for i in (0..n).filter(|&i| list.row_visible(i)) {
                let hover =
                    (hovered == Some((l, i)) || ctx.settings.touchscreen).then_some((mx, my));
                self.draw_entry(c, l, i, x, list.row_y(i), hover, ctx);
            }
            self.lists[l].draw_front(c, n);
        }
        centered(c, &ctx.tr("resourcePack.title"), ctx.width / 2, 16, WHITE);
        let info = ctx.tr("resourcePack.folderInfo");
        centered(c, &info, ctx.width / 2 - 77, ctx.height - 26, 0x808080);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, mb, ctx) {
            Some(DONE) => return self.done(ctx),
            Some(OPEN_FOLDER) => {
                let dir = ctx.game_dir.join("resourcepacks");
                let _ = std::fs::create_dir_all(&dir);
                ctx.effects
                    .push(Effect::OpenUrl(dir.to_string_lossy().into_owned()));
                return None;
            }
            _ => {}
        }
        if mb != 0 {
            return None;
        }
        for l in 0..2 {
            if x >= self.lists[l].left && x < self.lists[l].left + LIST_W {
                self.held = Some(l);
                self.lists[l].press(x, y, self.packs[l].len());
            }
        }
        let (l, i, ex, ey) = self.entry_at(x, y)?;
        self.press_entry(l, i, ex, ey, ctx)
    }

    fn confirm(&mut self, yes: bool, _id: i32, _ctx: &mut Ctx) -> Option<Nav> {
        let i = self.confirming.take()?;
        if yes && i < self.packs[0].len() {
            let p = self.packs[0].remove(i);
            self.packs[1].insert(0, p);
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        self.held = None;
        self.lists.iter_mut().for_each(ScrollList::release);
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        // `handleMouseInput` gives the wheel to both lists.
        for l in 0..2 {
            self.lists[l].wheel(dir, self.packs[l].len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_dir(root: &Path, name: &str, mcmeta: &str) {
        let d = root.join("resourcepacks").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("pack.mcmeta"), mcmeta).unwrap();
    }

    #[test]
    fn selected_packs_are_top_first_and_skip_unaccepted_incompatible_ones() {
        let root = std::env::temp_dir().join(format!("fc-packs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        pack_dir(
            &root,
            "a",
            r#"{"pack":{"pack_format":1,"description":"A"}}"#,
        );
        pack_dir(
            &root,
            "b",
            r#"{"pack":{"pack_format":1,"description":{"text":"B","color":"red"}}}"#,
        );
        pack_dir(
            &root,
            "old",
            r#"{"pack":{"pack_format":0,"description":"x"}}"#,
        );
        pack_dir(
            &root,
            "new",
            r#"{"pack":{"pack_format":2,"description":"x"}}"#,
        );
        pack_dir(&root, "broken", "not json");
        let all = available(&root);
        let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "new", "old"]);
        assert_eq!(all[1].description, "\u{a7}cB\u{a7}r");
        let mut s = Settings::new(root.clone());
        s.resource_packs = r#"["a","old","gone","new","b"]"#.into();
        s.incompatible_resource_packs = r#"["new"]"#.into();
        let got: Vec<String> = selected_paths(&root, &s)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(got, ["b", "new", "a"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
