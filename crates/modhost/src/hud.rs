//! HUD handover. Rust draws the vanilla HUD, but a mod can take an element over: it cancels the `RenderGameOverlayEvent.Pre` of
//! the element, or it changes the vanilla method that draws it. `OverlayFilter.java` decides this per element and per frame,
//! and sends the elements it kept as the command `'H'` of `wire.rs`: a u32 with bit `1 << element as u32` set for each
//! element that Java owns. Rust then does not draw those elements; what the mod draws arrives as the other commands.

/// A vanilla HUD element that Java can own. The discriminant is the bit number on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    /// The pumpkin overlay (`ElementType.HELMET`).
    Helmet = 0,
    /// The crosshair (`ElementType.CROSSHAIRS`; the changed method is `GuiIngame.showCrosshair`).
    Crosshairs = 1,
    /// The hotbar (`ElementType.HOTBAR`).
    Hotbar = 2,
}

/// The elements that Java owns in one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Owned(u32);

impl Owned {
    /// This set and the elements of a wire bit set; a bit that is no element is kept out.
    pub fn with(self, bits: u32) -> Self {
        Self(self.0 | bits & ((1 << (Element::Hotbar as u32 + 1)) - 1))
    }

    pub fn has(self, e: Element) -> bool {
        self.0 >> e as u32 & 1 != 0
    }

    /// Switch off in `f` every element that Java owns; the mod draws it.
    pub fn leave_out(self, f: &mut gui::Frame) {
        if self.has(Element::Helmet) {
            f.helmet_pumpkin = false;
        }
        if self.has(Element::Crosshairs) {
            f.hide_crosshair = true;
        }
        if self.has(Element::Hotbar) {
            f.player_view = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wire_bit_owns_its_element_only() {
        let o = Owned::default().with(1 << Element::Crosshairs as u32);
        assert!(o.has(Element::Crosshairs));
        assert!(!o.has(Element::Hotbar) && !o.has(Element::Helmet));
    }

    #[test]
    fn owned_elements_are_left_out_and_the_rest_stay() {
        let mut f = gui::Frame {
            helmet_pumpkin: true,
            player_view: true,
            ..gui::Frame::default()
        };
        Owned::default()
            .with(1 << Element::Crosshairs as u32)
            .leave_out(&mut f);
        assert!(f.hide_crosshair);
        assert!(f.helmet_pumpkin && f.player_view);
    }

    #[test]
    fn sets_add_up_and_unknown_bits_are_dropped() {
        let o = Owned::default().with(0b1).with(0b100 | 1 << 31 | 0b1000);
        assert!(o.has(Element::Helmet) && o.has(Element::Hotbar));
        assert_eq!(o, Owned(0b101));
    }
}
