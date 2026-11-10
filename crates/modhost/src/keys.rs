//! winit key to LWJGL 2 `Keyboard` key code, the numbers 1.8.9 mods read.

use winit::keyboard::KeyCode::{self, *};

/// Key and LWJGL 2 code, one row per key.
const CODES: &[(KeyCode, i32)] = &[
    (Escape, 1),
    (Digit1, 2),
    (Digit2, 3),
    (Digit3, 4),
    (Digit4, 5),
    (Digit5, 6),
    (Digit6, 7),
    (Digit7, 8),
    (Digit8, 9),
    (Digit9, 10),
    (Digit0, 11),
    (Minus, 12),
    (Equal, 13),
    (Backspace, 14),
    (Tab, 15),
    (KeyQ, 16),
    (KeyW, 17),
    (KeyE, 18),
    (KeyR, 19),
    (KeyT, 20),
    (KeyY, 21),
    (KeyU, 22),
    (KeyI, 23),
    (KeyO, 24),
    (KeyP, 25),
    (BracketLeft, 26),
    (BracketRight, 27),
    (Enter, 28),
    (ControlLeft, 29),
    (KeyA, 30),
    (KeyS, 31),
    (KeyD, 32),
    (KeyF, 33),
    (KeyG, 34),
    (KeyH, 35),
    (KeyJ, 36),
    (KeyK, 37),
    (KeyL, 38),
    (Semicolon, 39),
    (Quote, 40),
    (Backquote, 41),
    (ShiftLeft, 42),
    (Backslash, 43),
    (KeyZ, 44),
    (KeyX, 45),
    (KeyC, 46),
    (KeyV, 47),
    (KeyB, 48),
    (KeyN, 49),
    (KeyM, 50),
    (Comma, 51),
    (Period, 52),
    (Slash, 53),
    (ShiftRight, 54),
    (AltLeft, 56),
    (Space, 57),
    (CapsLock, 58),
    (F1, 59),
    (F2, 60),
    (F3, 61),
    (F4, 62),
    (F5, 63),
    (F6, 64),
    (F7, 65),
    (F8, 66),
    (F9, 67),
    (F10, 68),
    (F11, 87),
    (F12, 88),
    (ControlRight, 157),
    (AltRight, 184),
    (Home, 199),
    (ArrowUp, 200),
    (PageUp, 201),
    (ArrowLeft, 203),
    (ArrowRight, 205),
    (End, 207),
    (ArrowDown, 208),
    (PageDown, 209),
    (Insert, 210),
    (Delete, 211),
];

pub fn lwjgl(code: KeyCode) -> Option<i32> {
    CODES.iter().find(|(k, _)| *k == code).map(|&(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_keys_match_lwjgl() {
        let w: Vec<_> = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD]
            .into_iter()
            .map(lwjgl)
            .collect();
        assert_eq!(w, [Some(17), Some(30), Some(31), Some(32)]);
        assert_eq!(lwjgl(KeyCode::Space), Some(57));
        assert_eq!(lwjgl(KeyCode::NumLock), None);
    }
}
