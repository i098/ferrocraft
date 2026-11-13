//! Window input: while a menu or container is open it goes to that screen, else to the game.

use super::events::open_url;
use super::{boxed, App, Fm};
use gui::chat::ChatClick;
use gui::inventory::{Key, Mods, Request};
use gui::screens::{ConfirmLink, Event};
use input_audio::{lwjgl_from_control, Bind, Control};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

impl App {
    pub(super) fn route_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.drag_container();
                return;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.shift = s.shift_key();
                self.ctrl = s.control_key() || s.super_key();
            }
            _ => {}
        }
        if self.mod_screen()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
            )
        {
            return;
        }
        let modal = self.ui.is_open() || self.container.is_some();
        if modal || self.input.chat_input().is_some() {
            self.route_to_screen(event, modal);
        } else {
            self.input.on_window_event(event);
        }
    }

    fn drag_container(&mut self) {
        if !self.mouse_down {
            return;
        }
        let (mx, my) = self.gui_cursor();
        if let Some(c) = &mut self.container {
            c.mouse_drag(&self.game.inv, mx, my);
        }
    }

    /// Mouse and keys while a screen is open or chat is being typed.
    fn route_to_screen(&mut self, event: &WindowEvent, modal: bool) {
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(b) = button_index(*button) else {
                    return;
                };
                self.mouse_button(*state == ElementState::Pressed, b);
            }
            WindowEvent::MouseWheel { delta, .. } if modal => {
                let y = wheel_lines(delta);
                if y != 0.0 {
                    self.scroll_screen(y.signum() as i32);
                }
            }
            WindowEvent::KeyboardInput { event: k, .. } if modal => {
                if let PhysicalKey::Code(code) = k.physical_key {
                    if k.state.is_pressed() {
                        let typed = k.text.as_deref().and_then(|t| t.chars().next());
                        self.screen_key(code, typed, k.repeat);
                    }
                }
            }
            _ => self.chat_event(event),
        }
    }

    /// A key while chat is open: the field needs the font widths and the clipboard.
    fn chat_event(&mut self, event: &WindowEvent) {
        let m = metrics!(self);
        let ctrl = matches!(event, WindowEvent::KeyboardInput { .. }) && self.mods().ctrl;
        let (input, ui) = (&mut self.input, &mut self.ui);
        ui.with_clipboard(ctrl, |clip| input.on_window_event_in(event, &m, clip));
    }
}

fn button_index(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
        _ => None,
    }
}

fn wheel_lines(delta: &MouseScrollDelta) -> f64 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
        MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
    }
}

impl App {
    fn gui_cursor(&self) -> (i32, i32) {
        let Some(r) = &self.renderer else {
            return (0, 0);
        };
        let (w, h) = r.size();
        let scale = gui::GuiScale::new(
            w,
            h,
            u32::from(self.ui.settings.gui_scale),
            self.ui.unicode(),
        );
        let f = f64::from(scale.factor.max(1));
        ((self.cursor.0 / f) as i32, (self.cursor.1 / f) as i32)
    }

    fn mods(&self) -> Mods {
        Mods {
            shift: self.shift,
            ctrl: self.ctrl,
        }
    }

    fn mouse_button(&mut self, down: bool, button: u8) {
        self.mouse_down = down;
        if down && button == 0 && self.chat_click() {
            return;
        }
        let (mx, my) = self.gui_cursor();
        let mods = self.mods();
        if self.container.is_some() {
            let now = self.ui_now_ms();
            let reqs = {
                let c = self.container.as_mut().expect("checked");
                if down {
                    c.mouse_down(&mut self.game.inv, mx, my, button, mods, now)
                } else {
                    c.mouse_up(&mut self.game.inv, mx, my, button, mods)
                }
            };
            self.send_requests(reqs);
            return;
        }
        let (x, y, button) = (mx, my, i32::from(button));
        let ev = if down {
            Event::Click { x, y, button }
        } else {
            Event::Release { x, y, button }
        };
        let m = Fm(gui::Metrics(self.renderer.as_ref().and_then(|r| r.font())));
        let q = self.ui.event(&m, ev, self.shift);
        self.run_effects(q);
    }

    /// `GuiChat.mouseClicked`: a left click on a chat component runs its click event.
    /// True when the click is used up.
    fn chat_click(&mut self) -> bool {
        if self.input.chat_input().is_none() || self.modal() {
            return false;
        }
        let Some(piece) = self.chat_piece().cloned() else {
            return false;
        };
        let s = &self.ui.settings;
        let Some(click) = gui::chat::click(&piece, self.shift, s.chat_links, s.chat_links_prompt)
        else {
            return false;
        };
        match click {
            ChatClick::Handled => {}
            ChatClick::Command(line) => {
                if let Some(s) = self.link.session() {
                    let r = s.send_chat(&line);
                    self.send_result(r);
                }
            }
            ChatClick::Suggest(text) => {
                let m = metrics!(self);
                self.input.set_chat_text(&text, true, &m);
            }
            ChatClick::Insert(text) => {
                let m = metrics!(self);
                self.input.set_chat_text(&text, false, &m);
            }
            ChatClick::ConfirmLink(url) => {
                let m = metrics!(self);
                let q = self.ui.open(&m, boxed(ConfirmLink::new(url)));
                self.run_effects(q);
            }
            ChatClick::OpenLink(url) => open_url(&url),
        }
        true
    }

