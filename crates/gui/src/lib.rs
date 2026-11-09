//! Vanilla 1.8.9 GUI: the drawing toolkit every screen uses and the in-game HUD.
//! Everything draws into a [`toolkit::Gui`], whose quads the renderer draws over the world.

pub mod bossbar;
pub mod canvas;
pub mod chat;
pub mod debug;
pub mod hud;
pub mod inventory;
pub mod scoreboard;
pub mod screens;
pub mod status;
pub mod tablist;
pub mod title;
pub mod toolkit;

pub use canvas::GuiCanvas;
pub use hud::{Frame, HotbarItems, Hud};
pub use status::Status;
pub use toolkit::{Button, Gui, GuiScale, Metrics};
