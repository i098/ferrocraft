//! `FontRenderer.bidiReorder`: Arabic shaping (`ArabicShaping(LETTERS_SHAPE)`) and visual
//! reordering (`Bidi(text, DEFAULT_RTL)` + `writeReordered(DO_MIRRORING)`).

use unicode_bidi::{bidi_class, BidiClass, BidiInfo, Level};

/// `FontRenderer.bidiReorder`: the text unchanged when it has no right-to-left character.
pub fn reorder(text: &str) -> String {
    if !requires_bidi(text) {
        return text.to_owned();
    }
    let shaped: String = shape(&text.chars().collect::<Vec<_>>())
        .into_iter()
        .collect();
    visual(&shaped)
}

/// `Bidi.requiresBidi`.
fn requires_bidi(text: &str) -> bool {
    use BidiClass::{AL, AN, R, RLE, RLO};
    text.chars()
        .any(|c| matches!(bidi_class(c), R | AL | RLE | RLO | AN))
}

/// `new Bidi(text, DEFAULT_RTL)` then `writeReordered(DO_MIRRORING)`. Each paragraph gets its
/// own level; the whole text is then reordered as one line, as ICU does.
fn visual(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut levels = Vec::with_capacity(chars.len());
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if bidi_class(c) == BidiClass::B {
            let end = i + c.len_utf8();
            paragraph_levels(&text[start..end], &mut levels);
            start = end;
        }
    }
    paragraph_levels(&text[start..], &mut levels);
    BidiInfo::reorder_visual(&levels)
        .into_iter()
        .map(|i| {
            if levels[i].is_rtl() {
                mirror(chars[i])
            } else {
                chars[i]
            }
        })
        .collect()
}

/// The resolved level of each char of one paragraph; the paragraph level is that of the first
/// strong char, right-to-left when there is none.
fn paragraph_levels(para: &str, out: &mut Vec<Level>) {
    if para.is_empty() {
        return;
    }
    let first_strong = para.chars().find_map(|c| match bidi_class(c) {
        BidiClass::L => Some(Level::ltr()),
        BidiClass::R | BidiClass::AL => Some(Level::rtl()),
        _ => None,
    });
    let info = BidiInfo::new(para, Some(first_strong.unwrap_or_else(Level::rtl)));
    if let Some(p) = info.paragraphs.first() {
        out.extend(info.reordered_levels_per_char(p, p.range.clone()));
    }
}

fn mirror(c: char) -> char {
    unicode_bidi_mirroring::get_mirrored(c).unwrap_or(c)
}

// --- ArabicShaping, options LETTERS_SHAPE (logical order, lam-alef resized) ---

const IRRELEVANT: u32 = 4;
const LAMTYPE: u32 = 16;
const ALEFTYPE: u32 = 32;
const LINKR: u32 = 1;
const LINKL: u32 = 2;
const LINK_MASK: u32 = 3;
const LAMALEF_SPACE_SUB: char = '\u{ffff}';
const IRRELEVANT_POS: [u32; 8] = [0x0, 0x2, 0x4, 0x6, 0x8, 0xA, 0xC, 0xE];
const SHAPE_TABLE: [[[u32; 4]; 4]; 4] = [
    [[0, 0, 0, 0], [0, 0, 0, 0], [0, 1, 0, 3], [0, 1, 0, 1]],
    [[0, 0, 2, 2], [0, 0, 1, 2], [0, 1, 1, 2], [0, 1, 1, 3]],
    [[0, 0, 0, 0], [0, 0, 0, 0], [0, 1, 0, 3], [0, 1, 0, 3]],
    [[0, 0, 1, 2], [0, 0, 1, 2], [0, 1, 1, 2], [0, 1, 1, 3]],
];

