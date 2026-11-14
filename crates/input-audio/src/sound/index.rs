//! Vanilla 1.8.9 asset index plus `sounds.json`: event name -> weighted `.ogg` choices.
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// Where one `.ogg` lives: a plain file (assets or a pack folder) or an entry of a pack zip.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundFile {
    Path(PathBuf),
    Zip { zip: PathBuf, entry: String },
}

impl SoundFile {
    pub fn read(&self) -> Option<Cursor<Vec<u8>>> {
        match self {
            SoundFile::Path(p) => std::fs::read(p).ok(),
            SoundFile::Zip { zip, entry } => {
                let mut z = zip::ZipArchive::new(std::fs::File::open(zip).ok()?).ok()?;
                let mut bytes = Vec::new();
                z.by_name(entry).ok()?.read_to_end(&mut bytes).ok()?;
                Some(bytes)
            }
        }
        .map(Cursor::new)
    }
}

/// One selected resource pack (a folder or a zip), as `FallbackResourceManager` layers it.
#[derive(Clone, Debug)]
pub struct Pack(pub PathBuf);

impl Pack {
    /// `name` is a path under `assets/`, such as `minecraft/sounds.json`.
    fn locate(&self, name: &str) -> Option<SoundFile> {
        let entry = format!("assets/{name}");
        if self.0.is_dir() {
            let p = self.0.join(&entry);
            return p.is_file().then_some(SoundFile::Path(p));
        }
        let mut z = zip::ZipArchive::new(std::fs::File::open(&self.0).ok()?).ok()?;
        z.by_name(&entry).ok()?;
        Some(SoundFile::Zip {
            zip: self.0.clone(),
            entry,
        })
    }
}

/// Vanilla sound categories (`SoundCategory`), each with its own options slider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Master,
    Music,
    Record,
    Weather,
    Block,
    Hostile,
    Neutral,
    Player,
    Ambient,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Master,
        Category::Music,
        Category::Record,
        Category::Weather,
        Category::Block,
        Category::Hostile,
        Category::Neutral,
        Category::Player,
        Category::Ambient,
    ];

    fn parse(s: &str) -> Category {
        match s {
            "music" => Category::Music,
            "record" => Category::Record,
            "weather" => Category::Weather,
            "block" => Category::Block,
            "hostile" => Category::Hostile,
            "neutral" => Category::Neutral,
            "player" => Category::Player,
            "ambient" => Category::Ambient,
            _ => Category::Master,
        }
    }

    /// Key used in vanilla `options.txt` (`soundCategory_<name>`).
    pub fn option_key(self) -> &'static str {
        match self {
            Category::Master => "soundCategory_master",
            Category::Music => "soundCategory_music",
            Category::Record => "soundCategory_record",
            Category::Weather => "soundCategory_weather",
            Category::Block => "soundCategory_block",
            Category::Hostile => "soundCategory_hostile",
            Category::Neutral => "soundCategory_neutral",
            Category::Player => "soundCategory_player",
            Category::Ambient => "soundCategory_ambient",
        }
    }
}

/// One playable file with its `sounds.json` volume/pitch scale and selection weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub file: SoundFile,
    pub volume: f32,
    pub pitch: f32,
    pub weight: u32,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub category: Category,
    pub variants: Vec<Variant>,
}

impl Event {
    /// Weighted pick; `roll` is any random `u32`.
    pub fn pick(&self, roll: u32) -> Option<&Variant> {
        let total: u32 = self.variants.iter().map(|v| v.weight).sum();
        if total == 0 {
            return None;
        }
        let mut r = roll % total;
        self.variants.iter().find(|v| {
            if r < v.weight {
                true
            } else {
                r -= v.weight;
                false
            }
        })
    }
}

/// The launcher's `assets/indexes/<version>.json`: asset name -> file under `assets/objects`.
pub struct AssetIndex {
    objects_dir: PathBuf,
    objects: HashMap<String, String>,
}

