//! `TextureUtil.generateMipmaps` / `blendPixels` / `blendPixelComponents` of 1.8.9: the mip levels of one
//! sprite, each the average of 2x2 texels of the level above, blended in gamma 2.2 space.

/// Levels 1..=`levels` of the `side`-px RGBA sprite `base`. The count stops where the side would drop below 1.
///
/// `generateMipmaps` looks for a transparent texel in only the first `levels + 1` texels of the sprite (it
/// loops to the length of the level array, not of the pixels), so `levels` also decides the blend rule.
pub(crate) fn chain(base: &[u8], side: usize, levels: u32) -> Vec<Vec<u8>> {
    // A transparent texel there makes the sprite blend like a cutout: color from opaque neighbours only, and
    // weak alpha dropped.
    let cutout = base
        .as_chunks::<4>()
        .0
        .iter()
        .take(levels as usize + 1)
        .any(|p| p[3] == 0);
    let table = gamma_table();
    let mut out: Vec<Vec<u8>> = Vec::new();
    for level in 1..=levels.min(side.ilog2()) {
        let src: &[u8] = out.last().map_or(base, |v| v);
        out.push(halve(src, side >> (level - 1), cutout, &table));
    }
    out
}

/// One level from the one above it: each texel blends a 2x2 block (`from` px wide) of the source.
fn halve(src: &[u8], from: usize, cutout: bool, table: &[f32; 256]) -> Vec<u8> {
    let to = from / 2;
    let mut next = vec![0u8; to * to * 4];
    for y in 0..to {
        for x in 0..to {
            let at = |dx: usize, dy: usize| &src[((2 * y + dy) * from + 2 * x + dx) * 4..][..4];
            let px = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
            next[(y * to + x) * 4..][..4].copy_from_slice(&blend(px, cutout, table));
        }
    }
    next
}

/// `Math.pow(v / 255.0F, 2.2)` as a float, for every byte.
fn gamma_table() -> [f32; 256] {
    std::array::from_fn(|v| f64::from(v as f32 / 255.0).powf(2.2) as f32)
}

/// Back out of gamma space: `(Math.pow(x, 0.45454545454545453) * 255.0)` cast to int.
fn encode(x: f64) -> i32 {
    (x.powf(0.454_545_454_545_454_53) * 255.0) as i32
}

/// `blendPixels` for the four texels of a 2x2 block.
fn blend(px: [&[u8]; 4], cutout: bool, table: &[f32; 256]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (c, o) in out.iter_mut().enumerate() {
        let mut sum = 0f32;
        for p in px {
            // Cutout sprites skip fully transparent texels, whichever channel is read.
            if !cutout || p[3] != 0 {
                sum += table[p[c] as usize];
            }
        }
        *o = if cutout {
            encode(f64::from(sum / 4.0))
        } else {
            encode(f64::from(sum) * 0.25)
        } as u8;
    }
    if cutout && out[3] < 96 {
        out[3] = 0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How a test sprite is made transparent.
    #[derive(Clone, Copy)]
    enum Holes {
        None,
        Anywhere,
        /// Opaque first five texels, holes after them: the cutout rule only reads those first texels.
        PastTheStart,
    }

    /// Deterministic RGBA noise with some fully transparent and some faint texels.
    fn sprite(side: usize, seed: u32, holes: Holes) -> Vec<u8> {
        let mut state = seed;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        };
        (0..side * side)
            .flat_map(|i| {
                let (r, g, b, a) = (next(), next(), next(), next());
                let alpha = match (holes, a % 4) {
                    (Holes::None, _) => 255,
                    (Holes::PastTheStart, _) if i < 5 => 255,
                    (_, 0) => 0,
                    (_, 1) => a % 120,
                    _ => 255,
                };
                [r, g, b, alpha]
            })
            .collect()
    }

    /// The real `TextureUtil.generateMipmaps` agrees on every texel of every level (needs the oracle jar).
    #[test]
    #[ignore = "needs a JDK and downloads the 1.8.9 client jar"]
    fn matches_vanilla_generate_mipmaps() {
        let mut cases = Vec::new();
        for (n, holes) in [Holes::None, Holes::Anywhere, Holes::PastTheStart]
            .into_iter()
            .enumerate()
        {
            for levels in 1..=4 {
                cases.push((16 << (levels % 3), n as u32 * 10 + levels, holes, levels));
            }
        }
        let script: String = cases
            .iter()
            .map(|&(side, seed, holes, levels)| {
                let argb: String = sprite(side, seed, holes)
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| format!("{:02x}{:02x}{:02x}{:02x}", p[3], p[0], p[1], p[2]))
                    .collect();
                format!("mip {levels} {side} {argb}\n")
            })
            .collect();
        let lines = oracle::Oracle::get().run(&script).unwrap();
        let mut lines = lines.iter();
        for &(side, seed, holes, levels) in &cases {
            for (level, got) in chain(&sprite(side, seed, holes), side, levels)
                .iter()
                .enumerate()
            {
                let line = lines.next().unwrap();
                let want: Vec<u8> = (0..line.len() / 8)
                    .flat_map(|i| {
                        let v = u32::from_str_radix(&line[8 * i..8 * i + 8], 16).unwrap();
                        [(v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8]
                    })
                    .collect();
                assert_eq!(
                    got,
                    &want,
                    "side {side} levels {levels} level {}",
                    level + 1
                );
            }
        }
        assert!(lines.next().is_none());
    }

    #[test]
    fn levels_stop_at_one_texel() {
        let sizes: Vec<usize> = chain(&sprite(4, 1, Holes::None), 4, 4)
            .iter()
            .map(|l| l.len() / 4)
            .collect();
        assert_eq!(sizes, [4, 1]);
    }
}