/// `araLink` for U+0622..=U+06D3: link flags, base presentation form offset in bits 8+.
const ARA_LINK: [u32; 178] = [
    0x1121, 0x1321, 0x1501, 0x1721, 0x1903, 0x1d21, 0x1f03, 0x2301, 0x2503, 0x2903, 0x2d03, 0x3103,
    0x3503, 0x3901, 0x3b01, 0x3d01, 0x3f01, 0x4103, 0x4503, 0x4903, 0x4d03, 0x5103, 0x5503, 0x5903,
    0x5d03, 0, 0, 0, 0, 0, 3, 0x6103, 0x6503, 0x6903, 0x6d13, 0x7103, 0x7503, 0x7903, 0x7d01,
    0x7f01, 0x8103, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0x8501, 0x8701, 0x8901,
    0x8b01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 33, 33, 0, 33, 1, 1, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 3, 1, 3, 3, 3, 3, 1, 1,
];

/// `presLink` for U+FE70..=U+FEFC.
const PRES_LINK: [u32; 141] = [
    3, 3, 3, 0, 3, 0, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 32, 33, 32, 33, 0, 1, 32, 33, 0, 2, 3, 1,
    32, 33, 0, 2, 3, 1, 0, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 1, 0,
    1, 0, 1, 0, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3,
    1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 0, 2, 3, 1, 16, 18, 19, 17, 0, 2, 3, 1, 0, 2, 3, 1, 0,
    2, 3, 1, 0, 1, 0, 1, 0, 2, 3, 1, 0, 1, 0, 1, 0, 1, 0, 1,
];

/// `convertFEto06` for U+FE70..=U+FEFC.
const FE_TO_06: [u32; 141] = [
    0x64b, 0x64b, 0x64c, 0x64c, 0x64d, 0x64d, 0x64e, 0x64e, 0x64f, 0x64f, 0x650, 0x650, 0x651,
    0x651, 0x652, 0x652, 0x621, 0x622, 0x622, 0x623, 0x623, 0x624, 0x624, 0x625, 0x625, 0x626,
    0x626, 0x626, 0x626, 0x627, 0x627, 0x628, 0x628, 0x628, 0x628, 0x629, 0x629, 0x62a, 0x62a,
    0x62a, 0x62a, 0x62b, 0x62b, 0x62b, 0x62b, 0x62c, 0x62c, 0x62c, 0x62c, 0x62d, 0x62d, 0x62d,
    0x62d, 0x62e, 0x62e, 0x62e, 0x62e, 0x62f, 0x62f, 0x630, 0x630, 0x631, 0x631, 0x632, 0x632,
    0x633, 0x633, 0x633, 0x633, 0x634, 0x634, 0x634, 0x634, 0x635, 0x635, 0x635, 0x635, 0x636,
    0x636, 0x636, 0x636, 0x637, 0x637, 0x637, 0x637, 0x638, 0x638, 0x638, 0x638, 0x639, 0x639,
    0x639, 0x639, 0x63a, 0x63a, 0x63a, 0x63a, 0x641, 0x641, 0x641, 0x641, 0x642, 0x642, 0x642,
    0x642, 0x643, 0x643, 0x643, 0x643, 0x644, 0x644, 0x644, 0x644, 0x645, 0x645, 0x645, 0x645,
    0x646, 0x646, 0x646, 0x646, 0x647, 0x647, 0x647, 0x647, 0x648, 0x648, 0x649, 0x649, 0x64a,
    0x64a, 0x64a, 0x64a, 0x65c, 0x65c, 0x65d, 0x65d, 0x65e, 0x65e, 0x65f, 0x65f,
];

fn change_lam_alef(c: char) -> Option<char> {
    match c {
        '\u{622}' => Some('\u{65C}'),
        '\u{623}' => Some('\u{65D}'),
        '\u{625}' => Some('\u{65E}'),
        '\u{627}' => Some('\u{65F}'),
        _ => None,
    }
}

fn special_char(c: char) -> u32 {
    match c {
        '\u{622}'..='\u{625}'
        | '\u{627}'
        | '\u{62F}'..='\u{632}'
        | '\u{648}'..='\u{649}'
        | '\u{629}' => 1,
        '\u{64B}'..='\u{652}' => 2,
        '\u{653}'..='\u{655}' | '\u{670}' | '\u{FE70}'..='\u{FE7F}' => 3,
        _ => 0,
    }
}

