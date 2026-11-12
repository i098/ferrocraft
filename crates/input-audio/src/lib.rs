//! Input state: vanilla 1.8.9 key bindings, mouse look, mouse buttons, wheel, chat entry.
mod actions;
mod keys;
pub mod sound;

pub use actions::{scroll_slot, Action, DebugCombo};
pub use keys::{control_from_lwjgl, lwjgl_from_control, Bind, Control, KeyBindings};

use std::collections::VecDeque;
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

/// Vanilla look scale: degrees per scaled mouse pixel.
const DEGREES_PER_PIXEL: f32 = 0.15;
/// Vanilla 1.8.9 chat limit.
const CHAT_MAX_CHARS: usize = 100;
/// Vanilla `GuiNewChat` keeps 100 sent lines.
const CHAT_HISTORY: usize = 100;
/// Trackpads report pixels; one wheel step per this many.
const PIXELS_PER_STEP: f64 = 40.0;

/// Vanilla `f^3 * 8` look gain for a sensitivity slider value (1.0 = 100%).
pub fn look_gain(sensitivity: f32) -> f32 {
    let f = sensitivity * 0.6 + 0.2;
    f * f * f * 8.0
}

/// Own the OS cursor grab state. Returns the new "grabbed" state.
///
/// macOS has no `Confined` mode and refuses grabs while the window is not key, so the caller
/// must retry until this returns `want` and must forget the grab when focus is lost: the OS
/// drops it then.
pub fn apply_cursor_grab(window: &Window, want: bool) -> bool {
    if want {
        let ok = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            .is_ok();
        window.set_cursor_visible(!ok);
        ok
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        false
    }
}

/// The queued action for a bind press, if it has one.
fn press_action(bind: Bind, ctrl: bool) -> Option<Action> {
    Some(match bind {
        Bind::Inventory => Action::Inventory,
        Bind::Drop => Action::Drop { whole_stack: ctrl },
        Bind::Attack => Action::Attack,
        Bind::Use => Action::Use,
        Bind::PickBlock => Action::PickBlock,
        Bind::Screenshot => Action::Screenshot,
        Bind::Perspective => Action::TogglePerspective,
        Bind::Fullscreen => Action::ToggleFullscreen,
        Bind::Hotbar(n) => Action::Hotbar(n),
        _ => return None,
    })
}

/// F3 + key combos of `Minecraft.processKeyF3`.
fn debug_combo(code: KeyCode) -> Option<DebugCombo> {
    Some(match code {
        KeyCode::KeyA => DebugCombo::ReloadChunks,
        KeyCode::KeyD => DebugCombo::ClearChat,
        KeyCode::KeyH => DebugCombo::AdvancedTooltips,
        KeyCode::KeyP => DebugCombo::PauseOnLostFocus,
        KeyCode::KeyQ => DebugCombo::Help,
        _ => return None,
    })
}

