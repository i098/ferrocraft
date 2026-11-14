//! Vanilla 1.8.9 asset index plus `sounds.json`: event name -> weighted `.ogg` choices.
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Where one `.ogg` lives: a plain file (assets or a pack folder) or an entry of a pack zip.
#[derive(Clone)]
pub enum SoundFile {
    Path(PathBuf),
    Zip {
        archive: Arc<Mutex<zip::ZipArchive<std::fs::File>>>,
        entry: String,
    },
}

impl std::fmt::Debug for SoundFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SoundFile::Path(p) => f.debug_tuple("Path").field(p).finish(),
            SoundFile::Zip { entry, .. } => f.debug_struct("Zip").field("entry", entry).finish(),
        }
    }
}

impl PartialEq for SoundFile {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SoundFile::Path(a), SoundFile::Path(b)) => a == b,
            (
                SoundFile::Zip {
                    archive: a,
                    entry: x,
                },
                SoundFile::Zip {
                    archive: b,
                    entry: y,
                },
            ) => Arc::ptr_eq(a, b) && x == y,
            _ => false,
        }
    }
}

impl SoundFile {
    pub fn read(&self) -> Option<Cursor<Vec<u8>>> {
        match self {
            SoundFile::Path(p) => std::fs::read(p).ok(),
            SoundFile::Zip { archive, entry } => {
                let mut z = archive.lock().ok()?;
                let mut bytes = Vec::new();
                z.by_name(entry).ok()?.read_to_end(&mut bytes).ok()?;
                Some(bytes)
            }
        }
        .map(Cursor::new)
    }
}

/// One selected resource pack (a folder or a zip), as `FallbackResourceManager` layers it.
#[derive(Clone)]
pub struct Pack {
    path: PathBuf,
    zip: Option<Arc<Mutex<zip::ZipArchive<std::fs::File>>>>,
}

impl std::fmt::Debug for Pack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Pack").field(&self.path).finish()
    }
}

impl Pack {
    /// A zip is opened once here; an unreadable zip or a folder keeps no archive.
    pub fn new(path: PathBuf) -> Self {
        let zip = if path.is_dir() {
            None
        } else {
            std::fs::File::open(&path)
                .ok()
                .and_then(|f| zip::ZipArchive::new(f).ok())
                .map(|z| Arc::new(Mutex::new(z)))
        };
        Self { path, zip }
    }

    /// `name` is a path under `assets/`, such as `minecraft/sounds.json`.
    fn locate(&self, name: &str) -> Option<SoundFile> {
        let entry = format!("assets/{name}");
        let Some(zip) = &self.zip else {
            let p = self.path.join(&entry);
            return p.is_file().then_some(SoundFile::Path(p));
        };
        zip.lock().ok()?.by_name(&entry).ok()?;
        Some(SoundFile::Zip {
            archive: zip.clone(),
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
    /// Events a `"type": "event"` entry points at, per event; folded into `events` by `merge`.
    refs: HashMap<String, Vec<String>>,
    replace: HashSet<String>,
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
                index.merge(over);
            }
        }
        Ok(index)
    }

    /// `SoundHandler.loadSoundResource`: an event of `over` replaces the old one when it is new
    /// or has `"replace": true`; otherwise its sounds are appended to the old ones. An event
    /// reference takes the sounds of the named event, from `over` first, else from the old index.
    /// An event left without sounds keeps its old definition rather than going silent.
    fn merge(&mut self, over: SoundIndex) {
        let mut merged = Vec::new();
        for (name, ev) in &over.events {
            let mut ev = ev.clone();
            let mut seen = vec![name.as_str()];
            ev.variants.extend(self.expand(&over, name, &mut seen));
            merged.push((name.clone(), ev));
        }
        for (name, ev) in merged {
            match self.events.get_mut(&name) {
                Some(_)
                    if ev.variants.is_empty()
                        && over.refs.get(&name).is_some_and(|r| !r.is_empty()) => {}
                Some(old) if !over.replace.contains(&name) => old.variants.extend(ev.variants),
                _ => {
                    self.events.insert(name, ev);
                }
            }
        }
    }