    /// `GuiNewChat.getChatComponent` at the mouse.
    pub(super) fn chat_piece(&self) -> Option<&gui::chat::Piece> {
        let r = self.renderer.as_ref()?;
        let (w, h) = r.size();
        let scale = gui::GuiScale::new(
            w,
            h,
            u32::from(self.ui.settings.gui_scale),
            self.ui.unicode(),
        );
        let m = gui::Metrics(r.font());
        let (x, y) = (self.cursor.0 as i32, h as i32 - self.cursor.1 as i32);
        self.hud.chat.component_at(x, y, scale.factor.max(1), m)
    }

    fn ui_now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn scroll_screen(&mut self, step: i32) {
        if let Some(c) = &mut self.container {
            c.scroll(&self.game.inv, step);
        } else {
            let m = metrics!(self);
            let q = self.ui.event(&m, Event::Scroll(step), self.shift);
            self.run_effects(q);
        }
    }

    fn screen_key(&mut self, code: KeyCode, typed: Option<char>, repeat: bool) {
        if self.input.screenshot_key(code, !repeat) {
            return;
        }
        if self.container.is_some() {
            let key = self.container_key(code);
            let mods = self.mods();
            let m = metrics!(self);
            let reqs = self.container.as_mut().expect("checked").key(
                &mut self.game.inv,
                key,
                typed,
                mods,
                &m,
            );
            self.send_requests(reqs);
            return;
        }
        let Some(lwjgl) = lwjgl_from_control(Control::Key(code)) else {
            return;
        };
        let ch = typed.filter(|c| !c.is_control()).unwrap_or('\0');
        let ev = Event::Key {
            code: lwjgl,
            ch,
            ctrl: self.ctrl,
            shift: self.shift,
        };
        let m = metrics!(self);
        let q = self.ui.event(&m, ev, self.shift);
        self.run_effects(q);
    }

    fn container_key(&self, code: KeyCode) -> Key {
        if code == KeyCode::Escape {
            return Key::Escape;
        }
        if code == KeyCode::Backspace {
            return Key::Backspace;
        }
        match self.input.bindings().lookup(Control::Key(code)) {
            Some(Bind::Inventory) => Key::Inventory,
            Some(Bind::Drop) => Key::Drop,
            Some(Bind::PickBlock) => Key::PickBlock,
            Some(Bind::Chat) => Key::Chat,
            Some(Bind::Hotbar(n)) => Key::Hotbar(n),
            _ => Key::Other,
        }
    }

    pub(super) fn send_requests(&mut self, reqs: Vec<Request>) {
        for r in reqs {
            match r {
                Request::Click(c) => {
                    let res = self.session_send_click(&c);
                    self.send_result(res);
                }
                Request::Creative { slot, item } => {
                    if let Some(s) = self.link.session() {
                        let res = s.creative_slot(slot, &item);
                        self.send_result(res);
                    }
                }
                Request::Enchant { window_id, button } => {
                    if let Some(s) = self.link.session() {
                        let res = s.enchant_item(window_id, button);
                        self.send_result(res);
                    }
                }
                Request::Plugin { channel, data } => {
                    if let Some(s) = self.link.session() {
                        let res = s.plugin_message(channel, data);
                        self.send_result(res);
                    }
                }
                Request::Close { .. } => self.close_container(),
            }
        }
    }

    fn session_send_click(&mut self, c: &world::inventory::Click) -> std::io::Result<()> {
        match self.link.session() {
            Some(s) => s.click_window(
                c.window_id,
                c.slot,
                c.button,
                c.action,
                c.mode,
                &c.clicked_item,
            ),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Args;
    use gui::inventory::ContainerScreen;
    use input_audio::Action;

    #[test]
    fn f2_screenshots_while_a_container_is_open() {
        let mut app = App::new(Args {
            username: "Player".into(),
            auth: None,
            assets_dir: None,
            server: None,
            game_dir: std::env::temp_dir().join("ferrocraft-no-such-game-dir"),
        });
        app.container = Some(ContainerScreen::new(&app.game.inv, 854, 480));
        app.screen_key(KeyCode::F2, None, true);
        assert_eq!(app.input.take_actions(), vec![]);
        app.screen_key(KeyCode::F2, None, false);
        assert_eq!(app.input.take_actions(), vec![Action::Screenshot]);
    }
}