#[derive(Debug)]
pub struct InputState {
    sensitivity: f32,
    bindings: KeyBindings,
    held: [bool; Bind::ALL.len()],
    actions: VecDeque<Action>,
    ctrl_down: bool,
    f3_down: bool,
    sprinting: bool,
    sprint_timer: u8,
    /// Blindness: the sprint latch cannot start (`EntityPlayerSP.onLivingUpdate`); a running sprint goes on.
    pub blind: bool,
    /// Sneak as the mod host's `EntityPlayerSP` left it this tick; `None` without a host.
    host_sneak: Option<bool>,
    was_full_forward: bool,
    was_sneaking: bool,
    scroll_px: f64,
    look: (f32, f32),
    chat: Option<String>,
    submit: Option<String>,
    history: Vec<String>,
    history_pos: Option<usize>,
    history_draft: String,
    grab_requested: bool,
    screen_open: bool,
    focused: bool,
    pause_on_lost_focus: bool,
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
            bindings: KeyBindings::default(),
            held: [false; Bind::ALL.len()],
            actions: VecDeque::new(),
            ctrl_down: false,
            f3_down: false,
            sprinting: false,
            sprint_timer: 0,
            blind: false,
            host_sneak: None,
            was_full_forward: false,
            was_sneaking: false,
            scroll_px: 0.0,
            look: (0.0, 0.0),
            chat: None,
            submit: None,
            history: Vec::new(),
            history_pos: None,
            history_draft: String::new(),
            grab_requested: true,
            screen_open: false,
            focused: true,
            pause_on_lost_focus: true,
        }
    }

    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity;
    }

    /// `options.txt` `pauseOnLostFocus`.
    pub fn set_pause_on_lost_focus(&mut self, on: bool) {
        self.pause_on_lost_focus = on;
    }

    pub fn set_bindings(&mut self, bindings: KeyBindings) {
        self.release_all();
        self.bindings = bindings;
    }

    pub fn bindings(&self) -> &KeyBindings {
        &self.bindings
    }

    /// The engine tells input that a GUI screen (inventory, pause menu...) is open: movement
    /// stops and the cursor is free. Clicks and keys then belong to the screen.
    pub fn set_screen_open(&mut self, open: bool) {
        if open {
            self.release_all();
        }
        self.screen_open = open;
        self.grab_requested = !open;
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let text = event.text.as_deref();
                    self.key_event(code, event.state.is_pressed(), event.repeat, text);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.mouse_button(*button, *state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => self.scroll(*delta),
            WindowEvent::Focused(focused) => self.set_focused(*focused),
            _ => {}
        }
    }

    fn scroll(&mut self, delta: MouseScrollDelta) {
        match delta {
            MouseScrollDelta::LineDelta(_, y) => self.wheel(f64::from(y)),
            MouseScrollDelta::PixelDelta(PhysicalPosition { y, .. }) => {
                self.scroll_px += y;
                while self.scroll_px.abs() >= PIXELS_PER_STEP {
                    let s = self.scroll_px.signum();
                    self.scroll_px -= s * PIXELS_PER_STEP;
                    self.wheel(s);
                }
            }
        }
    }

    fn set_focused(&mut self, focused: bool) {
        let was_playing = self.wants_cursor_grab();
        self.focused = focused;
        if focused {
            return;
        }
        self.release_all();
        if was_playing && self.pause_on_lost_focus {
            self.grab_requested = false;
            self.actions.push_back(Action::Pause);
        }
    }

    fn wheel(&mut self, y: f64) {
        let step = if y == 0.0 { 0 } else { y.signum() as i32 };
        if step == 0 {
            return;
        }
        if self.chat.is_some() {
            self.actions.push_back(Action::ChatScroll(step));
        } else if !self.screen_open {
            self.actions.push_back(Action::HotbarScroll(step));
        }
    }

    /// Key press/release. `text` is the character text of a press, used in chat mode.
    pub fn on_key(&mut self, code: KeyCode, pressed: bool, text: Option<&str>) {
        self.key_event(code, pressed, false, text);
    }

    /// Vanilla takes the screenshot in any screen, chat included. True when `code` is the
    /// screenshot key, so the caller drops it; a press queues the action.
    pub fn screenshot_key(&mut self, code: KeyCode, fresh_press: bool) -> bool {
        if self.bindings.lookup(Control::Key(code)) != Some(Bind::Screenshot) {
            return false;
        }
        if fresh_press {
            self.actions.push_back(Action::Screenshot);
        }
        true
    }

    fn key_event(&mut self, code: KeyCode, pressed: bool, repeat: bool, text: Option<&str>) {
        match code {
            KeyCode::ControlLeft | KeyCode::ControlRight => self.ctrl_down = pressed,
            KeyCode::F3 if !pressed => self.f3_down = false,
            _ => {}
        }
        if self.screenshot_key(code, pressed && !repeat) {
            return;
        }
        if self.chat.is_some() || self.screen_open {
            self.modal_key(code, pressed, repeat, text);
            return;
        }
        if pressed && !repeat && self.hard_coded_key(code) {
            return;
        }
        self.control(Control::Key(code), pressed, repeat);
    }

    /// Key while chat or a GUI screen owns the keyboard.
    fn modal_key(&mut self, code: KeyCode, pressed: bool, repeat: bool, text: Option<&str>) {
        if self.chat.is_some() {
            if pressed {
                self.chat_key(code, text);
            }
        } else if pressed && !repeat && code == KeyCode::Escape {
            self.actions.push_back(Action::CloseScreen);
        }
    }

    /// Esc, F1, F3 and the F3 combos: not rebindable in vanilla. True when consumed.
    fn hard_coded_key(&mut self, code: KeyCode) -> bool {
        let combo = debug_combo(code);
        if let (true, Some(c)) = (self.f3_down, combo) {
            if c == DebugCombo::PauseOnLostFocus {
                self.pause_on_lost_focus = !self.pause_on_lost_focus;
            }
            self.actions.push_back(Action::Debug(c));
            return true;
        }
        let action = match code {
            KeyCode::Escape => {
                self.grab_requested = false;
                Action::Pause
            }
            KeyCode::F1 => Action::ToggleHud,
            KeyCode::F4 => Action::ToggleShader,
            KeyCode::F3 => {
                self.f3_down = true;
                Action::ToggleDebug
            }
            _ => return false,
        };
        self.actions.push_back(action);
        true
    }

    fn mouse_button(&mut self, button: MouseButton, pressed: bool) {
        if self.chat.is_some() || self.screen_open {
            return;
        }
        if pressed && !self.grab_requested {
            // Vanilla `setIngameFocus`: the click that regains the mouse does not attack.
            self.grab_requested = true;
            return;
        }
        self.control(Control::Mouse(button), pressed, false);
    }

    fn control(&mut self, control: Control, pressed: bool, repeat: bool) {
        let Some(bind) = self.bindings.lookup(control) else {
            return;
        };
        self.held[bind.index()] = pressed;
        if pressed && !repeat {
            self.on_press(bind);
        }
    }

    fn on_press(&mut self, bind: Bind) {
        if let Some(action) = press_action(bind, self.ctrl_down) {
            self.actions.push_back(action);
            return;
        }
        match bind {
            Bind::Chat => self.open_chat(String::new()),
            Bind::Command => self.open_chat("/".into()),
            _ => {}
        }
    }

    fn open_chat(&mut self, initial: String) {
        self.release_all();
        self.history_pos = None;
        self.history_draft.clear();
        self.chat = Some(initial);
    }

    fn chat_key(&mut self, code: KeyCode, text: Option<&str>) {
        let Some(buf) = self.chat.as_mut() else {
            return;
        };
        match code {
            KeyCode::Escape => self.chat = None,
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit_chat(),
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::ArrowUp => self.history_step(-1),
            KeyCode::ArrowDown => self.history_step(1),
            _ => {
                for c in text.unwrap_or("").chars().filter(|&c| allowed_char(c)) {
                    if buf.encode_utf16().count() + c.len_utf16() <= CHAT_MAX_CHARS {
                        buf.push(c);
                    }
                }
            }
        }
    }

    fn submit_chat(&mut self) {
        // Vanilla 1.8.9 uses Java `String.trim()`, which strips only chars <= U+0020 (keeps NBSP).
        let line = self.chat.take().unwrap_or_default();
        let line = line.trim_matches(|c| c <= ' ').to_owned();
        if line.is_empty() {
            return;
        }
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
            let extra = self.history.len().saturating_sub(CHAT_HISTORY);
            self.history.drain(..extra);
        }
        self.submit = Some(line);
    }

    /// `GuiChat.getSentHistory`: -1 older, +1 newer; past the newest restores the typed draft.
    fn history_step(&mut self, dir: i32) {
        let len = self.history.len();
        let cur = self.history_pos.unwrap_or(len);
        let next = (cur as i32 + dir).clamp(0, len as i32) as usize;
        if next == cur {
            return;
        }
        if next == len {
            self.history_pos = None;
            self.chat = Some(std::mem::take(&mut self.history_draft));
            return;
        }
        if self.history_pos.is_none() {
            self.history_draft = self.chat.clone().unwrap_or_default();
        }
        self.history_pos = Some(next);
        self.chat = Some(self.history[next].clone());
    }

    /// The sent-message part of `GuiNewChat.clearChatMessages` (F3+D).
    pub fn clear_chat_history(&mut self) {
        self.history.clear();
        self.history_pos = None;
        self.history_draft.clear();
    }

    fn release_all(&mut self) {
        self.held = [false; Bind::ALL.len()];
        self.f3_down = false;
    }

    fn is_held(&self, bind: Bind) -> bool {
        self.held[bind.index()]
    }

    /// Raw mouse delta in pixels (from `DeviceEvent::MouseMotion`).
    pub fn on_mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.wants_cursor_grab() && self.focused {
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

    /// Queued actions since the last call, oldest first.
    pub fn take_actions(&mut self) -> Vec<Action> {
        self.actions.drain(..).collect()
    }

    /// Call once per game tick: `EntityPlayerSP.onLivingUpdate` sprint latch.
    /// `can_sprint` is `foodLevel > 6 || allowFlying`.
    pub fn movement(
        &mut self,
        on_ground: bool,
        collided: bool,
        can_sprint: bool,
        using_item: bool,
    ) -> world::Input {
        let axis = |pos: bool, neg: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        let forward = axis(self.is_held(Bind::Forward), self.is_held(Bind::Back));
        self.host_sneak = None;
        let sneak = self.sneaking();
        let full = forward > 0.0 && !sneak && !using_item;
        let sprint_key = self.is_held(Bind::Sprint);
        self.sprint_timer = self.sprint_timer.saturating_sub(1);
        self.update_sprint(on_ground, can_sprint && full && !self.blind, sprint_key);
        if self.sprinting && (!full || collided || !can_sprint) {
            self.sprinting = false;
        }
        self.was_full_forward = full;
        self.was_sneaking = sneak;
        world::Input {
            forward,
            strafe: axis(self.is_held(Bind::Left), self.is_held(Bind::Right)),
            jump: self.is_held(Bind::Jump),
            sprint: self.sprinting,
            sneak,
            using_item: false,
            fly_speed: None,
        }
    }

    /// The same tick when the mod host runs `EntityPlayerSP.onLivingUpdate`: mods hook the sprint and sneak
    /// key reads there, so sprint and sneak are the host's. The movement keys are still read here.
    pub fn movement_from_host(&mut self, sprint: bool, sneak: bool) -> world::Input {
        let axis = |pos: bool, neg: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        self.host_sneak = Some(sneak);
        self.sprinting = sprint;
        self.was_full_forward = self.is_held(Bind::Forward) && !self.is_held(Bind::Back) && !sneak;
        self.was_sneaking = sneak;
        world::Input {
            forward: axis(self.is_held(Bind::Forward), self.is_held(Bind::Back)),
            strafe: axis(self.is_held(Bind::Left), self.is_held(Bind::Right)),
            jump: self.is_held(Bind::Jump),
            sprint,
            sneak,
            using_item: false,
            fly_speed: None,
        }
    }

    /// Sneak from the mod host while a screen is open; `None` returns to the key.
    pub fn set_host_sneak(&mut self, sneak: Option<bool>) {
        self.host_sneak = sneak;
    }

    fn update_sprint(&mut self, on_ground: bool, can_start: bool, sprint_key: bool) {
        let fresh = on_ground && !self.was_sneaking && !self.was_full_forward;
        if fresh && can_start && !self.sprinting {
            if self.sprint_timer == 0 && !sprint_key {
                self.sprint_timer = 7;
            } else {
                self.sprinting = true;
            }
        }
        if !self.sprinting && can_start && sprint_key {
            self.sprinting = true;
        }
    }

    pub fn sneaking(&self) -> bool {
        self.host_sneak.unwrap_or_else(|| self.is_held(Bind::Sneak))
    }

    /// Attack button held: vanilla keeps breaking blocks while it stays down.
    pub fn attack_held(&self) -> bool {
        self.is_held(Bind::Attack)
    }

    /// Use button held: vanilla repeats item use every 4 ticks while it stays down.
    pub fn use_held(&self) -> bool {
        self.is_held(Bind::Use)
    }

    /// Tab: show the player list while held.
    pub fn player_list_held(&self) -> bool {
        self.is_held(Bind::PlayerList)
    }

    pub fn chat_input(&self) -> Option<&str> {
        self.chat.as_deref()
    }

    pub fn take_chat_submit(&mut self) -> Option<String> {
        self.submit.take()
    }

    /// True while the game owns the mouse: in game, not in chat, not released with Esc,
    /// and the window has focus (the OS drops the grab on focus loss).
    pub fn wants_cursor_grab(&self) -> bool {
        self.chat.is_none() && !self.screen_open && self.grab_requested && self.focused
    }
}

