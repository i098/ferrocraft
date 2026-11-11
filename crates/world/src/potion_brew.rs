//! Potion colour and instant flag of an item damage value. Ported from 1.8.9 `PotionHelper`
//! (`getStatusEffects`, `parseRecipe`, `getColor`); durations are not needed and are left out.

/// Colour of a potion with no effect (water bottle, mundane).
const NO_EFFECT_COLOR: i32 = 3_694_022;

/// `(effect id, brewing recipe, amplifier recipe)` in effect-id order, from the `PotionHelper` static block.
const RECIPES: [(u8, &str, Option<&str>); 13] = [
    (1, "!0 & 1 & !2 & !3 & 1+6", Some("5")),
    (2, "!0 & 1 & !2 & 3 & 3+6", None),
    (5, "0 & !1 & !2 & 3 & 3+6", Some("5")),
    (6, "0 & !1 & 2 & !3", Some("5")),
    (7, "!0 & !1 & 2 & 3", Some("5")),
    (8, "0 & 1 & !2 & 3 & 3+6", Some("5")),
    (10, "0 & !1 & !2 & !3 & 0+6", Some("5")),
    (12, "0 & 1 & !2 & !3 & 0+6", None),
    (13, "0 & !1 & 2 & 3 & 2+6", None),
    (14, "!0 & 1 & 2 & 3 & 2+6", None),
    (16, "!0 & 1 & 2 & !3 & 2+6", None),
    (18, "!0 & !1 & !2 & 3 & 3+6", None),
    (19, "!0 & !1 & 2 & !3 & 2+6", Some("5")),
];

/// `Potion.getLiquidColor` of the effects above.
fn effect_color(id: u8) -> i32 {
    match id {
        1 => 8_171_462,
        2 => 5_926_017,
        5 => 9_643_043,
        6 => 16_262_179,
        7 => 4_393_481,
        8 => 2_293_580,
        10 => 13_458_603,
        12 => 14_981_690,
        13 => 3_035_801,
        14 => 8_356_754,
        16 => 0x1F_1FA1,
        18 => 0x48_4D48,
        _ => 5_149_489,
    }
}

fn flag(meta: i32, bit: i32) -> i32 {
    i32::from(meta & (1 << bit) != 0)
}

/// `PotionHelper.evaluateMetadata`.
fn evaluate(
    invert: bool,
    multiply: bool,
    negate: bool,
    op: i32,
    operand: i32,
    mult: i32,
    meta: i32,
) -> i32 {
    let set = meta.count_ones() as i32;
    let mut n = if invert {
        1 - flag(meta, operand)
    } else if op == -1 {
        flag(meta, operand)
    } else {
        i32::from(match op {
            0 => set == operand,
            1 => set > operand,
            _ => set < operand,
        })
    };
    if multiply {
        n *= mult;
    }
    if negate {
        n = -n;
    }
    n
}

/// `PotionHelper.parseRecipe`: `|` takes the first positive side, `&` the larger of two positive sides.
fn parse(r: &[u8], from: i32, to: i32, meta: i32) -> i32 {
    if from >= r.len() as i32 || to < 0 || from >= to {
        return 0;
    }
    let find = |c: u8| {
        r[from as usize..]
            .iter()
            .position(|&b| b == c)
            .map(|i| i as i32 + from)
    };
    if let Some(i) = find(b'|').filter(|&i| i < to) {
        let a = parse(r, from, i - 1, meta);
        return if a > 0 {
            a
        } else {
            parse(r, i + 1, to, meta).max(0)
        };
    }
    if let Some(i) = find(b'&').filter(|&i| i < to) {
        let (a, b) = (parse(r, from, i - 1, meta), parse(r, i + 1, to, meta));
        return if a <= 0 || b <= 0 { 0 } else { a.max(b) };
    }
    parse_terms(&r[from as usize..to as usize], meta)
}

/// A run of `!`, `-`, `=`, `<`, `>`, `+` terms with digits and `*` multipliers, summed.
fn parse_terms(r: &[u8], meta: i32) -> i32 {
    let (mut invert, mut star, mut multiply, mut negate) = (false, false, false, false);
    let (mut op, mut operand, mut mult, mut has_operand) = (-1, 0, 0, false);
    let mut total = 0;
    for &c in r {
        if c.is_ascii_digit() {
            let d = i32::from(c - b'0');
            if star {
                mult = d;
                multiply = true;
            } else {
                operand = operand * 10 + d;
                has_operand = true;
            }
            continue;
        }
        if c == b'*' {
            star = true;
            continue;
        }
        if !matches!(c, b'!' | b'-' | b'=' | b'<' | b'>' | b'+') {
            continue;
        }
        if has_operand {
            total += evaluate(invert, multiply, negate, op, operand, mult, meta);
            (invert, negate, star, multiply, has_operand) = (false, false, false, false, false);
            (mult, operand, op) = (0, 0, -1);
        }
        match c {
            b'!' => invert = true,
            b'-' => negate = true,
            b'=' => op = 0,
            b'<' => op = 2,
            b'>' => op = 1,
            _ => {}
        }
    }
    if has_operand {
        total += evaluate(invert, multiply, negate, op, operand, mult, meta);
    }
    total
}

/// `(rgb, instant)` of the potion with item damage `meta`: `Item.getPotionColor` and
/// `hasInstantPotionEffect`.
pub fn color_and_instant(meta: i32) -> (i32, bool) {
    let (mut sum, mut count, mut instant) = ([0.0_f32; 3], 0.0_f32, false);
    for (id, recipe, amp) in RECIPES {
        if parse(recipe.as_bytes(), 0, recipe.len() as i32, meta) <= 0 {
            continue;
        }
        let amplifier = amp.map_or(0, |a| parse(a.as_bytes(), 0, a.len() as i32, meta).max(0));
        instant |= matches!(id, 6 | 7);
        let c = effect_color(id);
        for _ in 0..=amplifier {
            for (s, shift) in sum.iter_mut().zip([16, 8, 0]) {
                *s += (c >> shift & 0xFF) as f32 / 255.0;
            }
            count += 1.0;
        }
    }
    if count == 0.0 {
        return (NO_EFFECT_COLOR, false);
    }
    let [r, g, b] = sum.map(|s| (s / count * 255.0) as i32);
    (r << 16 | g << 8 | b, instant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_instant_flags_follow_the_brewing_bits() {
        // Water bottle, splash healing II, poison, speed II.
        assert_eq!(color_and_instant(0), (3_694_022, false));
        assert_eq!(color_and_instant(16_453), (16_262_179, true));
        assert_eq!(color_and_instant(8_196), (5_149_489, false));
        assert_eq!(color_and_instant(16_418), (8_171_462, false));
        // Instant damage mixes no other effect.
        assert_eq!(color_and_instant(16_460), (4_393_481, true));
    }
}