impl AssetIndex {
    /// `minecraft_dir` holds `assets/indexes` and `assets/objects`, as the vanilla launcher lays them out.
    pub fn load(minecraft_dir: &Path, version: &str) -> Result<Self, String> {
        let assets = minecraft_dir.join("assets");
        let index = ["1.8", version]
            .iter()
            .map(|v| assets.join("indexes").join(format!("{v}.json")))
            .find(|p| p.is_file())
            .ok_or("no asset index in assets/indexes")?;
        let objects = parse_objects(&std::fs::read(&index).map_err(|e| e.to_string())?)?;
        Ok(Self {
            objects_dir: assets.join("objects"),
            objects,
        })
    }

    /// The file of asset `name`, such as `minecraft/sounds.json` or `pack.mcmeta`.
    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.objects
            .get(name)
            .map(|h| self.objects_dir.join(&h[..2]).join(h))
    }
}

pub struct SoundIndex {
    events: HashMap<String, Event>,
}

impl SoundIndex {
    /// `SoundHandler.onResourceManagerReload`: the base `sounds.json`, then each pack's, bottom
    /// pack first. `packs` is top first. A sound file comes from the topmost layer that has it.
    pub fn load(minecraft_dir: &Path, version: &str, packs: &[Pack]) -> Result<Self, String> {
        let assets = AssetIndex::load(minecraft_dir, version)?;
        let json_path = assets
            .path("minecraft/sounds.json")
            .ok_or("sounds.json not in asset index")?;
        let json = std::fs::read(&json_path).map_err(|e| e.to_string())?;
        let resolve = &|name: &str| {
            let rel = format!("minecraft/sounds/{name}.ogg");
            packs
                .iter()
                .find_map(|p| p.locate(&rel))
                .or_else(|| assets.path(&rel).map(SoundFile::Path))
        };
        let mut index = Self::from_json(&json, resolve)?;
        for pack in packs.iter().rev() {
            let Some(bytes) = pack.locate("minecraft/sounds.json").and_then(|f| f.read()) else {
                continue;
            };
            // A broken pack file is skipped, as vanilla logs and skips it.
            if let Ok(over) = Self::from_json(bytes.get_ref(), resolve) {
                index.merge(over, bytes.get_ref());
            }
        }
        Ok(index)
    }

    /// `SoundHandler.loadSoundResource`: an event of `over` replaces the old one when it is new
    /// or has `"replace": true`; otherwise its sounds are appended to the old ones.
    fn merge(&mut self, over: SoundIndex, json: &[u8]) {
        let root: Value = serde_json::from_slice(json).unwrap_or_default();
        for (name, ev) in over.events {
            let replace = root[&name]["replace"].as_bool().unwrap_or(false);
            match self.events.get_mut(&name) {
                Some(old) if !replace => old.variants.extend(ev.variants),
                _ => {
                    self.events.insert(name, ev);
                }
            }
        }
    }

    /// `resolve` maps a sound name such as `random/click` to its `.ogg` file.
    pub fn from_json(
        json: &[u8],
        resolve: impl Fn(&str) -> Option<SoundFile>,
    ) -> Result<Self, String> {
        let root: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let root = root.as_object().ok_or("sounds.json is not an object")?;
        let mut events = HashMap::new();
        for (name, ev) in root {
            let category = Category::parse(ev["category"].as_str().unwrap_or("master"));
            let mut variants = Vec::new();
            for s in ev["sounds"].as_array().into_iter().flatten() {
                let (file, volume, pitch, weight) = match s {
                    Value::String(n) => (n.as_str(), 1.0, 1.0, 1),
                    Value::Object(o) => (
                        o["name"].as_str().unwrap_or(""),
                        o.get("volume").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                        o.get("pitch").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                        o.get("weight").and_then(Value::as_u64).unwrap_or(1) as u32,
                    ),
                    _ => continue,
                };
                if let Some(path) = resolve(file) {
                    variants.push(Variant {
                        file: path,
                        volume,
                        pitch,
                        weight,
                    });
                }
            }
            events.insert(name.clone(), Event { category, variants });
        }
        Ok(Self { events })
    }

    pub fn event(&self, name: &str) -> Option<&Event> {
        self.events.get(name)
    }

