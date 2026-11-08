//! Vanilla 1.8.9 key bindings: defaults, `options.txt` names and LWJGL key codes.

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// A physical key or mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// The rebindable vanilla `KeyBinding`s that drive the game. F1, F3 and Esc are hard-coded
/// in vanilla and live in `InputState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Forward,
    Left,
    Back,
    Right,
    Jump,
    Sneak,
    Sprint,
    Inventory,
    Drop,
    Chat,
    Command,
    PlayerList,
    Attack,
    Use,
    PickBlock,
    Screenshot,
    Perspective,
    Fullscreen,
    /// Hotbar slot 0..=8.
    Hotbar(u8),
}

const BIND_COUNT: usize = 27;

impl Bind {
    pub const ALL: [Bind; BIND_COUNT] = [
        Bind::Forward,
        Bind::Left,
        Bind::Back,
        Bind::Right,
        Bind::Jump,
        Bind::Sneak,
        Bind::Sprint,
        Bind::Inventory,
        Bind::Drop,
        Bind::Chat,
        Bind::Command,
        Bind::PlayerList,
        Bind::Attack,
        Bind::Use,
        Bind::PickBlock,
        Bind::Screenshot,
        Bind::Perspective,
        Bind::Fullscreen,
        Bind::Hotbar(0),
        Bind::Hotbar(1),
        Bind::Hotbar(2),
        Bind::Hotbar(3),
        Bind::Hotbar(4),
        Bind::Hotbar(5),
        Bind::Hotbar(6),
        Bind::Hotbar(7),
        Bind::Hotbar(8),
    ];

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0)
    }

    /// The `options.txt` key without the `key_` prefix.
    pub fn option_name(self) -> String {
        let name = match self {
            Bind::Forward => "forward",
            Bind::Left => "left",
            Bind::Back => "back",
            Bind::Right => "right",
            Bind::Jump => "jump",
            Bind::Sneak => "sneak",
            Bind::Sprint => "sprint",
            Bind::Inventory => "inventory",
            Bind::Drop => "drop",
            Bind::Chat => "chat",
            Bind::Command => "command",
            Bind::PlayerList => "playerlist",
            Bind::Attack => "attack",
            Bind::Use => "use",
            Bind::PickBlock => "pickItem",
            Bind::Screenshot => "screenshot",
            Bind::Perspective => "togglePerspective",
            Bind::Fullscreen => "fullscreen",
            Bind::Hotbar(n) => return format!("key_key.hotbar.{}", n + 1),
        };
        format!("key_key.{name}")
    }

    /// `GameSettings` default key.
    pub fn default_control(self) -> Control {
        use KeyCode::*;
        let key = match self {
            Bind::Forward => KeyW,
            Bind::Left => KeyA,
            Bind::Back => KeyS,
            Bind::Right => KeyD,
            Bind::Jump => Space,
            Bind::Sneak => ShiftLeft,
            Bind::Sprint => ControlLeft,
            Bind::Inventory => KeyE,
            Bind::Drop => KeyQ,
            Bind::Chat => KeyT,
            Bind::Command => Slash,
            Bind::PlayerList => Tab,
            Bind::Screenshot => F2,
            Bind::Perspective => F5,
            Bind::Fullscreen => F11,
            Bind::Attack => return Control::Mouse(MouseButton::Left),
            Bind::Use => return Control::Mouse(MouseButton::Right),
            Bind::PickBlock => return Control::Mouse(MouseButton::Middle),
            Bind::Hotbar(n) => [
                Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
            ][usize::from(n.min(8))],
        };
        Control::Key(key)
    }
}

/// Current control for every [`Bind`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBindings([Control; BIND_COUNT]);

impl Default for KeyBindings {
    fn default() -> Self {
        Self(Bind::ALL.map(Bind::default_control))
    }
}

impl KeyBindings {
    pub fn get(&self, bind: Bind) -> Control {
        self.0[bind.index()]
    }

    pub fn set(&mut self, bind: Bind, control: Control) {
        self.0[bind.index()] = control;
    }

    pub(crate) fn matching(&self, control: Control) -> impl Iterator<Item = Bind> + '_ {
        Bind::ALL
            .into_iter()
            .filter(move |b| self.get(*b) == control)
    }

    /// Read `key_key.*:<lwjgl code>` lines of vanilla `options.txt`. Unknown names and codes
    /// keep the default, like `GameSettings.loadOptions`.
    pub fn from_options_txt(text: &str) -> Self {
        let mut out = Self::default();
        for line in text.lines() {
            let Some((name, code)) = line.split_once(':') else {
                continue;
            };
            let (Some(bind), Ok(code)) = (
                Bind::ALL.into_iter().find(|b| b.option_name() == name),
                code.trim().parse::<i32>(),
            ) else {
                continue;
            };
            if let Some(c) = control_from_lwjgl(code) {
                out.set(bind, c);
            }
        }
        out
    }
}

