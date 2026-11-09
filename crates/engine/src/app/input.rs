//! Window input: while a menu or container is open it goes to that screen, else to the game.

use super::{App, Fm};
use gui::inventory::{Key, Mods, Request};
use gui::screens::Event;
use input_audio::{lwjgl_from_control, Bind, Control};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

impl App {
    pub(super) fn route_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if self.mouse_down {
                    let (mx, my) = self.gui_cursor();
                    if let Some(c) = &mut self.container {
                        c.mouse_drag(&self.game.inv, mx, my);
                    }
                }
                return;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.shift = s.shift_key();
                self.ctrl = s.control_key() || s.super_key();
            }
            _ => {}
        }
        let modal = self.ui.is_open() || self.container.is_some();
        let typing = self.input.chat_input().is_some();
        if !modal && !typing {
            self.input.on_window_event(event);
            return;
        }
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => return,
                };
                self.mouse_button(*state == ElementState::Pressed, b);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                let step = y.signum() as i32;
                if y != 0.0 && modal {
                    self.scroll_screen(step);
                } else {
                    self.input.on_window_event(event);
                }
            }
            WindowEvent::KeyboardInput { event: k, .. } if modal => {
                if let PhysicalKey::Code(code) = k.physical_key {
                    if k.state.is_pressed() {
                        let typed = k.text.as_deref().and_then(|t| t.chars().next());
                        self.screen_key(code, typed);
                    }
                }
            }
            _ => self.input.on_window_event(event),
        }
    }

    fn gui_cursor(&self) -> (i32, i32) {
        let Some(r) = &self.renderer else {
            return (0, 0);
        };
        let (w, h) = r.size();
        let f = f64::from(
            gui::GuiScale::new(w, h, u32::from(self.ui.settings.gui_scale))
                .factor
                .max(1),
        );
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

    fn screen_key(&mut self, code: KeyCode, typed: Option<char>) {
        if self.container.is_some() {
            let key = self.container_key(code);
            let mods = self.mods();
            let reqs =
                self.container
                    .as_mut()
                    .expect("checked")
                    .key(&mut self.game.inv, key, typed, mods);
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
        let b = self.input.bindings();
        let is = |bind: Bind| b.get(bind) == Control::Key(code);
        if is(Bind::Inventory) {
            Key::Inventory
        } else if is(Bind::Drop) {
            Key::Drop
        } else if is(Bind::PickBlock) {
            Key::PickBlock
        } else if is(Bind::Chat) {
            Key::Chat
        } else if let Some(n) = (0..9).find(|n| is(Bind::Hotbar(*n))) {
            Key::Hotbar(n)
        } else {
            Key::Other
        }
    }

    fn send_requests(&mut self, reqs: Vec<Request>) {
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