    /// `SoundHandler.getRandomSoundFromCategories`: one event of `cats`, chosen by `roll`.
    pub fn random_in(&self, cats: &[Category], roll: u32) -> Option<&str> {
        let names: Vec<&String> = self
            .events
            .iter()
            .filter(|(_, e)| cats.contains(&e.category))
            .map(|(n, _)| n)
            .collect();
        (!names.is_empty()).then(|| names[roll as usize % names.len()].as_str())
    }
}

fn parse_objects(index: &[u8]) -> Result<HashMap<String, String>, String> {
    let v: Value = serde_json::from_slice(index).map_err(|e| e.to_string())?;
    let objs = v["objects"].as_object().ok_or("index has no objects")?;
    Ok(objs
        .iter()
        .filter_map(|(k, o)| {
            Some((
                k.clone(),
                o["hash"].as_str().filter(|h| h.len() > 2)?.to_owned(),
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    #[test]
    fn pack_file_wins_and_replace_rule() {
        let dir = std::env::temp_dir().join(format!("fc-snd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mc, pack) = (dir.join("mc"), dir.join("pack"));
        write(&mc, "assets/objects/ab/abcdef", b"vanilla");
        let base = br#"{"random.click":{"category":"master","sounds":["random/click"]},
            "random.pop":{"category":"player","sounds":["random/pop"]}}"#;
        write(&mc, "assets/objects/ab/abcdef2", base);
        write(
            &mc,
            "assets/indexes/1.8.json",
            br#"{"objects":{"minecraft/sounds.json":{"hash":"abcdef2"},
            "minecraft/sounds/random/click.ogg":{"hash":"abcdef"},
            "minecraft/sounds/random/pop.ogg":{"hash":"abcdef"}}}"#,
        );
        write(&pack, "assets/minecraft/sounds/random/click.ogg", b"custom");
        write(&pack, "assets/minecraft/sounds/random/extra.ogg", b"extra");
        write(
            &pack,
            "assets/minecraft/sounds.json",
            br#"{"random.pop":{"sounds":["random/extra"]},
                "random.click":{"replace":true,"sounds":["random/click"]}}"#,
        );
        let plain = SoundIndex::load(&mc, "1.8.9", &[]).unwrap();
        let read = |i: &SoundIndex, n: &str, k: usize| {
            i.event(n).unwrap().variants[k]
                .file
                .read()
                .unwrap()
                .into_inner()
        };
        assert_eq!(read(&plain, "random.click", 0), b"vanilla");
        let packed = SoundIndex::load(&mc, "1.8.9", &[Pack(pack)]).unwrap();
        assert_eq!(read(&packed, "random.click", 0), b"custom");
        assert_eq!(packed.event("random.click").unwrap().variants.len(), 1);
        // no `replace`: appended after the vanilla variant
        assert_eq!(packed.event("random.pop").unwrap().variants.len(), 2);
        assert_eq!(read(&packed, "random.pop", 1), b"extra");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_strings_objects_and_weights() {
        let json = br#"{"random.click":{"category":"master","sounds":["random/click"]},
            "mob.pig.say":{"category":"neutral","sounds":[{"name":"mob/pig/say1","weight":3,"volume":0.5},{"name":"mob/pig/say2"},"missing"]}}"#;
        let idx = SoundIndex::from_json(json, |n| {
            (n != "missing").then(|| SoundFile::Path(PathBuf::from(n)))
        })
        .unwrap();
        let pig = idx.event("mob.pig.say").unwrap();
        assert_eq!(pig.category, Category::Neutral);
        assert_eq!(pig.variants.len(), 2);
        assert_eq!(pig.pick(0).unwrap().volume, 0.5);
        assert_eq!(pig.pick(2).unwrap().volume, 0.5);
        assert_eq!(pig.pick(3).unwrap().weight, 1);
        assert_eq!(
            idx.event("random.click").unwrap().category,
            Category::Master
        );
        for roll in 0..4 {
            assert_eq!(
                idx.random_in(&[Category::Neutral], roll),
                Some("mob.pig.say")
            );
        }
        assert_eq!(idx.random_in(&[Category::Weather], 0), None);
    }
}
