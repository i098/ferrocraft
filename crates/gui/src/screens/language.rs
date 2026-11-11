//! `GuiLanguage` with its language list, and `LanguageMetadataSection` from `pack.mcmeta`.

use super::options::Opt;
use super::{
    centered, draw_buttons, press, release_all, Button, Canvas, Ctx, Effect, Nav, Screen,
    ScrollList, WHITE,
};

const FORCE_UNICODE: i32 = 100;
const DONE: i32 = 6;

/// `Language`: shown as `name (region)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub region: String,
    pub bidirectional: bool,
}

/// The `language` section of `pack.mcmeta`, sorted by code as `LanguageManager` keeps it.
/// Entries with an empty name or region are skipped.
pub fn languages(pack_mcmeta: &[u8]) -> Vec<Language> {
    let json: serde_json::Value = serde_json::from_slice(pack_mcmeta).unwrap_or_default();
    let Some(map) = json["language"].as_object() else {
        return Vec::new();
    };
    let mut all: Vec<Language> = map
        .iter()
        .filter_map(|(code, v)| {
            let s = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
            Some(Language {
                code: code.clone(),
                name: s("name")?,
                region: s("region")?,
                bidirectional: v["bidirectional"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    all.sort_by(|a, b| a.code.cmp(&b.code));
    all
}

/// `GuiLanguage`.
pub struct LanguageScreen {
    parent: Option<Box<dyn Screen>>,
    list: ScrollList,
    buttons: Vec<Button>,
    held: bool,
}

impl LanguageScreen {
    pub fn new(parent: Box<dyn Screen>) -> LanguageScreen {
        LanguageScreen {
            parent: Some(parent),
            list: ScrollList::new(0, 0, 0, 0, 18),
            buttons: Vec::new(),
            held: false,
        }
    }
}

impl Screen for LanguageScreen {
    fn init(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.width, ctx.height);
        let scroll = self.list.scroll;
        self.list = ScrollList::new(w, h, 32, h - 65 + 4, 18);
        self.list.scroll = scroll;
        let unicode = ctx.settings.label(Opt::ForceUnicode, ctx.texts);
        self.buttons = vec![
            Button::new(FORCE_UNICODE, w / 2 - 155, h - 38, 150, 20, unicode),
            Button::new(DONE, w / 2 - 155 + 160, h - 38, 150, 20, ctx.tr("gui.done")),
        ];
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        let n = ctx.texts.languages.len();
        if self.held {
            self.list.drag(my, n);
        }
        self.list.draw_back(c, n);
        let current = ctx.texts.bidirectional(&ctx.settings.lang);
        for (i, l) in ctx.texts.languages.iter().enumerate() {
            if !self.list.row_visible(i) {
                continue;
            }
            if l.code == ctx.settings.lang {
                self.list.draw_selection(c, i);
            }
            let label = format!("{} ({})", l.name, l.region);
            c.set_bidi(true);
            centered(c, &label, ctx.width / 2, self.list.row_y(i) + 1, WHITE);
            c.set_bidi(current);
        }
        self.list.draw_front(c, n);
        centered(c, &ctx.tr("options.language"), ctx.width / 2, 16, WHITE);
        let warning = format!("({})", ctx.tr("options.languageWarning"));
        centered(c, &warning, ctx.width / 2, ctx.height - 56, 0x808080);
        draw_buttons(&mut self.buttons, c, mx, my);
    }

    fn click(&mut self, x: i32, y: i32, mb: i32, ctx: &mut Ctx) -> Option<Nav> {
        match press(&mut self.buttons, x, y, mb, ctx) {
            Some(DONE) => return self.parent.take().map(Nav::Open),
            Some(FORCE_UNICODE) => {
                ctx.settings.cycle(Opt::ForceUnicode);
                ctx.save_settings();
                self.buttons[0].label = ctx.settings.label(Opt::ForceUnicode, ctx.texts);
                return None;
            }
            _ => {}
        }
        let n = ctx.texts.languages.len();
        if mb != 0 {
            return None;
        }
        self.held = true;
        self.list.press(x, y, n);
        let row = (y >= self.list.top && y <= self.list.bottom)
            .then(|| self.list.row_at(x, y, n))
            .flatten()?;
        ctx.settings.lang = ctx.texts.languages[row].code.clone();
        if let Err(e) = ctx.settings.save() {
            eprintln!("Failed to save options: {e}");
        }
        ctx.effects.push(Effect::LanguageChanged);
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        self.held = false;
        self.list.release();
        release_all(&mut self.buttons);
    }

    fn scroll(&mut self, dir: i32, ctx: &mut Ctx) {
        self.list.wheel(dir, ctx.texts.languages.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_sort_by_code_and_skip_empty_names() {
        let json = br#"{"language":{
            "fr_FR":{"name":"Fran\u00e7ais","region":"France"},
            "ar_SA":{"name":"\u0627\u0644\u0639\u0631\u0628\u064a\u0629","region":"x","bidirectional":true},
            "xx_XX":{"name":"","region":"Nowhere"},
            "en_US":{"name":"English","region":"US"}}}"#;
        let l = languages(json);
        let codes: Vec<&str> = l.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["ar_SA", "en_US", "fr_FR"]);
        assert!(l[0].bidirectional && !l[1].bidirectional);
        assert_eq!(
            (l[2].name.as_str(), l[2].region.as_str()),
            ("Français", "France")
        );
    }
}