    fn expand<'a>(
        &self,
        over: &'a SoundIndex,
        name: &str,
        seen: &mut Vec<&'a str>,
    ) -> Vec<Variant> {
        let mut out = Vec::new();
        for r in over.refs.get(name).into_iter().flatten() {
            if seen.contains(&r.as_str()) {
                continue;
            }
            seen.push(r);
            if let Some(ev) = over.events.get(r) {
                out.extend(ev.variants.iter().cloned());
                out.extend(self.expand(over, r, seen));
            } else if let Some(ev) = self.events.get(r) {
                out.extend(ev.variants.iter().cloned());
            }
        }
        out
    }

    /// `resolve` maps a sound name such as `random/click` to its `.ogg` file. A malformed
    /// record is an `Err`, as vanilla's `SoundListSerializer` rejects the whole file.
    pub fn from_json(
        json: &[u8],
        resolve: impl Fn(&str) -> Option<SoundFile>,
    ) -> Result<Self, String> {
        let root: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let root = root.as_object().ok_or("sounds.json is not an object")?;
        let mut index = Self {
            events: HashMap::new(),
            refs: HashMap::new(),
            replace: HashSet::new(),
        };
        for (name, ev) in root {
            let bad = |what: &str| format!("{name}: bad {what}");
            let (category, replace, sounds) = parse_header(ev).map_err(|what| bad(what))?;
            if replace {
                index.replace.insert(name.clone());
            }
            let mut variants = Vec::new();
            let mut refs = Vec::new();
            for s in sounds {
                let (file, volume, pitch, weight, is_event) =
                    parse_sound(s).map_err(|what| bad(what))?;
                if is_event {
                    refs.push(file.to_owned());
                } else if let Some(path) = resolve(file) {
                    variants.push(Variant {
                        file: path,
                        volume,
                        pitch,
                        weight,
                    });
                } else if file.contains(':') {
                    eprintln!("input-audio: sound event {name}: dropped variant {file}");
                }
            }
            let category = Category::parse(category);
            index.refs.insert(name.clone(), refs);
            index
                .events
                .insert(name.clone(), Event { category, variants });
        }
        Ok(index)
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

/// One event record: (category name, `replace`, `sounds` entries).
fn parse_header(ev: &Value) -> Result<(&str, bool, &[Value]), &'static str> {
    let ev = ev.as_object().ok_or("record")?;
    let category = match ev.get("category") {
        None => "master",
        Some(Value::String(s)) => s,
        Some(_) => return Err("category"),
    };
    let replace = match ev.get("replace") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("replace"),
    };
    let sounds = match ev.get("sounds") {
        None => &[][..],
        Some(Value::Array(a)) => a,
        Some(_) => return Err("sounds"),
    };
    Ok((category, replace, sounds))
}

