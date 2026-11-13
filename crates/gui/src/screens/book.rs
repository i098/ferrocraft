//! `GuiScreenBook` for a written book: the pages of the held item, read only.

use super::{
    blit256, default_background, draw_buttons, press, release_all, wrap, Button, Canvas, Ctx,
    Effect, Nav, Screen,
};
use ferrocraft_net_client::{formatted_text, styled};
use world::inventory::{ItemStack, Tag};

const BOOK: &str = "textures/gui/book.png";
/// Page text width and the lines that fit (`128 / FONT_HEIGHT`).
const TEXT_WIDTH: i32 = 116;
const MAX_LINES: usize = 14;

pub struct Book {
    /// The `pages` strings of the item: chat component JSON.
    pages: Vec<String>,
    page: usize,
    done: Vec<Button>,
    /// The wrapped lines of the current page; built on the first frame because wrapping needs the font.
    lines: Option<Vec<String>>,
}

/// `Items.written_book`.
pub const WRITTEN_BOOK: i16 = 387;

impl Book {
    /// `GuiScreenBook` for a held stack: its `pages` tag, or `None` when it is not a written book.
    pub fn for_stack(s: &ItemStack) -> Option<Book> {
        (s.id == WRITTEN_BOOK).then(|| Book::new(pages_of(s)))
    }

    pub fn new(pages: Vec<String>) -> Book {
        Book {
            pages,
            page: 0,
            done: Vec::new(),
            lines: None,
        }
    }

    /// `bookTotalPages`: an empty book still has one page.
    fn total(&self) -> usize {
        self.pages.len().max(1)
    }

    fn left(ctx: &Ctx) -> i32 {
        (ctx.width - 192) / 2
    }

    /// The `NextPageButton`s that show: `(x, is_next)`.
    fn arrows(&self, ctx: &Ctx) -> Vec<(i32, bool)> {
        let left = Self::left(ctx);
        let mut out = Vec::new();
        if self.page + 1 < self.total() {
            out.push((left + 120, true));
        }
        if self.page > 0 {
            out.push((left + 38, false));
        }
        out
    }
}

fn over_arrow(x: i32, mx: i32, my: i32) -> bool {
    mx >= x && my >= 156 && mx < x + 23 && my < 156 + 13
}

impl Screen for Book {
    fn init(&mut self, ctx: &mut Ctx) {
        self.done = vec![Button::new(
            0,
            ctx.width / 2 - 100,
            4 + 192,
            200,
            20,
            ctx.tr("gui.done"),
        )];
        self.lines = None;
    }

    fn draw(&mut self, c: &mut dyn Canvas, mx: i32, my: i32, ctx: &mut Ctx) {
        default_background(c, ctx);
        let left = Self::left(ctx);
        blit256(c, BOOK, left, 2, 0, 0, 192, 192);
        let indicator = ctx.texts.fmt(
            "book.pageIndicator",
            &[&(self.page + 1).to_string(), &self.total().to_string()],
        );
        let x = left - c.width(&indicator) + 192 - 44;
        c.text(&indicator, x as f32, 18.0, 0, false);
        let json = self.pages.get(self.page);
        let lines = self.lines.get_or_insert_with(|| {
            let text = json.map_or(String::new(), |j| formatted_text(&styled(j)));
            wrap(c, &text, TEXT_WIDTH)
        });
        for (i, line) in lines.iter().take(MAX_LINES).enumerate() {
            c.text(
                line,
                (left + 36) as f32,
                (34 + i as i32 * 9) as f32,
                0,
                false,
            );
        }
        draw_buttons(&mut self.done, c, mx, my);
        for (x, next) in self.arrows(ctx) {
            let u = if over_arrow(x, mx, my) { 23 } else { 0 };
            blit256(c, BOOK, x, 156, u, 192 + if next { 0 } else { 13 }, 23, 13);
        }
    }

    fn click(&mut self, x: i32, y: i32, button: i32, ctx: &mut Ctx) -> Option<Nav> {
        if press(&mut self.done, x, y, button, ctx).is_some() {
            return Some(Nav::Game);
        }
        if button == 0 {
            let hit = self
                .arrows(ctx)
                .into_iter()
                .find(|&(ax, _)| over_arrow(ax, x, y));
            if let Some((_, next)) = hit {
                ctx.effects.push(Effect::Sound("gui.button.press"));
                self.page = if next { self.page + 1 } else { self.page - 1 };
                self.lines = None;
            }
        }
        None
    }

    fn release(&mut self, _x: i32, _y: i32, _b: i32, _ctx: &mut Ctx) {
        release_all(&mut self.done);
    }
}

/// The `pages` list of the stack's NBT: one chat component JSON string each.
fn pages_of(s: &ItemStack) -> Vec<String> {
    let list = s
        .nbt
        .as_ref()
        .and_then(|n| n.root.iter().find(|(k, _)| k == "pages"));
    match list {
        Some((_, Tag::List(l))) => l
            .iter()
            .filter_map(|t| match t {
                Tag::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::inventory::Nbt;

    fn stack(id: i16, pages: Vec<Tag>) -> ItemStack {
        ItemStack {
            id,
            count: 1,
            damage: 0,
            nbt: Some(Nbt {
                name: String::new(),
                root: vec![("pages".into(), Tag::List(pages))],
            }),
        }
    }

    #[test]
    fn only_a_written_book_opens_with_its_pages() {
        let pages = vec![Tag::String("\"a\"".into()), Tag::String("\"b\"".into())];
        let book = Book::for_stack(&stack(WRITTEN_BOOK, pages)).expect("written book");
        assert_eq!(book.pages, ["\"a\"", "\"b\""]);
        assert!(Book::for_stack(&stack(276, vec![])).is_none());
        let bare = ItemStack {
            nbt: None,
            ..stack(WRITTEN_BOOK, vec![])
        };
        assert_eq!(Book::for_stack(&bare).expect("book").total(), 1);
    }
}
