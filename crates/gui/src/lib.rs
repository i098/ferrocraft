//! Vanilla 1.8.9 GUI: the drawing toolkit every screen uses and the in-game HUD.
//! Everything draws into a [`toolkit::Gui`], whose quads the renderer draws over the world.

pub mod chat;
pub mod debug;
pub mod hud;
pub mod toolkit;

pub use hud::{Frame, Hud};
pub use toolkit::{Button, Gui, GuiScale, Metrics};
