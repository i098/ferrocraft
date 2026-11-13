//! Asset loading from the 1.8.9 client jar (or an extracted copy): block atlas and the ascii font.

use crate::blocks::texture_names;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const TILE: usize = 16;
const COLUMNS: usize = 16;
/// `wgpu::Limits::default().max_texture_dimension_2d`, the limit of the device `request_device` asks for.
const MAX_ATLAS_SIDE: usize = 8192;

pub enum AssetSource {
    Jar(zip::ZipArchive<std::fs::File>),
    Dir(PathBuf),
    /// `FallbackResourceManager`: the selected resource packs, top first, over the client jar.
    Layers(Vec<AssetSource>),
}

impl AssetSource {
    pub fn open(path: &Path) -> Option<AssetSource> {
        if path.is_dir() {
            return Some(AssetSource::Dir(path.to_path_buf()));
        }
        let file = std::fs::File::open(path).ok()?;
        zip::ZipArchive::new(file).ok().map(AssetSource::Jar)
    }

    /// `packs` (top first, each a zip or a folder) over `base`; a pack that cannot be opened is
    /// skipped, as vanilla skips it.
    pub fn layered(packs: &[PathBuf], base: &Path) -> Option<AssetSource> {
        let base = Self::open(base)?;
        if packs.is_empty() {
            return Some(base);
        }
        let mut layers: Vec<AssetSource> = packs.iter().filter_map(|p| Self::open(p)).collect();
        layers.push(base);
        Some(AssetSource::Layers(layers))
    }

    /// `name` is a jar path such as `assets/minecraft/textures/font/ascii.png`.
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        match self {
            AssetSource::Jar(zip) => {
                let mut bytes = Vec::new();
                zip.by_name(name).ok()?.read_to_end(&mut bytes).ok()?;
                Some(bytes)
            }
            AssetSource::Dir(dir) => {
                let stripped = name.strip_prefix("assets/").unwrap_or(name);
                std::fs::read(dir.join(name))
                    .or_else(|_| std::fs::read(dir.join(stripped)))
                    .ok()
            }
            AssetSource::Layers(layers) => layers.iter_mut().find_map(|l| l.read(name)),
        }
    }

    /// `getAllResources`: every layer's copy of `name`, bottom first.
    pub fn read_all(&mut self, name: &str) -> Vec<Vec<u8>> {
        match self {
            AssetSource::Layers(layers) => layers
                .iter_mut()
                .rev()
                .flat_map(|l| l.read_all(name))
                .collect(),
            single => single.read(name).into_iter().collect(),
        }
    }
}

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

pub fn decode_png(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Image {
        width: info.width as usize,
        height: info.height as usize,
        rgba,
    })
}

pub fn encode_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(rgba).map_err(std::io::Error::other)
}

/// Block texture atlas: square tiles in a grid, `tile` px wide (the largest block texture, rounded up to a power of two, at least 16); tile 0 is solid white (flat colors use it).
pub struct Atlas {
    pub textured: bool,
    pub width: u32,
    pub height: u32,
    pub tile: u32,
    pub rgba: Vec<u8>,
    cells: HashMap<&'static str, u32>,
}

impl Atlas {
    /// Only the white tile: every block draws with its flat color.
    pub fn flat() -> Atlas {
        Atlas {
            textured: false,
            width: TILE as u32,
            height: TILE as u32,
            tile: TILE as u32,
            rgba: vec![255; TILE * TILE * 4],
            cells: HashMap::new(),
        }
    }

