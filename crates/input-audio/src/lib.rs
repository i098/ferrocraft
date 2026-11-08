//! Input state: vanilla 1.8.9 default keys, mouse look, chat entry.
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Vanilla look scale: degrees per scaled mouse pixel.
const DEGREES_PER_PIXEL: f32 = 0.15;
/// Vanilla 1.8.9 chat limit.
const CHAT_MAX_CHARS: usize = 100;

/// Vanilla `f^3 * 8` look gain for a sensitivity slider value (1.0 = 100%).
pub fn look_gain(sensitivity: f32) -> f32 {
    let f = sensitivity * 0.6 + 0.2;
    f * f * f * 8.0
}

#[derive(Debug)]
pub struct InputState {
    sensitivity: f32,
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    jump: bool,
    sneak: bool,
    sprint: bool,
    look: (f32, f32),
    chat: Option<String>,
    submit: Option<String>,
    grab_requested: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl InputState {
    /// `sensitivity` is the vanilla slider value: 0.0 = 0%, 1.0 = 100%.
    pub fn new(sensitivity: f32) -> Self {
        Self {
            sensitivity,
            forward: false,
            back: false,
            left: false,
            right: false,
            jump: false,
            sneak: false,
            sprint: false,
            look: (0.0, 0.0),
            chat: None,
            submit: None,
            grab_requested: true,
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let text = event.text.as_deref();
                    self.on_key(code, event.state.is_pressed(), text);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.grab_requested = true,
            WindowEvent::Focused(false) => self.release_all(),
            _ => {}
        }
    }

    /// Key press/release. `text` is the character text of a press, used in chat mode.
    pub fn on_key(&mut self, code: KeyCode, pressed: bool, text: Option<&str>) {
        if self.chat.is_some() {
            if pressed {
                self.chat_key(code, text);
            }
            return;
        }
        match code {
            KeyCode::KeyW => self.forward = pressed,
            KeyCode::KeyS => self.back = pressed,
            KeyCode::KeyA => self.left = pressed,
            KeyCode::KeyD => self.right = pressed,
            KeyCode::Space => self.jump = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.sneak = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.sprint = pressed,
            KeyCode::KeyT if pressed => self.open_chat(String::new()),
            KeyCode::Slash if pressed => self.open_chat("/".into()),
            KeyCode::Escape if pressed => self.grab_requested = false,
            _ => {}
        }
    }

    fn open_chat(&mut self, initial: String) {
        self.release_all();
        self.chat = Some(initial);
    }

    fn chat_key(&mut self, code: KeyCode, text: Option<&str>) {
        let Some(buf) = self.chat.as_mut() else {
            return;
        };
        match code {
            KeyCode::Escape => self.chat = None,
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let line = self.chat.take().unwrap_or_default();
                if !line.trim().is_empty() {
                    self.submit = Some(line);
                }
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            _ => {
                for c in text.unwrap_or("").chars().filter(|c| !c.is_control()) {
                    if buf.chars().count() < CHAT_MAX_CHARS {
                        buf.push(c);
                    }
                }
            }
        }
    }

    fn release_all(&mut self) {
        self.forward = false;
        self.back = false;
        self.left = false;
        self.right = false;
        self.jump = false;
        self.sneak = false;
        self.sprint = false;
    }

    /// Raw mouse delta in pixels (from `DeviceEvent::MouseMotion`).
    pub fn on_mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.chat.is_none() && self.grab_requested {
            self.look.0 += dx as f32;
            self.look.1 += dy as f32;
        }
    }