/// One `sounds` entry: (name, volume, pitch, weight, is an event reference).
fn parse_sound(s: &Value) -> Result<(&str, f32, f32, u32, bool), &'static str> {
    let o = match s {
        Value::String(n) => return Ok((n, 1.0, 1.0, 1, false)),
        Value::Object(o) => o,
        _ => return Err("sound entry"),
    };
    let num = |k: &'static str| match o.get(k) {
        None => Ok(1.0),
        Some(v) => v.as_f64().map(|f| f as f32).ok_or(k),
    };
    let weight = match o.get("weight") {
        None => 1,
        Some(v) => v
            .as_u64()
            .and_then(|w| u32::try_from(w).ok())
            .ok_or("weight")?,
    };
    let is_event = match o.get("type") {
        None => false,
        Some(t) => match t.as_str() {
            Some("sound") => false,
            Some("event") => true,
            _ => return Err("type"),
        },
    };
    let file = o.get("name").and_then(Value::as_str).ok_or("name")?;
    Ok((file, num("volume")?, num("pitch")?, weight, is_event))
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
        let packed = SoundIndex::load(&mc, "1.8.9", &[Pack::new(pack)]).unwrap();
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

    fn idx(json: &str) -> SoundIndex {
        SoundIndex::from_json(json.as_bytes(), |n| Some(SoundFile::Path(PathBuf::from(n)))).unwrap()
    }

    fn files(i: &SoundIndex, n: &str) -> Vec<PathBuf> {
        i.event(n)
            .unwrap()
            .variants
            .iter()
            .map(|v| match &v.file {
                SoundFile::Path(p) => p.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    const BASE: &str = r#"{"random.click":{"sounds":["a"]},"random.pop":{"sounds":["b"]}}"#;

    #[test]
    fn event_reference_takes_named_event_sounds() {
        let mut i = idx(BASE);
        i.merge(idx(
            r#"{"random.click":{"replace":true,"sounds":[{"name":"random.pop","type":"event"}]}}"#,
        ));
        assert_eq!(files(&i, "random.click"), [PathBuf::from("b")]);
    }

    #[test]
    fn unknown_event_reference_keeps_old_sound() {
        let mut i = idx(BASE);
        i.merge(idx(
            r#"{"random.click":{"replace":true,"sounds":[{"name":"nope","type":"event"}]}}"#,
        ));
        assert_eq!(files(&i, "random.click"), [PathBuf::from("a")]);
    }

    #[test]
    fn event_reference_cycle_terminates() {
        let mut i = idx(BASE);
        i.merge(idx(r#"{"x":{"sounds":["c",{"name":"y","type":"event"}]},
                "y":{"sounds":[{"name":"x","type":"event"}]}}"#));
        assert_eq!(files(&i, "x"), [PathBuf::from("c")]);
        assert_eq!(files(&i, "y"), [PathBuf::from("c")]);
    }

    #[test]
    fn empty_replace_mutes_event() {
        let mut i = idx(BASE);
        i.merge(idx(r#"{"random.click":{"replace":true,"sounds":[]}}"#));
        assert!(i.event("random.click").unwrap().variants.is_empty());
    }

    #[test]
    fn malformed_records_are_errors_not_panics() {
        for bad in [
            r#"{"a":{"sounds":[{}]}}"#,
            r#"{"a":{"sounds":[{"name":"x","weight":4294967296}]}}"#,
            r#"{"a":{"replace":"yes"}}"#,
            r#"{"a":{"sounds":"x"}}"#,
            r#"{"a":{"category":1}}"#,
            r#"{"a":{"sounds":[{"name":"x","type":"bogus"}]}}"#,
        ] {
            assert!(
                SoundIndex::from_json(bad.as_bytes(), |_| None).is_err(),
                "{bad}"
            );
        }
    }

    fn vanilla(mc: &Path) {
        write(mc, "assets/objects/ab/abcdef", b"vanilla");
        write(
            mc,
            "assets/objects/ab/abcdef2",
            br#"{"random.click":{"sounds":["random/click"]},"random.pop":{"sounds":["random/pop"]}}"#,
        );
        write(
            mc,
            "assets/indexes/1.8.json",
            br#"{"objects":{"minecraft/sounds.json":{"hash":"abcdef2"},
            "minecraft/sounds/random/click.ogg":{"hash":"abcdef"},
            "minecraft/sounds/random/pop.ogg":{"hash":"abcdef"}}}"#,
        );
    }

    fn bytes(i: &SoundIndex, n: &str) -> Vec<Vec<u8>> {
        i.event(n).unwrap().variants.iter()
            .map(|v| v.file.read().unwrap().into_inner())
            .collect()
    }

    #[test]
    fn zip_pack_replaces_and_event_entry_appends() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("fc-snd-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mc = dir.join("mc");
        vanilla(&mc);
        let zp = dir.join("pack.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zp).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("assets/minecraft/sounds/random/zclick.ogg", o).unwrap();
        w.write_all(b"zipclick").unwrap();
        w.start_file("assets/minecraft/sounds.json", o).unwrap();
        w.write_all(
            br#"{"random.click":{"replace":true,"sounds":["random/zclick"]},
            "random.pop":{"sounds":[{"name":"random.click","type":"event"}]}}"#,
        )
        .unwrap();
        w.finish().unwrap();
        let i = SoundIndex::load(&mc, "1.8.9", &[Pack::new(zp)]).unwrap();
        assert_eq!(bytes(&i, "random.click"), [b"zipclick".to_vec()]);
        assert_eq!(bytes(&i, "random.pop"), [b"vanilla".to_vec(), b"zipclick".to_vec()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_pack_keeps_vanilla() {
        let dir = std::env::temp_dir().join(format!("fc-snd-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mc = dir.join("mc");
        vanilla(&mc);
        let i = SoundIndex::load(&mc, "1.8.9", &[Pack::new(dir.join("nope.zip"))]).unwrap();
        assert_eq!(bytes(&i, "random.click"), [b"vanilla".to_vec()]);
        // unreadable zip, and a folder pack whose sounds.json is malformed
        write(&dir, "bad.zip", b"not a zip");
        let folder = dir.join("folder");
        write(&folder, "assets/minecraft/sounds/random/click.ogg", b"custom");
        write(&folder, "assets/minecraft/sounds.json", br#"{"random.click":{"sounds":[{}]}}"#);
        let packs = [Pack::new(dir.join("bad.zip")), Pack::new(folder)];
        let i = SoundIndex::load(&mc, "1.8.9", &packs).unwrap();
        // bad sounds.json skipped whole (pop untouched); the pack's click.ogg still overrides the file
        assert_eq!(bytes(&i, "random.click"), [b"custom".to_vec()]);
        assert_eq!(bytes(&i, "random.pop"), [b"vanilla".to_vec()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
