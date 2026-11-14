//! `isChild` branch of `ModelBiped`, `ModelQuadruped` and `ModelChicken` `render`: the head and
//! the rest of the body are drawn with their own scale and translation.

use super::{Class, EntityDraw, Part};

/// `scalef(scale)` after `translatef(shift)`, in model pixels, as [`Part::placed`] takes it.
type Placed = (f32, [f32; 3]);

/// How a child model splits: the first `heads` parts form the head group.
struct Split {
    heads: usize,
    head: Placed,
    rest: Placed,
}

/// `ModelBiped`: head scale 1.5 / 2 and lift 16, the rest scale 1 / 2 and lift 24.
const BIPED: Split = Split {
    heads: 1,
    head: (0.75, [0.0, 16.0, 0.0]),
    rest: (0.5, [0.0, 24.0, 0.0]),
};

/// `ModelQuadruped` with `childYOffset` and `childZOffset`: the head only moves, the rest halves.
const fn quadruped(heads: usize, y_offset: f32, z_offset: f32) -> Split {
    Split {
        heads,
        head: (1.0, [0.0, y_offset, z_offset]),
        rest: (0.5, [0.0, 24.0, 0.0]),
    }
}

/// `ModelChicken`: head, bill and chin move by (0, 5, 2); the rest halves.
const CHICKEN: Split = Split {
    heads: 3,
    head: (1.0, [0.0, 5.0, 2.0]),
    rest: (0.5, [0.0, 24.0, 0.0]),
};

fn split(d: &EntityDraw) -> Option<Split> {
    Some(match d.class {
        Class::Mob(54 | 57) => BIPED,
        // `ModelQuadruped` has (8, 4); `ModelPig` sets `childYOffset` to 4 and `ModelCow` adds 2 to `childZOffset`.
        Class::Mob(90) => quadruped(2, 4.0, 4.0),
        Class::Mob(92) => quadruped(3, 8.0, 6.0),
        Class::Mob(91) => quadruped(1, 8.0, 4.0),
        Class::Mob(93) => CHICKEN,
        _ => return None,
    })
}

/// Places the parts of a baby mob; adults and classes without a child branch stay as they are.
pub(super) fn apply(parts: &mut [Part], d: &EntityDraw) {
    let Some(s) = split(d).filter(|_| d.child) else {
        return;
    };
    for (i, part) in parts.iter_mut().enumerate() {
        let (scale, shift) = if i < s.heads { s.head } else { s.rest };
        part.scale = scale;
        part.shift = shift;
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::steve;
    use super::super::{model, Class, EntityDraw};

    fn draw(id: u8, child: bool) -> EntityDraw {
        let mut d = steve();
        d.class = Class::Mob(id);
        d.child = child;
        d
    }

    /// Model pixels of the head pivot after the child placement.
    fn head_y(id: u8, child: bool) -> f32 {
        let p = &model(&draw(id, child)).parts[0];
        (p.pivot[1] + p.shift[1]) * p.scale
    }

    #[test]
    fn baby_head_sits_at_the_class_offset() {
        // Zombie head pivot y 0 -> (0 + 16) * 0.75; pig 12 -> 12 + 4; cow 4 -> 4 + 8; chicken 15 -> 15 + 5.
        assert_eq!(head_y(54, false), 0.0);
        assert_eq!(head_y(54, true), 12.0);
        assert_eq!(head_y(90, false), 12.0);
        assert_eq!(head_y(90, true), 16.0);
        assert_eq!(head_y(92, true), 12.0);
        assert_eq!(head_y(93, true), 20.0);
    }

    #[test]
    fn baby_body_is_half_size() {
        for id in [54, 57, 90, 91, 92, 93] {
            let m = model(&draw(id, true));
            let last = m.parts.last().unwrap();
            assert_eq!((last.scale, last.shift[1]), (0.5, 24.0), "mob {id}");
        }
    }
}
