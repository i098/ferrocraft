//! Discrete game actions that `InputState` queues for the engine, like vanilla's key handling
//! in `Minecraft.runTick`.

/// `Minecraft.processKeyF3` combos (1.8.9), pressed while F3 is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugCombo {
    /// F3+A: reload chunks (`RenderGlobal.loadRenderers`).
    ReloadChunks,
    /// F3+D: clear chat.
    ClearChat,
    /// F3+H: advanced item tooltips.
    AdvancedTooltips,
    /// F3+P: pause when the window loses focus.
    PauseOnLostFocus,
    /// F3+Q: list the combos in chat.
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Left click press: attack the entity or start breaking the block under the crosshair.
    Attack,
    /// Right click press: use the held item or place a block.
    Use,
    /// Middle click: pick the block under the crosshair.
    PickBlock,
    /// Hotbar key 1-9 as slot 0..=8.
    Hotbar(u8),
    /// One wheel step in game: -1 or +1 as vanilla `Mouse.getEventDWheel` sign.
    /// Apply it with [`scroll_slot`].
    HotbarScroll(i32),
    /// One wheel step while chat is open: scroll the chat window.
    ChatScroll(i32),
    /// PageUp (+1) or PageDown (-1) in chat: `GuiNewChat.scroll` by a page less one line.
    ChatPage(i32),
    Inventory,
    /// Q drops one item, Ctrl+Q the whole stack.
    Drop {
        whole_stack: bool,
    },
    /// Esc in game: open the pause menu.
    Pause,
    /// Esc while a GUI screen is open: close it.
    CloseScreen,
    /// F3.
    ToggleDebug,
    /// F1.
    ToggleHud,
    /// F2.
    Screenshot,
    /// F4: switch the post-process effect off or back on.
    ToggleShader,
    /// F5.
    TogglePerspective,
    /// F11.
    ToggleFullscreen,
    Debug(DebugCombo),
}

/// `InventoryPlayer.changeCurrentItem`: the wheel direction is clamped to one step, up (+1)
/// moves left, and the hotbar wraps.
pub fn scroll_slot(current: u8, wheel: i32) -> u8 {
    let step = wheel.signum();
    (i32::from(current) - step).rem_euclid(9) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_up_moves_left_and_wraps_both_ways() {
        assert_eq!(scroll_slot(4, 1), 3);
        assert_eq!(scroll_slot(4, -1), 5);
        assert_eq!(scroll_slot(0, 1), 8);
        assert_eq!(scroll_slot(8, -1), 0);
        assert_eq!(scroll_slot(4, 120), 3);
        assert_eq!(scroll_slot(4, 0), 4);
    }
}
