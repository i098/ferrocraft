//! Asset loading from the 1.8.9 client jar (or an extracted copy): block atlas and the ascii font.

use crate::blocks::texture_names;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const TILE: usize = 16;
const COLUMNS: usize = 16;

pub enum AssetSource {
    Jar(zip::ZipArchive<std::fs::File>),
    /// An extracted tree, or a launcher `assets` dir whose `indexes/1.8.json` maps names to `objects/xx/hash`.
    Dir {
        dir: PathBuf,
        index: HashMap<String, String>,
    },
}

impl AssetSource {
    pub fn open(path: &Path) -> Option<AssetSource> {
        if path.is_dir() {
            return Some(AssetSource::Dir {
                dir: path.to_path_buf(),
                index: read_index(path),
            });
        }
        let file = std::fs::File::open(path).ok()?;
        zip::ZipArchive::new(file).ok().map(AssetSource::Jar)
    }

    /// `name` is a jar path such as `assets/minecraft/textures/font/ascii.png`.
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        match self {
            AssetSource::Jar(zip) => {
                let mut bytes = Vec::new();
                zip.by_name(name).ok()?.read_to_end(&mut bytes).ok()?;
                Some(bytes)
            }
            AssetSource::Dir { dir, index } => {
                let stripped = name.strip_prefix("assets/").unwrap_or(name);
                std::fs::read(dir.join(name))
                    .or_else(|_| std::fs::read(dir.join(stripped)))
                    .ok()
                    .or_else(|| {
                        let hash = index.get(stripped)?;
                        std::fs::read(dir.join("objects").join(hash.get(..2)?).join(hash)).ok()
                    })
            }
        }
    }
}

fn read_index(dir: &Path) -> HashMap<String, String> {
    let Ok(bytes) = std::fs::read(dir.join("indexes/1.8.json")) else {
        return HashMap::new();
    };
    let index: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    index["objects"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, o)| Some((name.clone(), o["hash"].as_str()?.to_string())))
        .collect()
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
            let image = source.read(&path).and_then(|b| decode_png(&b));
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

fn missing_tile() -> Image {
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

/// `ascii.png`: 16x16 glyph cells of 8x8 texels with per-glyph widths.
pub struct Font {
    pub image: Image,
    /// Pixel width of the glyph bitmap (last used column + 1).
    glyph: [u8; 256],
}

impl Font {
    pub fn load(source: &mut AssetSource) -> Option<Font> {
        let image = decode_png(&source.read("assets/minecraft/textures/font/ascii.png")?)?;
        if image.width % 16 != 0 || image.width != image.height {
            return None;
        }
        let cell = image.width / 16;
        let mut glyph = [0u8; 256];
        for (c, g) in glyph.iter_mut().enumerate() {
            let (gx, gy) = ((c % 16) * cell, (c / 16) * cell);
            let used = (0..cell).rev().find(|&x| {
                (0..cell).any(|y| image.rgba[((gy + y) * image.width + gx + x) * 4 + 3] != 0)
            });
            // Scale the used width back to the 8-texel grid the vanilla advance is defined in.
            *g = used.map_or(0, |x| ((x + 1) * 8).div_ceil(cell) as u8);
        }
        Some(Font { image, glyph })
    }

    /// Glyph bitmap width in 8-texel units.
    pub fn glyph_width(&self, c: u8) -> u32 {
        self.glyph[c as usize] as u32
    }

    /// Vanilla `FontRenderer` advance: glyph width + 1, and 4 for the space.
    pub fn advance(&self, c: u8) -> u32 {
        if c == b' ' {
            4
        } else {
            self.glyph[c as usize] as u32 + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_source_resolves_names_through_the_hashed_index() {
        let dir = std::env::temp_dir().join(format!("fc-assets-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("indexes")).unwrap();
        std::fs::create_dir_all(dir.join("objects/ab")).unwrap();
        std::fs::write(
            dir.join("indexes/1.8.json"),
            r#"{"objects":{"minecraft/textures/font/ascii.png":{"hash":"abcdef","size":3}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("objects/ab/abcdef"), b"png").unwrap();
        let mut src = AssetSource::open(&dir).unwrap();
        let hit = src.read("assets/minecraft/textures/font/ascii.png");
        let miss = src.read("assets/minecraft/textures/blocks/stone.png");
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(hit.as_deref(), Some(&b"png"[..]));
        assert_eq!(miss, None);
    }
}
