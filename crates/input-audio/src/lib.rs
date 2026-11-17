//! Input state: vanilla 1.8.9 key bindings, mouse look, mouse buttons, wheel, chat entry.
mod actions;
mod autocomplete;
mod keys;
pub mod sound;

pub use actions::{scroll_slot, Action, DebugCombo};
pub use keys::{control_from_lwjgl, lwjgl_from_control, Bind, Control, KeyBindings};

use gui::screens::{Metrics, Mods, TextField};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

/// Vanilla waits this long after focus loss before it pauses.
const FOCUS_PAUSE_DELAY: Duration = Duration::from_millis(500);

/// The font widths and the clipboard text a chat key needs.
type ChatEnv<'a> = (&'a dyn Metrics, &'a mut String);

/// Widths for callers with no font (`on_key`, tests): every char is 6 units wide.
struct Mono;

impl Metrics for Mono {
    fn width(&self, s: &str) -> i32 {
        6 * s.chars().count() as i32
    }
}

/// The text before and after the caret.
fn split_at_caret(field: &TextField) -> (&str, &str) {
    let at = field
        .text
        .char_indices()
        .nth(field.cursor())
        .map_or(field.text.len(), |(b, _)| b);
    field.text.split_at(at)
}

/// Puts `head + tail` in the field with the caret after `head`; a no-op when `head` is unchanged.
fn replace_head(field: &mut TextField, head: String, tail: &str, m: &dyn Metrics) {
    if field.text == format!("{head}{tail}") {
        return;
    }
    let caret = head.chars().count();
    field.set(&format!("{head}{tail}"), m);
    field.set_cursor(caret, m);
}