/// `ChatAllowedCharacters.isAllowedCharacter`: no `§`, no control chars below space, no DEL.
pub fn allowed_char(c: char) -> bool {
    c != '\u{a7}' && c >= ' ' && c != '\u{7f}'
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

    fn release(s: &mut InputState, c: KeyCode) {
        s.on_key(c, false, None);
    }

    fn walk(s: &mut InputState, ticks: usize) -> bool {
        let mut sprint = false;
        for _ in 0..ticks {
            sprint = s.movement(true, false, true, false).sprint;
        }
        sprint
    }

    #[test]
    fn screenshot_key_works_in_screen_and_conflict_runs_one_action() {
        let mut s = InputState::default();
        s.set_screen_open(true);
        press(&mut s, KeyCode::F2);
        assert_eq!(s.take_actions(), vec![Action::Screenshot]);
        s.set_screen_open(false);
        let mut b = KeyBindings::default();
        b.set(Bind::Drop, Some(Control::Key(KeyCode::KeyE)));
        s.set_bindings(b);
        press(&mut s, KeyCode::KeyE);
        assert_eq!(s.take_actions(), vec![Action::Drop { whole_stack: false }]);
    }

    #[test]
    fn double_tap_forward_within_seven_ticks_sprints() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        assert!(!walk(&mut s, 1));
        release(&mut s, KeyCode::KeyW);
        walk(&mut s, 2);
        press(&mut s, KeyCode::KeyW);
        assert!(walk(&mut s, 1));
    }

    #[test]
    fn double_tap_after_eight_ticks_does_not_sprint() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        walk(&mut s, 1);
        release(&mut s, KeyCode::KeyW);
        walk(&mut s, 8);
        press(&mut s, KeyCode::KeyW);
        assert!(!walk(&mut s, 1));
    }

    #[test]
    fn sprint_survives_releasing_ctrl_and_stops_on_each_cancel() {
        let start = |s: &mut InputState| {
            press(s, KeyCode::ControlLeft);
            press(s, KeyCode::KeyW);
            assert!(walk(s, 1));
            release(s, KeyCode::ControlLeft);
        };
        let mut s = InputState::default();
        start(&mut s);
        assert!(walk(&mut s, 3));
        assert!(!s.movement(true, true, true, false).sprint);

        let mut s = InputState::default();
        start(&mut s);
        assert!(!s.movement(true, false, false, false).sprint);

        let mut s = InputState::default();
        start(&mut s);
        press(&mut s, KeyCode::ShiftLeft);
        assert!(!s.movement(true, false, true, false).sprint);
        release(&mut s, KeyCode::ShiftLeft);
        assert!(!walk(&mut s, 2));

        let mut s = InputState::default();
        start(&mut s);
        release(&mut s, KeyCode::KeyW);
        assert!(!walk(&mut s, 1));
    }

    #[test]
    fn keys_map_to_input() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        press(&mut s, KeyCode::KeyA);
        press(&mut s, KeyCode::Space);
        press(&mut s, KeyCode::ControlLeft);
        let i = s.movement(true, false, true, false);
        assert_eq!(
            (i.forward, i.strafe, i.jump, i.sprint),
            (1.0, 1.0, true, true)
        );
        press(&mut s, KeyCode::KeyS);
        press(&mut s, KeyCode::KeyD);
        let i = s.movement(true, false, true, false);
        assert_eq!((i.forward, i.strafe), (0.0, 0.0));
        s.on_key(KeyCode::KeyW, false, None);
        s.on_key(KeyCode::Space, false, None);
        let i = s.movement(true, false, true, false);
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
        assert_eq!(s.movement(true, false, true, false).forward, 0.0);
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

    #[test]
    fn double_tap_forward_latches_sprint() {
        let mut s = InputState::default();
        let tick = |s: &mut InputState| s.movement(true, false, true, false).sprint;
        press(&mut s, KeyCode::KeyW);
        assert!(!tick(&mut s));
        s.on_key(KeyCode::KeyW, false, None);
        assert!(!tick(&mut s));
        press(&mut s, KeyCode::KeyW);
        assert!(tick(&mut s));
        assert!(tick(&mut s));
        press(&mut s, KeyCode::ShiftLeft);
        assert!(!tick(&mut s));
        s.on_key(KeyCode::ShiftLeft, false, None);
        assert!(!tick(&mut s));
        press(&mut s, KeyCode::ControlLeft);
        assert!(tick(&mut s));
        s.on_key(KeyCode::ControlLeft, false, None);
        assert!(tick(&mut s));
        assert!(!s.movement(true, true, true, false).sprint);
        s.on_key(KeyCode::KeyW, false, None);
        for _ in 0..10 {
            tick(&mut s);
        }
        press(&mut s, KeyCode::KeyW);
        tick(&mut s);
        s.on_key(KeyCode::KeyW, false, None);
        for _ in 0..7 {
            tick(&mut s);
        }
        press(&mut s, KeyCode::KeyW);
        assert!(!tick(&mut s));
    }

    #[test]
    fn using_an_item_stops_and_blocks_sprint() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        press(&mut s, KeyCode::ControlLeft);
        assert!(s.movement(true, false, true, false).sprint);
        assert!(!s.movement(true, false, true, true).sprint);
        assert!(!s.movement(true, false, true, true).sprint);
        assert!(s.movement(true, false, true, false).sprint);
    }

    #[test]
    fn hotbar_drop_f3_and_esc_actions() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::Digit3);
        press(&mut s, KeyCode::KeyQ);
        press(&mut s, KeyCode::ControlLeft);
        press(&mut s, KeyCode::KeyQ);
        press(&mut s, KeyCode::F3);
        press(&mut s, KeyCode::KeyD);
        press(&mut s, KeyCode::KeyB);
        press(&mut s, KeyCode::KeyG);
        press(&mut s, KeyCode::Escape);
        assert_eq!(
            s.take_actions(),
            vec![
                Action::Hotbar(2),
                Action::Drop { whole_stack: false },
                Action::Drop { whole_stack: true },
                Action::ToggleDebug,
                Action::Debug(DebugCombo::ClearChat),
                Action::Pause,
            ]
        );
    }

    #[test]
    fn click_regrabs_without_attacking_then_attacks() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::Escape);
        s.take_actions();
        s.mouse_button(MouseButton::Left, true);
        assert!(s.wants_cursor_grab() && s.take_actions().is_empty());
        s.mouse_button(MouseButton::Left, false);
        s.mouse_button(MouseButton::Left, true);
        s.mouse_button(MouseButton::Right, true);
        assert!(s.attack_held());
        assert_eq!(s.take_actions(), vec![Action::Attack, Action::Use]);
    }

    #[test]
    fn focus_loss_pauses_and_stops_movement() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        s.set_focused(false);
        assert_eq!(s.movement(true, false, true, false).forward, 0.0);
        assert_eq!(s.take_actions(), vec![Action::Pause]);
        s.set_focused(true);
        assert!(!s.wants_cursor_grab());
    }

    #[test]
    fn chat_history_walks_up_and_down() {
        let mut s = InputState::default();
        for l in ["a", "b"] {
            press(&mut s, KeyCode::KeyT);
            typ(&mut s, KeyCode::KeyA, l);
            press(&mut s, KeyCode::Enter);
        }
        press(&mut s, KeyCode::KeyT);
        press(&mut s, KeyCode::ArrowUp);
        press(&mut s, KeyCode::ArrowUp);
        assert_eq!(s.chat_input(), Some("a"));
        press(&mut s, KeyCode::ArrowDown);
        press(&mut s, KeyCode::ArrowDown);
        assert_eq!(s.chat_input(), Some(""));
    }

    #[test]
    fn chat_rejects_section_sign_and_del() {
        assert!(!allowed_char('\u{a7}'));
        assert!(!allowed_char('\u{7f}'));
        assert!(!allowed_char('\n'));
        assert!(allowed_char(' ') && allowed_char('é'));
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        typ(&mut s, KeyCode::KeyA, "a\u{a7}b\u{7f}c");
        assert_eq!(s.chat_input(), Some("abc"));
    }

    #[test]
    fn chat_history_keeps_and_restores_draft() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        typ(&mut s, KeyCode::KeyA, "draft");
        press(&mut s, KeyCode::ArrowUp);
        assert_eq!(s.chat_input(), Some("draft"));
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::KeyT);
        typ(&mut s, KeyCode::KeyA, "wip");
        press(&mut s, KeyCode::ArrowUp);
        assert_eq!(s.chat_input(), Some("draft"));
        press(&mut s, KeyCode::ArrowDown);
        assert_eq!(s.chat_input(), Some("wip"));
    }
}
