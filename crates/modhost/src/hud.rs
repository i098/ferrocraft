//! HUD handover. Rust draws the vanilla HUD, but a mod can take an element over: it cancels the `RenderGameOverlayEvent.Pre` of
//! the element, or it changes the vanilla method that draws it. `OverlayFilter.java` decides this per element and per frame,
//! and sends the elements it kept as the command `'H'` of `wire.rs`: a u32 with bit `1 << element as u32` set for each
//! element that Java owns. Rust then does not draw those elements; what the mod draws arrives as the other commands.
//! The crosshair is not here: Rust always draws it until the mod picture path (issue #282) can draw the vanilla texture.

/// A vanilla HUD element that Java can own. The discriminant is the bit number on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    /// The pumpkin overlay (`ElementType.HELMET`).
    Helmet = 0,
    /// The hotbar (`ElementType.HOTBAR`).
    Hotbar = 1,
    /// The F3 screen (`ElementType.DEBUG`).
    Debug = 2,
    /// The tab list (`ElementType.PLAYER_LIST`).
    PlayerList = 3,
}

/// The elements that Java owns in one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Owned(u32);

impl Owned {
    /// This set and the elements of a wire bit set; a bit that is no element is kept out.
    pub fn with(self, bits: u32) -> Self {
        Self(self.0 | bits & ((1 << (Element::PlayerList as u32 + 1)) - 1))
    }

    pub fn has(self, e: Element) -> bool {
        self.0 >> e as u32 & 1 != 0
    }

    /// Switch off in `f` every element that Java owns; the mod draws it.
    pub fn leave_out(self, f: &mut gui::Frame) {
        if self.has(Element::Helmet) {
            f.helmet_pumpkin = false;
        }
        if self.has(Element::Hotbar) {
            f.player_view = false;
        }
        if self.has(Element::Debug) {
            // An empty screen, not `None`: the crosshair still follows F3.
            f.debug = Some((&[], &[]));
        }
        if self.has(Element::PlayerList) {
            f.player_list = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_elements_are_left_out_and_the_rest_stay() {
        let mut f = gui::Frame {
            helmet_pumpkin: true,
            player_view: true,
            player_list: true,
            ..gui::Frame::default()
        };
        Owned::default()
            .with(
                1 << Element::Hotbar as u32
                    | 1 << Element::Debug as u32
                    | 1 << Element::PlayerList as u32,
            )
            .leave_out(&mut f);
        assert!(!f.player_view && !f.player_list);
        assert!(f.helmet_pumpkin);
        assert_eq!(f.debug.map(|(l, r)| l.len() + r.len()), Some(0));
    }

    #[test]
    fn sets_add_up_and_unknown_bits_are_dropped() {
        let o = Owned::default().with(0b1).with(0b10 | 1 << 31 | 0b10000);
        assert!(o.has(Element::Helmet) && o.has(Element::Hotbar));
        assert_eq!(o, Owned(0b11));
    }
}