/// Vanilla look scale: degrees per scaled mouse pixel.
const DEGREES_PER_PIXEL: f32 = 0.15;
/// Vanilla 1.8.9 chat limit.
const CHAT_MAX_CHARS: usize = 100;
/// Vanilla `GuiNewChat` keeps 100 sent lines.
const CHAT_HISTORY: usize = 100;
/// GUI size before the engine reports one: the field only needs a width to scroll.
const DEFAULT_GUI_SIZE: (i32, i32) = (640, 480);
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
        KeyCode::KeyB => DebugCombo::Hitboxes,
        KeyCode::KeyD => DebugCombo::ClearChat,
        KeyCode::KeyH => DebugCombo::AdvancedTooltips,
        KeyCode::KeyP => DebugCombo::PauseOnLostFocus,
        KeyCode::KeyQ => DebugCombo::Help,
        KeyCode::KeyS | KeyCode::KeyT => DebugCombo::ReloadResources,
        KeyCode::KeyF => DebugCombo::RenderDistance,
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
    /// The last `setSprinting` call, for the speed attribute's sprint modifier; see `take_sprint_change`.
    sprint_change: Option<bool>,
    /// `sprintingTicksLeft`: 600 after `setSprinting(true)`, 0 after `setSprinting(false)`.
    sprint_ticks: u16,
    sprint_timer: u8,
    /// Blindness: the sprint latch cannot start (`EntityPlayerSP.onLivingUpdate`); a running sprint goes on.
    pub blind: bool,
    /// `isRiding()`: item use then keeps the double-tap window.
    pub riding: bool,
    /// Sneak as the mod host's `EntityPlayerSP` left it this tick; `None` without a host.
    host_sneak: Option<bool>,
    was_full_forward: bool,
    was_sneaking: bool,
    scroll_px: f64,
    look: (f32, f32),
    /// `GuiChat.inputField`.
    chat: Option<TextField>,
    gui_size: (i32, i32),
    shift_down: bool,
    autocomplete: autocomplete::Autocomplete,
    chat_note: Option<String>,
    submit: Option<String>,
    history: Vec<String>,
    history_pos: Option<usize>,
    history_draft: String,
    grab_requested: bool,
    screen_open: bool,
    focused: bool,
    /// When the window lost focus; `None` while focused.
    focus_lost_at: Option<Instant>,
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
            sprint_change: None,
            sprint_ticks: 0,
            sprint_timer: 0,
            blind: false,
            riding: false,
            host_sneak: None,
            was_full_forward: false,
            was_sneaking: false,
            scroll_px: 0.0,
            look: (0.0, 0.0),
            chat: None,
            gui_size: DEFAULT_GUI_SIZE,
            shift_down: false,
            submit: None,
            history: Vec::new(),
            history_pos: None,
            history_draft: String::new(),
            autocomplete: Default::default(),
            chat_note: None,
            grab_requested: true,
            screen_open: false,
            focused: true,
            focus_lost_at: None,
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
        self.on_window_event_in(event, &Mono, &mut String::new());
    }

    /// `on_window_event` with what the chat field needs: the font widths that scroll it, and the
    /// clipboard text for Ctrl+C, Ctrl+X and Ctrl+V (read before and written back after).
    pub fn on_window_event_in(&mut self, event: &WindowEvent, m: &dyn Metrics, clip: &mut String) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let text = event.text.as_deref();
                    self.key_event(
                        code,
                        event.state.is_pressed(),
                        event.repeat,
                        text,
                        (m, clip),
                    );
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
        self.set_focused_at(focused, Instant::now());
    }

    fn set_focused_at(&mut self, focused: bool, now: Instant) {
        self.focused = focused;
        if focused {
            self.focus_lost_at = None;
            return;
        }
        self.focus_lost_at.get_or_insert(now);
        self.release_all();
    }

    /// `GameRenderer.render` (1.8.9, lines 776-780): once the window has been unfocused for
    /// more than 500 ms, `pauseGame` opens the pause menu; it does nothing with a screen open.
    fn poll_focus_pause(&mut self, now: Instant) {
        let Some(lost) = self.focus_lost_at else {
            return;
        };
        if self.pause_on_lost_focus
            && now.saturating_duration_since(lost) > FOCUS_PAUSE_DELAY
            && self.chat.is_none()
            && !self.screen_open
            && self.grab_requested
        {
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
            // `GuiChat.handleMouseInput`: 7 lines a step, 1 with Shift held.
            let lines = if self.shift_down { 1 } else { 7 };
            self.actions.push_back(Action::ChatScroll(step * lines));
        } else if !self.screen_open {
            self.actions.push_back(Action::HotbarScroll(step));
        }
    }

    /// Key press/release. `text` is the character text of a press, used in chat mode.
    pub fn on_key(&mut self, code: KeyCode, pressed: bool, text: Option<&str>) {
        self.key_event(code, pressed, false, text, (&Mono, &mut String::new()));
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

    fn key_event(
        &mut self,
        code: KeyCode,
        pressed: bool,
        repeat: bool,
        text: Option<&str>,
        env: ChatEnv,
    ) {
        match code {
            KeyCode::ControlLeft | KeyCode::ControlRight => self.ctrl_down = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.shift_down = pressed,
            KeyCode::F3 if !pressed => self.f3_down = false,
            _ => {}
        }
        if self.screenshot_key(code, pressed && !repeat) {
            return;
        }
        if self.chat.is_some() || self.screen_open {
            self.modal_key(code, pressed, repeat, text, env);
            return;
        }
        if pressed && !repeat && self.hard_coded_key(code) {
            return;
        }
        self.control(Control::Key(code), pressed, repeat);
    }

    /// Key while chat or a GUI screen owns the keyboard.
    fn modal_key(
        &mut self,
        code: KeyCode,
        pressed: bool,
        repeat: bool,
        text: Option<&str>,
        env: ChatEnv,
    ) {
        if self.chat.is_some() {
            if pressed {
                self.chat_key(code, text, env);
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
        self.autocomplete = Default::default();
        let mut field = self.new_chat_field();
        field.set(&initial, &Mono);
        self.chat = Some(field);
    }

    /// `GuiChat.initGui`: a 100-unit field at the bottom, no frame, always focused.
    fn new_chat_field(&self) -> TextField {
        let (w, h) = self.gui_size;
        let mut field = TextField::new(4, h - 12, w - 4, 12);
        field.max_len = CHAT_MAX_CHARS;
        field.background = false;
        field.set_focused(true);
        field
    }

    /// The scaled GUI size, which places and sizes the chat field (`GuiChat.initGui`).
    pub fn set_gui_size(&mut self, w: i32, h: i32) {
        self.gui_size = (w, h);
        if let Some(f) = &mut self.chat {
            (f.x, f.y, f.w) = (4, h - 12, w - 4);
        }
    }

    /// The chat field, for drawing; `None` while chat is closed.
    pub fn chat_field(&self) -> Option<&TextField> {
        self.chat.as_ref()
    }

    /// `GuiChat.updateScreen`: the caret blink counter.
    pub fn tick_chat(&mut self) {
        if let Some(f) = &mut self.chat {
            f.tick();
        }
    }

    fn chat_key(&mut self, code: KeyCode, text: Option<&str>, (m, clip): ChatEnv) {
        let Some(field) = self.chat.as_mut() else {
            return;
        };
        // `GuiChat.keyTyped`: Tab completes, every other key ends the wait and the name cycle.
        let note = Self::complete(&mut self.autocomplete, field, m, code == KeyCode::Tab);
        self.chat_note = note.or(self.chat_note.take());
        match code {
            KeyCode::Tab => {}
            KeyCode::Escape => self.chat = None,
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit_chat(),
            KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::PageUp | KeyCode::PageDown => {
                self.scroll_key(code, m)
            }
            _ => self.field_key(code, text, (m, clip)),
        }
    }

    /// Sent history on Up and Down, a page of chat lines on PageUp and PageDown.
    fn scroll_key(&mut self, code: KeyCode, m: &dyn Metrics) {
        match code {
            KeyCode::ArrowUp => self.history_step(-1, m),
            KeyCode::ArrowDown => self.history_step(1, m),
            KeyCode::PageUp => self.actions.push_back(Action::ChatPage(1)),
            _ => self.actions.push_back(Action::ChatPage(-1)),
        }
    }

    /// The keys `GuiTextField.textboxKeyTyped` handles: caret, selection, delete, clipboard, text.
    fn field_key(&mut self, code: KeyCode, text: Option<&str>, (m, clip): ChatEnv) {
        let Some(field) = self.chat.as_mut() else {
            return;
        };
        let mods = Mods {
            ctrl: self.ctrl_down,
            shift: self.shift_down,
        };
        let lwjgl = lwjgl_from_control(Control::Key(code)).unwrap_or(0);
        // The key code goes with the first char only; more chars are an IME commit.
        let mut chars = text.unwrap_or("").chars();
        let first = chars.next().unwrap_or('\0');
        field.key(lwjgl, first, mods, m, clip);
        chars.for_each(|c| {
            field.key(0, c, Mods::default(), m, clip);
        });
    }

    /// Runs `autocomplete` on the text before the caret, as vanilla does, and keeps the rest.
    fn complete(
        ac: &mut autocomplete::Autocomplete,
        field: &mut TextField,
        m: &dyn Metrics,
        tab: bool,
    ) -> Option<String> {
        let (head, tail) = split_at_caret(field);
        let mut head = head.to_owned();
        let tail = tail.to_owned();
        let note = ac.key(tab, &mut head);
        replace_head(field, head, &tail, m);
        note
    }

    fn submit_chat(&mut self) {
        // Vanilla 1.8.9 uses Java `String.trim()`, which strips only chars <= U+0020 (keeps NBSP).
        let line = self.chat.take().map(|f| f.text).unwrap_or_default();
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
    fn history_step(&mut self, dir: i32, m: &dyn Metrics) {
        let len = self.history.len();
        let cur = self.history_pos.unwrap_or(len);
        let next = (cur as i32 + dir).clamp(0, len as i32) as usize;
        if next == cur {
            return;
        }
        let Some(field) = self.chat.as_mut() else {
            return;
        };
        if next == len {
            self.history_pos = None;
            field.set(&std::mem::take(&mut self.history_draft), m);
            return;
        }
        if self.history_pos.is_none() {
            self.history_draft = field.text.clone();
        }
        self.history_pos = Some(next);
        field.set(&self.history[next], m);
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
        self.poll_focus_pause(Instant::now());
        self.actions.drain(..).collect()
    }

    /// Call once per game tick: `EntityPlayerSP.onLivingUpdate` sprint latch.
    /// `can_sprint` is `foodLevel > 6 || canFly`; `using_item` is `isUsingItem()`.
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
        if self.sprint_ticks > 0 {
            self.sprint_ticks -= 1;
            if self.sprint_ticks == 0 {
                self.set_sprinting(false);
            }
        }
        self.sprint_timer = self.sprint_timer.saturating_sub(1);
        if using_item && !self.riding {
            self.sprint_timer = 0;
        }
        self.update_sprint(on_ground, can_sprint && full && !self.blind, sprint_key);
        if self.sprinting && (!full || collided || !can_sprint) {
            self.set_sprinting(false);
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
        if sprint != self.sprinting {
            self.set_sprinting(sprint);
        }
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

    /// `setSprinting(false)`: a held sprint key starts the sprint again on the next `movement` call.
    pub fn stop_sprint(&mut self) {
        self.set_sprinting(false);
    }

    /// `EntityLivingBase.setSprinting`: the flag and the speed modifier change together.
    fn set_sprinting(&mut self, sprinting: bool) {
        self.sprinting = sprinting;
        self.sprint_ticks = if sprinting { 600 } else { 0 };
        self.sprint_change = Some(sprinting);
    }

    /// The newest `setSprinting` argument since the last call; the caller applies it with `Speed::set_sprint`.
    pub fn take_sprint_change(&mut self) -> Option<bool> {
        self.sprint_change.take()
    }

    /// An own S1C entity flag write (`handleEntityData`): sets the sprint flag, not the speed modifier.
    pub fn set_sprint_flag(&mut self, sprinting: bool) {
        self.sprinting = sprinting;
    }

    fn update_sprint(&mut self, on_ground: bool, can_start: bool, sprint_key: bool) {
        let fresh = on_ground && !self.was_sneaking && !self.was_full_forward;
        if fresh && can_start && !self.sprinting {
            if self.sprint_timer == 0 && !sprint_key {
                self.sprint_timer = 7;
            } else {
                self.set_sprinting(true);
            }
        }
        if !self.sprinting && can_start && sprint_key {
            self.set_sprinting(true);
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

    /// `GameSettings.keyBindSpectatorOutlines.isKeyDown()`.
    pub fn spectator_outlines_held(&self) -> bool {
        self.is_held(Bind::SpectatorOutlines)
    }

    pub fn chat_input(&self) -> Option<&str> {
        self.chat.as_ref().map(|f| f.text.as_str())
    }

    /// `GuiChat.setText(text, overwrite)` / `GuiTextField.writeText`: a chat click replaces the field
    /// or writes at the caret. Text over 100 UTF-16 units is cut, like the vanilla text field.
    pub fn set_chat_text(&mut self, text: &str, overwrite: bool, m: &dyn Metrics) {
        let Some(field) = self.chat.as_mut() else {
            return;
        };
        if overwrite {
            field.set(text, m);
        } else {
            field.write(text, m);
        }
    }

    /// The text a Tab asked the server to complete (`C14PacketTabComplete`).
    pub fn take_tab_request(&mut self) -> Option<String> {
        self.autocomplete.take_request()
    }

    /// `GuiChat.onAutocompleteResponse`; the returned line is the name list for the chat.
    pub fn tab_response(&mut self, matches: &[String], m: &dyn Metrics) -> Option<String> {
        let field = self.chat.as_mut()?;
        let (head, tail) = split_at_caret(field);
        let mut head = head.to_owned();
        let tail = tail.to_owned();
        let note = self.autocomplete.response(matches, &mut head);
        replace_head(field, head, &tail, m);
        note
    }

    /// The name list a Tab cycle printed, for the chat.
    pub fn take_chat_note(&mut self) -> Option<String> {
        self.chat_note.take()
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
    fn server_flag_write_clears_sprint_without_a_modifier_change() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        press(&mut s, KeyCode::ControlLeft);
        assert!(s.movement(true, false, true, false).sprint);
        assert_eq!(s.take_sprint_change(), Some(true));
        s.on_key(KeyCode::ControlLeft, false, None);
        s.set_sprint_flag(false);
        assert_eq!(s.take_sprint_change(), None);
        assert!(!s.movement(true, false, true, false).sprint);
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

    /// `sprintingTicksLeft`: a double-tap sprint ends after 600 ticks; a held sprint key starts it again.
    #[test]
    fn sprint_ends_after_600_ticks_unless_the_key_is_held() {
        let tick = |s: &mut InputState| s.movement(true, false, true, false).sprint;
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        tick(&mut s);
        s.on_key(KeyCode::KeyW, false, None);
        tick(&mut s);
        press(&mut s, KeyCode::KeyW);
        assert!(tick(&mut s));
        for _ in 0..599 {
            assert!(tick(&mut s));
        }
        assert!(!tick(&mut s), "tick 600 ends the sprint");
        assert_eq!(s.take_sprint_change(), Some(false));

        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        press(&mut s, KeyCode::ControlLeft);
        assert!(tick(&mut s));
        for _ in 0..1300 {
            assert!(tick(&mut s));
        }
    }

    /// `EntityPlayerSP.onLivingUpdate`: item use sets `sprintToggleTimer` to 0 unless riding.
    #[test]
    fn using_an_item_clears_the_double_tap_window() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        s.movement(true, false, true, false);
        s.on_key(KeyCode::KeyW, false, None);
        s.movement(true, false, true, true);
        s.movement(true, false, true, false);
        press(&mut s, KeyCode::KeyW);
        assert!(!s.movement(true, false, true, false).sprint);
        s.riding = true;
        s.on_key(KeyCode::KeyW, false, None);
        s.movement(true, false, true, false);
        press(&mut s, KeyCode::KeyW);
        s.movement(true, false, true, true);
        s.on_key(KeyCode::KeyW, false, None);
        s.movement(true, false, true, false);
        press(&mut s, KeyCode::KeyW);
        assert!(s.movement(true, false, true, false).sprint);
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
                Action::Debug(DebugCombo::Hitboxes),
                Action::Pause,
            ]
        );
    }

    /// `processKeyF3` in the 1.8.9 jar: S (31) and T (20) call `reloadResources`; F (33) calls
    /// `GameOptions.set(RENDER_DISTANCE, ±1)`, which only saves.
    #[test]
    fn f3_s_t_f_queue_their_actions() {
        for (key, combo) in [
            (KeyCode::KeyS, DebugCombo::ReloadResources),
            (KeyCode::KeyT, DebugCombo::ReloadResources),
            (KeyCode::KeyF, DebugCombo::RenderDistance),
        ] {
            let mut s = InputState::default();
            press(&mut s, KeyCode::F3);
            press(&mut s, key);
            assert_eq!(
                s.take_actions(),
                vec![Action::ToggleDebug, Action::Debug(combo)],
                "{key:?}"
            );
        }
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
    fn focus_loss_stops_movement_and_pauses_after_500_ms() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyW);
        s.set_focused_at(false, t0);
        assert_eq!(s.movement(true, false, true, false).forward, 0.0);
        s.poll_focus_pause(t0 + ms(100));
        assert_eq!(s.take_actions(), vec![]);
        s.poll_focus_pause(t0 + ms(500));
        assert_eq!(s.take_actions(), vec![]);
        s.poll_focus_pause(t0 + ms(600));
        s.poll_focus_pause(t0 + ms(700));
        assert_eq!(s.take_actions(), vec![Action::Pause]);
        s.set_focused_at(true, t0 + ms(800));
        assert!(!s.wants_cursor_grab());
    }

    #[test]
    fn short_focus_steal_never_pauses() {
        let t0 = Instant::now();
        let mut s = InputState::default();
        s.set_focused_at(false, t0);
        s.set_focused_at(true, t0 + Duration::from_millis(100));
        s.poll_focus_pause(t0 + Duration::from_secs(5));
        assert_eq!(s.take_actions(), vec![]);
    }

    #[test]
    fn open_screen_blocks_focus_pause() {
        let t0 = Instant::now();
        let mut s = InputState::default();
        s.set_screen_open(true);
        s.set_focused_at(false, t0);
        s.poll_focus_pause(t0 + Duration::from_millis(600));
        assert_eq!(s.take_actions(), vec![]);
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
    fn chat_caret_moves_edits_and_pastes() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        typ(&mut s, KeyCode::KeyA, "helo");
        press(&mut s, KeyCode::ArrowLeft);
        typ(&mut s, KeyCode::KeyL, "l");
        assert_eq!(s.chat_input(), Some("hello"));
        assert_eq!(s.chat_field().map(|f| f.cursor()), Some(4));
        press(&mut s, KeyCode::Home);
        press(&mut s, KeyCode::Delete);
        assert_eq!(s.chat_input(), Some("ello"));
        // Ctrl+A, Ctrl+C, End, Ctrl+V on a clipboard.
        let mut clip = String::new();
        let chord = |s: &mut InputState, k: KeyCode, clip: &mut String| {
            s.key_event(KeyCode::ControlLeft, true, false, None, (&Mono, clip));
            s.key_event(k, true, false, None, (&Mono, clip));
            s.key_event(KeyCode::ControlLeft, false, false, None, (&Mono, clip));
        };
        chord(&mut s, KeyCode::KeyA, &mut clip);
        chord(&mut s, KeyCode::KeyC, &mut clip);
        assert_eq!(clip, "ello");
        press(&mut s, KeyCode::End);
        chord(&mut s, KeyCode::KeyV, &mut clip);
        assert_eq!(s.chat_input(), Some("elloello"));
        assert_eq!(s.chat_field().map(|f| f.cursor()), Some(8));
    }

    #[test]
    fn chat_paste_stops_at_100_utf16_units() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        let mut clip = "a".repeat(99) + "😀";
        s.key_event(KeyCode::ControlLeft, true, false, None, (&Mono, &mut clip));
        s.key_event(KeyCode::KeyV, true, false, None, (&Mono, &mut clip));
        assert_eq!(s.chat_input().map(|t| t.encode_utf16().count()), Some(99));
    }

    #[test]
    fn chat_wheel_scrolls_7_lines_or_1_with_shift() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        s.wheel(1.0);
        press(&mut s, KeyCode::ShiftLeft);
        s.wheel(-1.0);
        assert_eq!(
            s.take_actions(),
            [Action::ChatScroll(7), Action::ChatScroll(-1)]
        );
    }

    #[test]
    fn chat_page_keys_queue_a_page_scroll() {
        let mut s = InputState::default();
        press(&mut s, KeyCode::KeyT);
        press(&mut s, KeyCode::PageUp);
        press(&mut s, KeyCode::PageDown);
        assert_eq!(
            s.take_actions(),
            [Action::ChatPage(1), Action::ChatPage(-1)]
        );
    }

    #[test]
    fn chat_rejects_section_sign_and_del() {
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
