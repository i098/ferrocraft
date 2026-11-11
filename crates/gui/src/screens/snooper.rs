//! `GuiSnooper`: the machine data vanilla would report, with the `snooperEnabled` toggle.
//!
//! Ferrocraft never sends this data: vanilla posts it to `snoop.minecraft.net`, which no longer
//! exists. The list shows only the values this client can know; the Java, OpenGL, memory, `fps`
//! and `display_frequency` rows have no equivalent here.

use super::options::Opt;
use super::{
    centered, draw_buttons, press, release_all, trim, wrap, Button, Canvas, Ctx, Nav, Screen,
    ScrollList, WHITE,
};
use std::sync::LazyLock;

const TOGGLE: i32 = 1;
const DONE: i32 = 2;

/// `Snooper.uniqueID`: a random UUID, fixed for the run.
static TOKEN: LazyLock<String> = LazyLock::new(|| {
    use std::hash::{BuildHasher, RandomState};
    let r = |n: u64| RandomState::new().hash_one(n);
    // Version 4, variant 2, as `UUID.randomUUID`.
    let hi = r(0) & !0xf000 | 0x4000;
    let lo = r(1) & !(0xc << 60) | (0x8 << 60);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xffff,
        hi & 0xffff,
        lo >> 48,
        lo & 0xffff_ffff_ffff
    )
});

/// The client's snooper map (`Snooper.getCurrentStats`), sorted by key as the `TreeMap` in `initGui`.
fn rows(ctx: &Ctx) -> Vec<(String, String)> {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "Mac OS X",
        "windows" => "Windows",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        other => other,
    };
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let packs: Vec<String> = serde_json::from_str(&ctx.settings.resource_packs).unwrap_or_default();
    let rows = [
        ("client_brand", "vanilla".to_owned()),
        ("cpu_cores", cores.to_string()),
        (
            "current_action",
            if ctx.in_world {
                "multiplayer"
            } else {
                "out_of_game"
            }
            .to_owned(),
        ),
        (
            "display_type",
            if ctx.settings.fullscreen {
                "fullscreen"
            } else {
                "windowed"
            }
            .to_owned(),
        ),
        (
            "endianness",
            if cfg!(target_endian = "little") {
                "little"
            } else {
                "big"
            }
            .to_owned(),
        ),
        ("launched_version", "1.8.9".to_owned()),
        ("os_architecture", arch.to_owned()),
        ("os_name", os.to_owned()),
        ("snooper_token", TOKEN.clone()),
        ("version", "1.8.9".to_owned()),
        ("vsync_enabled", ctx.settings.vsync.to_string()),
        // Vanilla's formula: the run time in milliseconds / 60 * 1000.
        ("run_time", (ctx.now_ms / 60 * 1000).to_string()),
        ("resource_packs", packs.len().to_string()),
    ];
    let mut rows: Vec<(String, String)> =
        rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    rows.extend(
        packs
            .into_iter()
            .enumerate()
            .map(|(i, p)| (format!("resource_pack[{i}]"), p)),
    );
    rows.sort();
    rows
}

/// `GuiSnooper`.
pub struct Snooper {
    parent: Option<Box<dyn Screen>>,
    list: ScrollList,
    buttons: Vec<Button>,
    desc: Vec<String>,
    rows: Vec<(String, String)>,
    held: bool,
}

impl Snooper {
    pub fn new(parent: Box<dyn Screen>) -> Snooper {
        Snooper {
            parent: Some(parent),
            list: ScrollList::new(0, 0, 0, 0, 10),
            buttons: Vec::new(),
            desc: Vec::new(),
            rows: Vec::new(),
            held: false,
        }
    }
}

impl Screen for Snooper {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        self.desc = wrap(ctx.metrics, &ctx.tr("options.snooper.desc"), w - 30);
        let max = w - 220;
        self.rows = rows(ctx)
            .into_iter()
            .map(|(k, v)| (k, trim(ctx.metrics, &v, max, false)))
            .collect();
        let toggle = ctx.settings.label(Opt::Snooper, ctx.texts);
        self.buttons = vec![
            Button::new(TOGGLE, w / 2 - 152, h - 30, 150, 20, toggle),
            Button::new(DONE, w / 2 + 2, h - 30, 150, 20, ctx.tr("gui.done")),
        ];
        let scroll = self.list.scroll;
        self.list = ScrollList::new(w, h, 80, h - 40, 10);
        self.list.bar_x = w - 10;
        self.list.scroll = scroll;
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let n = self.rows.len();
        if self.held {
            self.list.drag(my, n);
        }
        self.list.draw_back(c, n);
        for (i, (k, v)) in self.rows.iter().enumerate() {
            if self.list.row_visible(i) {
                let y = self.list.row_y(i);
                c.text(k, 10.0, y as f32, WHITE, false);
                c.text(v, 230.0, y as f32, WHITE, false);
            }
        }
        self.list.draw_front(c, n);
        centered(c, &ctx.tr("options.snooper.title"), ctx.width / 2, 8, WHITE);
        for (i, line) in self.desc.iter().enumerate() {
            centered(c, line, ctx.width / 2, 22 + i as i32 * 9, 0x808080);
        }
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, mb, ctx) {
            Some(DONE) => {
                ctx.save_settings();
                return self.parent.take().map(Nav::Open);
            }
            Some(TOGGLE) => {
                ctx.settings.cycle(Opt::Snooper);
                ctx.save_settings();
                self.buttons[0].label = ctx.settings.label(Opt::Snooper, ctx.texts);
                return None;
            }
            _ => {}
        }
        if mb == 0 {
            self.held = true;
            self.list.press(x, y, self.rows.len());
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        self.held = false;
        self.list.release();
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, _ctx: &mut Ctx) {
        self.list.wheel(dir, self.rows.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_a_v4_uuid() {
        let t = TOKEN.as_str();
        let parts: Vec<&str> = t.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'));
        assert!("89ab".contains(&parts[3][..1]));
    }
}