    pub fn from_assets(source: &mut AssetSource) -> Atlas {
        let names = texture_names();
        let mut images: Vec<Image> = Vec::with_capacity(names.len());
        for name in &names {
            let path = format!("assets/minecraft/textures/blocks/{name}.png");
            let mut image = source.read(&path).and_then(|b| decode_png(&b));
            if *name == "grass_side" {
                // Vanilla draws the tinted overlay over the side; the tint is constant here, so bake it.
                let overlay = source
                    .read("assets/minecraft/textures/blocks/grass_side_overlay.png")
                    .and_then(|b| decode_png(&b));
                if let (Some(base), Some(over)) = (image.as_mut(), overlay) {
                    bake_overlay(base, &over, crate::blocks::rgb(crate::blocks::GRASS_COLOR));
                }
            }
            images.push(image.unwrap_or_else(|| Image {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            }));
        }
        let loaded = images.iter().filter(|i| i.width > 0).count();
        let rows = (names.len() + 1).div_ceil(COLUMNS);
        let tile = atlas_tile(images.iter().map(|i| i.width.min(i.height)), rows);
        let (w, h) = (COLUMNS * tile, rows * tile);
        let mut rgba = vec![0u8; w * h * 4];
        let mut cells = HashMap::new();
        let white = Image {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        blit(&mut rgba, w, tile, 0, &white);
        for (i, (name, image)) in names.iter().zip(&images).enumerate() {
            let cell = i + 1;
            if image.width == 0 {
                blit(&mut rgba, w, tile, cell, &missing_tile());
            } else {
                blit(&mut rgba, w, tile, cell, image);
            }
            cells.insert(*name, cell as u32);
        }
        if loaded == 0 {
            eprintln!("ferrocraft-render: no block textures found; blocks use the missing tile");
        }
        Atlas {
            textured: true,
            width: w as u32,
            height: h as u32,
            tile: tile as u32,
            rgba,
            cells,
        }
    }
    /// Atlas coordinates for texture `name` at tile-local `(u, v)` in `0..=1`.
    pub fn uv(&self, name: &str, [u, v]: [f32; 2]) -> [f32; 2] {
        let cell = self.cells.get(name).copied().unwrap_or(0) as usize;
        if !self.textured || cell == 0 {
            return [0.5 / self.width as f32, 0.5 / self.height as f32];
        }
        let (cx, cy) = ((cell % COLUMNS) as f32, (cell / COLUMNS) as f32);
        [
            (cx + u) * self.tile as f32 / self.width as f32,
            (cy + v) * self.tile as f32 / self.height as f32,
        ]
    }
}

pub(crate) fn missing_tile() -> Image {
    let rgba = (0..TILE * TILE)
        .flat_map(|i| {
            if (i % TILE / 8 + i / TILE / 8).is_multiple_of(2) {
                [248, 0, 248, 255]
            } else {
                [0, 0, 0, 255]
            }
        })
        .collect();
    Image {
        width: TILE,
        height: TILE,
        rgba,
    }
}

/// Tile edge for the block atlas: the largest sprite side, rounded up to a power of two, at least 16,
/// and small enough that the `COLUMNS` x `rows` grid fits the device texture limit (larger packs downsample).
fn atlas_tile(sides: impl Iterator<Item = usize>, rows: usize) -> usize {
    let fit = (MAX_ATLAS_SIDE / rows.max(COLUMNS)).max(TILE);
    let tile = sides.max().unwrap_or(0).max(TILE).next_power_of_two();
    tile.min(1 << fit.ilog2())
}

/// Copy the top-left square of `image` into grid `cell`, nearest-sampled to `tile` px (animated strips keep frame 0).
fn blit(atlas: &mut [u8], atlas_width: usize, tile: usize, cell: usize, image: &Image) {
    let side = image.width.min(image.height).max(1);
    let (ox, oy) = ((cell % COLUMNS) * tile, (cell / COLUMNS) * tile);
    for y in 0..tile {
        for x in 0..tile {
            let (sx, sy) = (x * side / tile, y * side / tile);
            let src = (sy * image.width + sx) * 4;
            let dst = ((oy + y) * atlas_width + ox + x) * 4;
            atlas[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
}

/// Alpha-composite `over` (multiplied by `tint`) onto `base`; ignored when the sizes differ.
fn bake_overlay(base: &mut Image, over: &Image, tint: [f32; 3]) {
    if (base.width, base.height) != (over.width, over.height) {
        return;
    }
    for (b, o) in base
        .rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(over.rgba.as_chunks::<4>().0)
    {
        let a = o[3] as f32 / 255.0;
        for c in 0..3 {
            let top = o[c] as f32 * tint[c];
            b[c] = (top * a + b[c] as f32 * (1.0 - a)) as u8;
        }
    }
}

/// Vanilla `FontRenderer` metrics: `charWidth` of the 256 `ascii.png` glyphs and the
/// `glyph_sizes.bin` table of the unicode pages (high nibble first column, low nibble last).
pub struct Font {
    pub ascii: [u8; 256],
    pub unicode: Vec<u8>,
    pub force_unicode: bool,
}

impl Font {
    pub fn load(source: &mut AssetSource) -> Option<Font> {
        let image = decode_png(&source.read("assets/minecraft/textures/font/ascii.png")?)?;
        if image.width % 16 != 0 || image.height % 16 != 0 {
            return None;
        }
        let (cell_w, cell_h) = (image.width / 16, image.height / 16);
        let f = 8.0 / cell_w as f64;
        let mut ascii = [0u8; 256];
        for (c, w) in ascii.iter_mut().enumerate() {
            if c == 32 {
                *w = 4;
                continue;
            }
            let (gx, gy) = ((c % 16) * cell_w, (c / 16) * cell_h);
            let used = (0..cell_w).rev().find(|&x| {
                (0..cell_h).any(|y| image.rgba[((gy + y) * image.width + gx + x) * 4 + 3] != 0)
            });
            let columns = used.map_or(0, |x| x + 1);
            *w = ((0.5 + columns as f64 * f) as u8) + 1;
        }
        let mut unicode = source
            .read("assets/minecraft/font/glyph_sizes.bin")
            .unwrap_or_default();
        unicode.resize(65536, 0);
        Some(Font {
            ascii,
            unicode,
            force_unicode: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(side: usize, px: [u8; 4]) -> Image {
        Image {
            width: side,
            height: side,
            rgba: px.repeat(side * side),
        }
    }

    #[test]
    fn tile_follows_largest_sprite_and_uvs_stay_in_cell() {
        assert_eq!(atlas_tile([16, 16].into_iter(), 20), 16);
        assert_eq!(atlas_tile([16, 32, 0].into_iter(), 20), 32);
        assert_eq!(atlas_tile([48].into_iter(), 20), 64);
        // 317 cells = 20 rows: 256x gives 4096x5120, 512x would give 10240 high, over 8192
        assert_eq!(atlas_tile([256].into_iter(), 20), 256);
        assert_eq!(atlas_tile([512, 1024].into_iter(), 20), 256);
        let tile = 32;
        let mut rgba = vec![0u8; 16 * tile * 2 * tile * 4];
        blit(&mut rgba, 16 * tile, tile, 1, &solid(16, [1, 2, 3, 255]));
        blit(&mut rgba, 16 * tile, tile, 2, &solid(32, [9, 9, 9, 255]));
        let px = |cell: usize, x: usize, y: usize| {
            let i = (y * 16 * tile + cell * tile + x) * 4;
            rgba[i]
        };
        // a 16x sprite scales by 2 and fills its whole 32 px cell; a 32x sprite copies 1:1
        assert_eq!((px(1, 0, 0), px(1, 31, 31)), (1, 1));
        assert_eq!((px(2, 0, 0), px(2, 31, 31)), (9, 9));
        let atlas = Atlas {
            textured: true,
            width: (16 * tile) as u32,
            height: (2 * tile) as u32,
            tile: tile as u32,
            rgba: Vec::new(),
            cells: HashMap::from([("a", 17)]),
        };
        assert_eq!(atlas.uv("a", [0.0, 0.0]), [1.0 / 16.0, 0.5]);
        assert_eq!(atlas.uv("a", [1.0, 1.0]), [2.0 / 16.0, 1.0]);
    }

    #[test]
    fn read_all_returns_every_layer_bottom_first() {
        let root = std::env::temp_dir().join(format!("assets_read_all_{}", std::process::id()));
        let (top, bottom, empty) = (root.join("top"), root.join("bottom"), root.join("empty"));
        for (dir, text) in [
            (&top, Some("top")),
            (&bottom, Some("bottom")),
            (&empty, None),
        ] {
            std::fs::create_dir_all(dir.join("assets/minecraft/lang")).unwrap();
            if let Some(text) = text {
                std::fs::write(dir.join("assets/minecraft/lang/en_US.lang"), text).unwrap();
            }
        }
        let mut src = AssetSource::Layers(vec![
            AssetSource::Dir(top),
            AssetSource::Dir(empty),
            AssetSource::Dir(bottom),
        ]);
        let name = "assets/minecraft/lang/en_US.lang";
        assert_eq!(
            src.read_all(name),
            vec![b"bottom".to_vec(), b"top".to_vec()]
        );
        assert_eq!(src.read(name), Some(b"top".to_vec()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
