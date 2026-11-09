//! [`screens::Canvas`] over a [`Gui`], so screens, the inventory and the HUD share one quad list.

use crate::screens::{Canvas, Metrics};
use crate::toolkit::Gui;
use render::Blend;

pub struct GuiCanvas<'a, 'f> {
    pub gui: &'a mut Gui<'f>,
    /// PNGs from [`Canvas::texture`]; the engine drains them into `Renderer::register_texture`.
    pub pending_textures: Vec<(String, Vec<u8>)>,
}

impl<'a, 'f> GuiCanvas<'a, 'f> {
    pub fn new(gui: &'a mut Gui<'f>) -> Self {
        GuiCanvas {
            gui,
            pending_textures: Vec::new(),
        }
    }
}

impl Metrics for GuiCanvas<'_, '_> {
    fn width(&self, s: &str) -> i32 {
        self.gui.width(s)
    }
}

impl Canvas for GuiCanvas<'_, '_> {
    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, argb: u32) {
        self.gui.fill(x0, y0, x1, y1, argb);
    }

    fn gradient(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, top: u32, bottom: u32) {
        self.gui.gradient(x0, y0, x1, y1, top, bottom);
    }

    fn blit(&mut self, tex: &str, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], argb: u32) {
        self.gui.blit_tinted(tex.to_owned(), x, y, w, h, uv, argb);
    }

    fn quad(&mut self, tex: &str, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: [u32; 4]) {
        self.gui.quad(tex.to_owned(), pts, uv, argb);
    }

    fn quad_add(&mut self, tex: &str, pts: [[f32; 2]; 4], uv: [[f32; 2]; 4], argb: u32) {
        self.gui.set_blend(Blend::Add);
        self.gui.quad(tex.to_owned(), pts, uv, [argb; 4]);
        self.gui.set_blend(Blend::Alpha);
    }

    fn text(&mut self, s: &str, x: f32, y: f32, argb: u32, shadow: bool) {
        self.gui.draw_string(s, x, y, argb, shadow);
    }

    fn texture(&mut self, name: &str, png: &[u8]) {
        self.pending_textures.push((name.to_owned(), png.to_vec()));
    }

    fn push(&mut self, translate: [f32; 2], scale: f32, rotate_deg: f32) {
        self.gui.push();
        self.gui.translate(translate[0], translate[1]);
        self.gui.scale(scale, scale);
        self.gui.rotate(rotate_deg);
    }

    fn pop(&mut self) {
        self.gui.pop();
    }
}