fn link(c: char) -> u32 {
    match c {
        '\u{622}'..='\u{6D3}' => ARA_LINK[c as usize - 0x622],
        '\u{200D}' => 3,
        '\u{206D}'..='\u{206F}' => 4,
        '\u{FE70}'..='\u{FEFC}' => PRES_LINK[c as usize - 0xFE70],
        _ => 0,
    }
}

fn is_tashkeel(c: char) -> bool {
    ('\u{64B}'..='\u{652}').contains(&c)
}

/// Link state of the scan in `shape`: the current char and the last two relevant ones.
struct Scan {
    curr: u32,
    next: u32,
    last_link: u32,
    prev_link: u32,
    last_pos: isize,
    nx: isize,
}

/// The link of the next relevant char before `i`, and its index (`isize::MAX` at the start).
fn next_relevant(d: &[char], i: isize) -> (u32, isize) {
    let mut nw = i - 1;
    loop {
        if nw == -1 {
            return (0, isize::MAX);
        }
        let l = link(d[nw as usize]);
        if l & IRRELEVANT == 0 {
            return (l, nw);
        }
        nw -= 1;
    }
}

/// The presentation form of `d[i]` given its neighbours' links.
fn shaped_form(c: char, sc: &Scan) -> char {
    let flag = special_char(c);
    let mut shape = SHAPE_TABLE[(sc.next & LINK_MASK) as usize]
        [(sc.last_link & LINK_MASK) as usize][(sc.curr & LINK_MASK) as usize];
    if flag == 1 {
        shape &= 1;
    } else if flag == 2 {
        let joins = sc.last_link & LINKL != 0
            && sc.next & LINKR != 0
            && c != '\u{64C}'
            && c != '\u{64D}'
            && !(sc.next & ALEFTYPE == ALEFTYPE && sc.last_link & LAMTYPE == LAMTYPE);
        shape = u32::from(joins);
    }
    let base = if flag == 2 {
        0xFE70 + IRRELEVANT_POS[c as usize - 0x64B]
    } else {
        0xFE70 + (sc.curr >> 8)
    };
    char::from_u32(base + shape).unwrap_or(c)
}

/// Shapes `d[i]` (or the lam of a lam-alef pair); returns the index that was shaped and whether
/// a lam-alef pair was merged.
fn shape_at(d: &mut [char], i: isize, sc: &mut Scan) -> (isize, bool) {
    let (next, nx) = next_relevant(d, i);
    sc.next = next;
    sc.nx = nx;
    let mut at = i;
    let mut merged = false;
    if sc.curr & ALEFTYPE > 0 && sc.last_link & LAMTYPE > 0 {
        merged = true;
        let w = change_lam_alef(d[i as usize]);
        if let Some(w) = w {
            d[i as usize] = LAMALEF_SPACE_SUB;
            d[sc.last_pos as usize] = w;
            at = sc.last_pos;
        }
        sc.last_link = sc.prev_link;
        sc.curr = w.map_or(0, link);
    }
    d[at as usize] = shaped_form(d[at as usize], sc);
    (at, merged)
}

