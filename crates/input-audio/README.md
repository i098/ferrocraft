# input-audio

Owner: fc-p-input

Keyboard, mouse and chat input per vanilla 1.8.9.

- `InputState` turns winit events into held state (`movement()`, `attack_held()`, `use_held()`,
  `player_list_held()`), mouse look (`take_look_delta()`) and discrete `Action`s
  (`take_actions()`): hotbar 1-9, wheel, E, Q / Ctrl+Q, Esc, F1, F2, F3 and F3 combos, F4, F5, F11,
  left/middle/right click.
- Sprint is Ctrl held or a double tap of forward (350 ms), cancelled by sneak or releasing forward.
- `KeyBindings` holds the vanilla defaults and reads `key_key.*` lines of `options.txt`
  (LWJGL codes).
- `apply_cursor_grab` locks the cursor (Locked, then Confined). The engine must call
  `window.focus_window()` at start: a child process on macOS begins unfocused and then gets no
  key events. Focus loss drops the grab, releases all keys and queues `Action::Pause`.
- A GUI screen calls `set_screen_open(true)`: movement stops, the cursor is free, and Esc
  queues `Action::CloseScreen`.
