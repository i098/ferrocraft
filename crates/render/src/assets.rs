//! Asset loading from the 1.8.9 client jar (or an extracted copy): block atlas and the ascii font.

use crate::blocks::texture_names;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const TILE: usize = 16;
const COLUMNS: usize = 16;

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

/// Block texture atlas: 16x16 tiles in a grid; tile 0 is solid white (flat colors use it).
pub struct Atlas {
    pub textured: bool,
    pub width: u32,
    pub height: u32,
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
            rgba: vec![255; TILE * TILE * 4],
            cells: HashMap::new(),
        }
    }

    pub fn from_assets(source: &mut AssetSource) -> Atlas {
        let names = texture_names();
        let rows = (names.len() + 1).div_ceil(COLUMNS);
        let (w, h) = (COLUMNS * TILE, rows * TILE);
        let mut rgba = vec![0u8; w * h * 4];
        let mut cells = HashMap::new();
        blit(
            &mut rgba,
            w,
            0,
            &Image {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            },
        );
        let mut loaded = 0;
        for (i, name) in names.iter().enumerate() {
            let cell = i + 1;
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
            loaded += usize::from(image.is_some());
            blit(&mut rgba, w, cell, &image.unwrap_or_else(missing_tile));
            cells.insert(*name, cell as u32);
        }
        if loaded == 0 {
            eprintln!("ferrocraft-render: no block textures found; blocks use the missing tile");
        }
        Atlas {
            textured: true,
            width: w as u32,
            height: h as u32,
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
            (cx + u) * TILE as f32 / self.width as f32,
            (cy + v) * TILE as f32 / self.height as f32,
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

/// Copy the top-left square of `image` into grid `cell`, nearest-sampled to 16x16 (animated strips keep frame 0).
fn blit(atlas: &mut [u8], atlas_width: usize, cell: usize, image: &Image) {
    let side = image.width.min(image.height).max(1);
    let (ox, oy) = ((cell % COLUMNS) * TILE, (cell / COLUMNS) * TILE);
    for y in 0..TILE {
        for x in 0..TILE {
            let (sx, sy) = (x * side / TILE, y * side / TILE);
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
