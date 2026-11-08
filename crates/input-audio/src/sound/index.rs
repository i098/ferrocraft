//! Vanilla 1.8.9 asset index plus `sounds.json`: event name -> weighted `.ogg` choices.
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    pub file: PathBuf,
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

pub struct SoundIndex {
    events: HashMap<String, Event>,
}

impl SoundIndex {
    /// `minecraft_dir` holds `assets/indexes` and `assets/objects`, as the vanilla launcher lays them out.
    pub fn load(minecraft_dir: &Path, version: &str) -> Result<Self, String> {
        let assets = minecraft_dir.join("assets");
        let index = ["1.8", version]
            .iter()
            .map(|v| assets.join("indexes").join(format!("{v}.json")))
            .find(|p| p.is_file())
            .ok_or("no asset index in assets/indexes")?;
        let objects = parse_objects(&std::fs::read(&index).map_err(|e| e.to_string())?)?;
        let object = |name: &str| {
            objects.get(name).map(|h| assets.join("objects").join(&h[..2]).join(h))
        };
        let json_path = object("minecraft/sounds.json").ok_or("sounds.json not in asset index")?;
        let json = std::fs::read(&json_path).map_err(|e| e.to_string())?;
        Self::from_json(&json, |name| object(&format!("minecraft/sounds/{name}.ogg")))
    }

    /// `resolve` maps a sound name such as `random/click` to its `.ogg` path.
    pub fn from_json(
        json: &[u8],
        resolve: impl Fn(&str) -> Option<PathBuf>,
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
                    variants.push(Variant { file: path, volume, pitch, weight });
                }
            }
            events.insert(name.clone(), Event { category, variants });
        }
        Ok(Self { events })
    }

    pub fn event(&self, name: &str) -> Option<&Event> {
        self.events.get(name)
    }
}

fn parse_objects(index: &[u8]) -> Result<HashMap<String, String>, String> {
    let v: Value = serde_json::from_slice(index).map_err(|e| e.to_string())?;
    let objs = v["objects"].as_object().ok_or("index has no objects")?;
    Ok(objs
        .iter()
        .filter_map(|(k, o)| Some((k.clone(), o["hash"].as_str().filter(|h| h.len() > 2)?.to_owned())))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strings_objects_and_weights() {
        let json = br#"{"random.click":{"category":"master","sounds":["random/click"]},
            "mob.pig.say":{"category":"neutral","sounds":[{"name":"mob/pig/say1","weight":3,"volume":0.5},{"name":"mob/pig/say2"},"missing"]}}"#;
        let idx = SoundIndex::from_json(json, |n| (n != "missing").then(|| PathBuf::from(n))).unwrap();
        let pig = idx.event("mob.pig.say").unwrap();
        assert_eq!(pig.category, Category::Neutral);
        assert_eq!(pig.variants.len(), 2);
        assert_eq!(pig.pick(0).unwrap().volume, 0.5);
        assert_eq!(pig.pick(2).unwrap().volume, 0.5);
        assert_eq!(pig.pick(3).unwrap().weight, 1);
        assert_eq!(idx.event("random.click").unwrap().category, Category::Master);
    }
}