/// LWJGL 2 keyboard codes (positive) and mouse buttons (-100 + button).
pub fn control_from_lwjgl(code: i32) -> Option<Control> {
    match code {
        -100 => Some(Control::Mouse(MouseButton::Left)),
        -99 => Some(Control::Mouse(MouseButton::Right)),
        -98 => Some(Control::Mouse(MouseButton::Middle)),
        -97 => Some(Control::Mouse(MouseButton::Back)),
        -96 => Some(Control::Mouse(MouseButton::Forward)),
        _ => LWJGL_KEYS
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, k)| Control::Key(*k)),
    }
}

pub fn lwjgl_from_control(control: Control) -> Option<i32> {
    match control {
        Control::Mouse(MouseButton::Left) => Some(-100),
        Control::Mouse(MouseButton::Right) => Some(-99),
        Control::Mouse(MouseButton::Middle) => Some(-98),
        Control::Mouse(MouseButton::Back) => Some(-97),
        Control::Mouse(MouseButton::Forward) => Some(-96),
        Control::Mouse(MouseButton::Other(_)) => None,
        Control::Key(k) => LWJGL_KEYS.iter().find(|(_, c)| *c == k).map(|(c, _)| *c),
    }
}

use KeyCode as K;
const LWJGL_KEYS: &[(i32, KeyCode)] = &[
    (1, K::Escape),
    (2, K::Digit1),
    (3, K::Digit2),
    (4, K::Digit3),
    (5, K::Digit4),
    (6, K::Digit5),
    (7, K::Digit6),
    (8, K::Digit7),
    (9, K::Digit8),
    (10, K::Digit9),
    (11, K::Digit0),
    (12, K::Minus),
    (13, K::Equal),
    (14, K::Backspace),
    (15, K::Tab),
    (16, K::KeyQ),
    (17, K::KeyW),
    (18, K::KeyE),
    (19, K::KeyR),
    (20, K::KeyT),
    (21, K::KeyY),
    (22, K::KeyU),
    (23, K::KeyI),
    (24, K::KeyO),
    (25, K::KeyP),
    (26, K::BracketLeft),
    (27, K::BracketRight),
    (28, K::Enter),
    (29, K::ControlLeft),
    (30, K::KeyA),
    (31, K::KeyS),
    (32, K::KeyD),
    (33, K::KeyF),
    (34, K::KeyG),
    (35, K::KeyH),
    (36, K::KeyJ),
    (37, K::KeyK),
    (38, K::KeyL),
    (39, K::Semicolon),
    (40, K::Quote),
    (41, K::Backquote),
    (42, K::ShiftLeft),
    (43, K::Backslash),
    (44, K::KeyZ),
    (45, K::KeyX),
    (46, K::KeyC),
    (47, K::KeyV),
    (48, K::KeyB),
    (49, K::KeyN),
    (50, K::KeyM),
    (51, K::Comma),
    (52, K::Period),
    (53, K::Slash),
    (54, K::ShiftRight),
    (56, K::AltLeft),
    (57, K::Space),
    (58, K::CapsLock),
    (59, K::F1),
    (60, K::F2),
    (61, K::F3),
    (62, K::F4),
    (63, K::F5),
    (64, K::F6),
    (65, K::F7),
    (66, K::F8),
    (67, K::F9),
    (68, K::F10),
    (87, K::F11),
    (88, K::F12),
    (157, K::ControlRight),
    (184, K::AltRight),
    (199, K::Home),
    (200, K::ArrowUp),
    (201, K::PageUp),
    (203, K::ArrowLeft),
    (205, K::ArrowRight),
    (207, K::End),
    (208, K::ArrowDown),
    (209, K::PageDown),
    (210, K::Insert),
    (211, K::Delete),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_txt_rebinds_and_ignores_garbage() {
        let b = KeyBindings::from_options_txt(
            "key_key.forward:200\nkey_key.attack:-99\nkey_key.jump:9999\nfoo:1\nkey_key.drop:x\nkey_key.hotbar.3:30",
        );
        assert_eq!(b.get(Bind::Forward), Control::Key(KeyCode::ArrowUp));
        assert_eq!(b.get(Bind::Attack), Control::Mouse(MouseButton::Right));
        assert_eq!(b.get(Bind::Jump), Control::Key(KeyCode::Space));
        assert_eq!(b.get(Bind::Drop), Control::Key(KeyCode::KeyQ));
        assert_eq!(b.get(Bind::Hotbar(2)), Control::Key(KeyCode::KeyA));
    }

    #[test]
    fn lwjgl_codes_round_trip() {
        for (code, key) in LWJGL_KEYS {
            assert_eq!(lwjgl_from_control(Control::Key(*key)), Some(*code));
        }
    }
}