/// `ArabicShaping.shape`: Arabic letters to their presentation forms; a lam-alef pair becomes
/// one character.
fn shape(text: &[char]) -> Vec<char> {
    let mut d: Vec<char> = text.iter().rev().copied().collect();
    let Some(&first) = d.last() else {
        return Vec::new();
    };
    // `normalize`: presentation forms back to the base block.
    for c in &mut d {
        if ('\u{FE70}'..='\u{FEFC}').contains(c) {
            *c = char::from_u32(FE_TO_06[*c as usize - 0xFE70]).unwrap_or(*c);
        }
    }
    let first = if ('\u{FE70}'..='\u{FEFC}').contains(&first) {
        d[d.len() - 1]
    } else {
        first
    };
    let mut i = d.len() as isize - 1;
    let mut sc = Scan {
        curr: link(first),
        next: 0,
        last_link: 0,
        prev_link: 0,
        last_pos: i,
        nx: -2,
    };
    let mut lamalef_found = false;
    while i >= 0 {
        if sc.curr & 0xFF00 > 0 || is_tashkeel(d[i as usize]) {
            let (at, merged) = shape_at(&mut d, i, &mut sc);
            i = at;
            lamalef_found |= merged;
        }
        if sc.curr & IRRELEVANT == 0 {
            sc.prev_link = sc.last_link;
            sc.last_link = sc.curr;
            sc.last_pos = i;
        }
        i -= 1;
        if i == sc.nx {
            sc.curr = sc.next;
            sc.nx = -2;
        } else if i != -1 {
            sc.curr = link(d[i as usize]);
        }
    }
    if lamalef_found {
        d.retain(|&c| c != LAMALEF_SPACE_SUB);
    }
    d.reverse();
    d
}

#[cfg(test)]
mod tests {
    use super::reorder;

    const CORPUS: &[&str] = &[
        "English (US)",
        "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629} (\u{627}\u{644}\u{633}\u{639}\u{648}\u{62f}\u{64a}\u{629})",
        "\u{5e2}\u{5d1}\u{5e8}\u{5d9}\u{5ea} (\u{5d9}\u{5e9}\u{5e8}\u{5d0}\u{5dc})",
        "\u{644}\u{627} \u{644}\u{623} \u{644}\u{625} \u{644}\u{622} \u{6a9}\u{644}\u{627}",
        "\u{645}\u{64e}\u{631}\u{652}\u{62d}\u{64e}\u{628}\u{627}\u{64b} \u{628}\u{651}\u{650}",
        "abc \u{627}\u{644}\u{633}\u{644}\u{627}\u{645} 123 def",
        "\u{627}\u{644}\u{633}\u{644}\u{627}\u{645} (12) [x] <\u{627}> {\u{628}}",
        "\u{a7}c\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}\u{a7}r test",
        "\u{627}\u{628}\n\u{62c}\u{62f} abc\nxyz \u{5d0}\u{5d1}",
        "\u{6cc}\u{6a9}\u{6af}\u{6d2} \u{67e}\u{686}\u{698}",
        "\u{627}\u{644}\u{633}\u{639}\u{631} \u{661}\u{662}\u{663} 45.6",
        "",
    ];

    /// 400 strings drawn from Arabic and Hebrew letters, marks, digits, brackets and Latin.
    fn random_corpus() -> Vec<String> {
        let alphabet: Vec<char> = ('\u{621}'..='\u{64a}')
            .chain('\u{64b}'..='\u{652}')
            .chain([
                '\u{67e}', '\u{6a9}', '\u{6cc}', '\u{5d0}', '\u{5d1}', '\u{5ea}',
            ])
            .chain("  aZ09()[]<>.-\u{660}\u{661}\u{a7}".chars())
            .collect();
        let mut x = 0x2545_f491_u32;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as usize
        };
        (0..400)
            .map(|_| {
                (0..1 + next() % 12)
                    .map(|_| alphabet[next() % alphabet.len()])
                    .collect()
            })
            .collect()
    }

    /// The shaped, reordered text against the real client's ICU `ArabicShaping` and `Bidi`.
    #[test]
    fn matches_the_real_client() {
        let corpus: Vec<String> = CORPUS
            .iter()
            .map(|s| (*s).to_owned())
            .chain(random_corpus())
            .collect();
        let script: String = corpus
            .iter()
            .map(|s| {
                let hex: String = s.bytes().map(|b| format!("{b:02x}")).collect();
                format!("bidi {hex}\n")
            })
            .collect();
        let lines = oracle::Oracle::get().run(&script).unwrap();
        assert_eq!(lines.len(), corpus.len());
        for (s, line) in corpus.iter().zip(lines) {
            let want: Vec<u8> = (0..line.len() / 2)
                .map(|i| u8::from_str_radix(&line[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            assert_eq!(reorder(s).as_bytes(), &want[..], "input {s:?}");
        }
    }
}
