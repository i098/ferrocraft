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

    /// The `options.txt` key, e.g. `key_key.forward`.
    pub fn option_name(self) -> String {
        match self {
            Bind::Hotbar(n) => format!("key_key.hotbar.{}", n + 1),
            _ => format!("key_key.{}", BASE_BINDS[self.index()].0),
        }
    }

    /// `GameSettings` default key.
    pub fn default_control(self) -> Control {
        match self {
            Bind::Hotbar(n) => Control::Key(HOTBAR_KEYS[usize::from(n.min(8))]),
            _ => BASE_BINDS[self.index()].1,
        }
    }
}

use KeyCode as K;
const fn key(k: KeyCode) -> Control {
    Control::Key(k)
}
const fn mouse(b: MouseButton) -> Control {
    Control::Mouse(b)
}

/// `options.txt` name and default for each non-hotbar bind, in `Bind::ALL` order.
const BASE_BINDS: [(&str, Control); 18] = [
    ("forward", key(K::KeyW)),
    ("left", key(K::KeyA)),
    ("back", key(K::KeyS)),
    ("right", key(K::KeyD)),
    ("jump", key(K::Space)),
    ("sneak", key(K::ShiftLeft)),
    ("sprint", key(K::ControlLeft)),
    ("inventory", key(K::KeyE)),
    ("drop", key(K::KeyQ)),
    ("chat", key(K::KeyT)),
    ("command", key(K::Slash)),
    ("playerlist", key(K::Tab)),
    ("attack", mouse(MouseButton::Left)),
    ("use", mouse(MouseButton::Right)),
    ("pickItem", mouse(MouseButton::Middle)),
    ("screenshot", key(K::F2)),
    ("togglePerspective", key(K::F5)),
    ("fullscreen", key(K::F11)),
];

const HOTBAR_KEYS: [KeyCode; 9] = [
    K::Digit1,
    K::Digit2,
    K::Digit3,
    K::Digit4,
    K::Digit5,
    K::Digit6,
    K::Digit7,
    K::Digit8,
    K::Digit9,
];

/// Order of `GameSettings.keyBindings` construction. `KeyBinding.resetKeyBindingArrayAndHash`
/// fills a key-code hash in this order, so the last bind on a shared key wins.
const VANILLA_ORDER: [Bind; BIND_COUNT] = [
    Bind::Forward,
    Bind::Left,
    Bind::Back,
    Bind::Right,
    Bind::Jump,
    Bind::Sneak,
    Bind::Sprint,
    Bind::Inventory,
    Bind::Use,
    Bind::Drop,
    Bind::Attack,
    Bind::PickBlock,
    Bind::Chat,
    Bind::PlayerList,
    Bind::Command,
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

/// Current control for every [`Bind`]; `None` is unbound (`options.txt` code 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBindings([Option<Control>; BIND_COUNT]);

impl Default for KeyBindings {
    fn default() -> Self {
        Self(Bind::ALL.map(|b| Some(b.default_control())))
    }
}

impl KeyBindings {
    pub fn get(&self, bind: Bind) -> Option<Control> {
        self.0[bind.index()]
    }

    pub fn set(&mut self, bind: Bind, control: Option<Control>) {
        self.0[bind.index()] = control;
    }

    /// The one bind vanilla's key-code hash resolves `control` to, if any.
    pub fn lookup(&self, control: Control) -> Option<Bind> {
        VANILLA_ORDER
            .into_iter()
            .rev()
            .find(|b| self.get(*b) == Some(control))
    }

    /// Read `key_key.*:<lwjgl code>` lines of vanilla `options.txt`. Code 0 and codes with no
    /// supported control leave the bind unbound, like a `GameSettings.loadOptions` key code
    /// that no pressed key matches. Unknown names and unparsable codes keep the default.
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
            out.set(bind, control_from_lwjgl(code));
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
    (55, K::NumpadMultiply),
    (69, K::NumLock),
    (70, K::ScrollLock),
    (71, K::Numpad7),
    (72, K::Numpad8),
    (73, K::Numpad9),
    (74, K::NumpadSubtract),
    (75, K::Numpad4),
    (76, K::Numpad5),
    (77, K::Numpad6),
    (78, K::NumpadAdd),
    (79, K::Numpad1),
    (80, K::Numpad2),
    (81, K::Numpad3),
    (82, K::Numpad0),
    (83, K::NumpadDecimal),
    (100, K::F13),
    (101, K::F14),
    (102, K::F15),
    (141, K::NumpadEqual),
    (156, K::NumpadEnter),
    (179, K::NumpadComma),
    (181, K::NumpadDivide),
    (183, K::PrintScreen),
    (197, K::Pause),
    (219, K::SuperLeft),
    (220, K::SuperRight),
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
        assert_eq!(b.get(Bind::Forward), Some(Control::Key(KeyCode::ArrowUp)));
        assert_eq!(
            b.get(Bind::Attack),
            Some(Control::Mouse(MouseButton::Right))
        );
        assert_eq!(b.get(Bind::Jump), None);
        assert_eq!(b.get(Bind::Drop), Some(Control::Key(KeyCode::KeyQ)));
        assert_eq!(b.get(Bind::Hotbar(2)), Some(Control::Key(KeyCode::KeyA)));
    }

    #[test]
    fn code_zero_unbinds_and_keypad_maps() {
        let b = KeyBindings::from_options_txt("key_key.drop:0\nkey_key.jump:82");
        assert_eq!(b.get(Bind::Drop), None);
        assert_eq!(b.lookup(Control::Key(KeyCode::KeyQ)), None);
        assert_eq!(b.get(Bind::Jump), Some(Control::Key(KeyCode::Numpad0)));
    }

    #[test]
    fn shared_key_resolves_to_one_bind() {
        let b = KeyBindings::from_options_txt("key_key.drop:18");
        assert_eq!(b.lookup(Control::Key(KeyCode::KeyE)), Some(Bind::Drop));
    }

    #[test]
    fn lwjgl_codes_round_trip() {
        for (code, key) in LWJGL_KEYS {
            assert_eq!(lwjgl_from_control(Control::Key(*key)), Some(*code));
        }
    }
}