    pub fn on_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.on_mouse_motion(*dx, *dy);
        }
    }

    /// Accumulated (yaw, pitch) change in vanilla degrees since the last call.
    /// Positive yaw turns right, positive pitch looks down. The caller clamps pitch to ±90.
    pub fn take_look_delta(&mut self) -> (f32, f32) {
        let (dx, dy) = std::mem::take(&mut self.look);
        let k = look_gain(self.sensitivity) * DEGREES_PER_PIXEL;
        (dx * k, dy * k)
    }

    pub fn movement(&self) -> world::Input {
        let axis = |pos: bool, neg: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        world::Input {
            forward: axis(self.forward, self.back),
            strafe: axis(self.left, self.right),
            jump: self.jump,
            sprint: self.sprint,
        }
    }

    pub fn sneaking(&self) -> bool {
        self.sneak
    }

    pub fn chat_input(&self) -> Option<&str> {
        self.chat.as_deref()
    }

    pub fn take_chat_submit(&mut self) -> Option<String> {
        self.submit.take()
    }

    /// True while the game owns the mouse: not in chat and not released with Esc.
    pub fn wants_cursor_grab(&self) -> bool {
        self.chat.is_none() && self.grab_requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(s: &mut InputState, c: KeyCode) {
        s.on_key(c, true, None);
    }

    fn typ(s: &mut InputState, c: KeyCode, t: &str) {
        s.on_key(c, true, Some(t));
    }

    #[test]
    fn keys_map_to_input() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        press(&mut s, KeyCode::KeyA);
        press(&mut s, KeyCode::Space);
        press(&mut s, KeyCode::ControlLeft);
        let i = s.movement();
        assert_eq!(
            (i.forward, i.strafe, i.jump, i.sprint),
            (1.0, 1.0, true, true)
        );
        press(&mut s, KeyCode::KeyS);
        press(&mut s, KeyCode::KeyD);
        let i = s.movement();
        assert_eq!((i.forward, i.strafe), (0.0, 0.0));
        s.on_key(KeyCode::KeyW, false, None);
        s.on_key(KeyCode::Space, false, None);
        let i = s.movement();
        assert_eq!((i.forward, i.jump), (-1.0, false));
        press(&mut s, KeyCode::ShiftLeft);
        assert!(s.sneaking());
    }

    #[test]
    fn chat_open_type_backspace_submit() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        press(&mut s, KeyCode::KeyT);
        assert_eq!(s.chat_input(), Some(""));
        assert!(!s.wants_cursor_grab());
        assert_eq!(s.movement().forward, 0.0);
        typ(&mut s, KeyCode::KeyH, "h");
        typ(&mut s, KeyCode::KeyI, "i");
        typ(&mut s, KeyCode::KeyT, "t");
        press(&mut s, KeyCode::Backspace);
        assert_eq!(s.chat_input(), Some("hi"));
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.chat_input(), None);
        assert_eq!(s.take_chat_submit().as_deref(), Some("hi"));
        assert_eq!(s.take_chat_submit(), None);
        assert!(s.wants_cursor_grab());
    }

    #[test]
    fn chat_escape_and_empty_submit() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::Slash);
        assert_eq!(s.chat_input(), Some("/"));
        press(&mut s, KeyCode::Escape);
        assert_eq!(s.chat_input(), None);
        assert_eq!(s.take_chat_submit(), None);
        press(&mut s, KeyCode::KeyT);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.take_chat_submit(), None);
    }

    #[test]
    fn chat_is_capped_at_100_chars() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        for _ in 0..120 {
            typ(&mut s, KeyCode::KeyA, "a");
        }
        assert_eq!(s.chat_input().map(str::len), Some(100));
    }

    #[test]
    fn look_delta_matches_vanilla_formula() {
        // 100% sensitivity: f = 0.8, f^3 * 8 = 4.096, * 0.15 deg/px.
        let mut s = InputState::new(1.0);
        s.on_mouse_motion(10.0, -5.0);
        s.on_mouse_motion(10.0, 0.0);
        let (yaw, pitch) = s.take_look_delta();
        assert!((yaw - 20.0 * 4.096 * 0.15).abs() < 1e-4);
        assert!((pitch - -5.0 * 4.096 * 0.15).abs() < 1e-4);
        assert_eq!(s.take_look_delta(), (0.0, 0.0));
        // 50% (vanilla default): f = 0.5, gain = 1.0.
        assert!((look_gain(0.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mouse_ignored_in_chat_and_after_esc() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        s.on_mouse_motion(10.0, 10.0);
        press(&mut s, KeyCode::Escape);
        press(&mut s, KeyCode::Escape);
        assert!(!s.wants_cursor_grab());
        s.on_mouse_motion(10.0, 10.0);
        assert_eq!(s.take_look_delta(), (0.0, 0.0));
    }
}
