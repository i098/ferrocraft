//! Reads a OneClient cluster's `.oneclient-mods.json` (a `path` and a `sha1` per jar)
//! and fetches the Forge 1.8.9 twin of each Fabric jar from Modrinth.
//!
//! Flow: `POST /v2/version_files` (sha1 -> project), `GET /v2/projects` (slugs, for the
//! deny list), `GET /v2/project/<id>/version?game_versions=["1.8.9"]&loaders=["forge"]`,
//! then download the newest file and check its sha1.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha1::{Digest, Sha1};

const API: &str = "https://api.modrinth.com/v2";
const MANIFEST: &str = ".oneclient-mods.json";
/// These patch what the host replaces (LWJGL keyboard, crash screens) or download Essential.
pub const DENY: [&str; 4] = ["pylon", "keyyyyyyyy", "crashpatch", "legacy-betterhurtcam"];

#[derive(Debug)]
pub struct Resolved {
    /// Downloaded Forge jars, in `out_dir`.
    pub jars: Vec<PathBuf>,
    /// Slugs on the deny list that the cluster holds.
    pub denied: Vec<String>,
    /// Slug (or jar file name when Modrinth does not know the hash) with no Forge 1.8.9 build.
    pub no_forge: Vec<String>,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Http(String),
    /// A manifest or API reply that does not have the expected shape.
    Format(String),
    Sha1 {
        file: String,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Http(e) => write!(f, "http: {e}"),
            Error::Format(e) => write!(f, "format: {e}"),
            Error::Sha1 {
                file,
                expected,
                actual,
            } => {
                write!(f, "sha1 of {file} is {actual}, Modrinth says {expected}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// The network side of the resolver; tests replace it with recorded replies.
pub trait Http {
    fn post_json(&self, url: &str, body: &Value) -> Result<Value, Error>;
    fn get_json(&self, url: &str) -> Result<Value, Error>;
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, Error>;
}

pub struct Modrinth;

fn http_err(e: ureq::Error) -> Error {
    Error::Http(e.to_string())
}

impl Http for Modrinth {
    fn post_json(&self, url: &str, body: &Value) -> Result<Value, Error> {
        let mut r = ureq::post(url)
            .header("User-Agent", UA)
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(http_err)?;
        serde_json::from_slice(&r.body_mut().read_to_vec().map_err(http_err)?)
            .map_err(|e| Error::Format(e.to_string()))
    }
    fn get_json(&self, url: &str) -> Result<Value, Error> {
        serde_json::from_slice(&self.get_bytes(url)?).map_err(|e| Error::Format(e.to_string()))
    }
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut r = ureq::get(url)
            .header("User-Agent", UA)
            .call()
            .map_err(http_err)?;
        r.body_mut().read_to_vec().map_err(http_err)
    }
}

const UA: &str = "ferrocraft/oneclient (github.com/ferrocraft)";

/// `cluster_mods` is the cluster's `.oneclient-mods.json`, or the cluster directory holding it.
pub fn resolve(cluster_mods: &Path, out_dir: &Path) -> Result<Resolved, Error> {
    resolve_with(&Modrinth, cluster_mods, out_dir)
}

pub fn resolve_with(
    api: &dyn Http,
    cluster_mods: &Path,
    out_dir: &Path,
) -> Result<Resolved, Error> {
    let manifest = if cluster_mods.is_dir() {
        cluster_mods.join(MANIFEST)
    } else {
        cluster_mods.to_path_buf()
    };
    let entries = read_manifest(&manifest)?;
    let hashes: Vec<&str> = entries.iter().map(|(_, h)| h.as_str()).collect();

    let found = api.post_json(
        &format!("{API}/version_files"),
        &json!({"hashes": hashes, "algorithm": "sha1"}),
    )?;
    let mut ids: Vec<&str> = hashes
        .iter()
        .filter_map(|h| found[*h]["project_id"].as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let slugs = project_slugs(api, &ids)?;

    let mut out = Resolved {
        jars: vec![],
        denied: vec![],
        no_forge: vec![],
    };
    let mut done: Vec<&str> = vec![];
    for (path, hash) in &entries {
        let Some(id) = found[hash.as_str()]["project_id"].as_str() else {
            out.no_forge.push(file_name(path));
            continue;
        };
        if done.contains(&id) {
            continue;
        }
        done.push(id);
        let slug = slugs
            .iter()
            .find(|(i, _)| i == id)
            .map_or(id, |(_, s)| s.as_str());
        if DENY.contains(&slug) {
            out.denied.push(slug.to_owned());
        } else if let Some(file) = newest_forge_file(api, id)? {
            out.jars.push(download(api, &file, out_dir)?);
        } else {
            out.no_forge.push(slug.to_owned());
        }
    }
    Ok(out)
}

fn read_manifest(path: &Path) -> Result<Vec<(String, String)>, Error> {
    let v: Value = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|e| Error::Format(format!("{}: {e}", path.display())))?;
    let entries = v["entries"]
        .as_array()
        .ok_or_else(|| Error::Format(format!("{}: no `entries`", path.display())))?;
    Ok(entries
        .iter()
        .filter_map(|e| {
            let (p, h) = (e["path"].as_str()?, e["hash"].as_str()?);
            p.ends_with(".jar")
                .then(|| (p.to_owned(), h.trim().to_ascii_lowercase()))
        })
        .collect())
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

fn project_slugs(api: &dyn Http, ids: &[&str]) -> Result<Vec<(String, String)>, Error> {
    let mut slugs = vec![];
    for chunk in ids.chunks(100) {
        let url = format!("{API}/projects?ids={}", enc(&json!(chunk).to_string()));
        for p in api.get_json(&url)?.as_array().into_iter().flatten() {
            if let (Some(id), Some(slug)) = (p["id"].as_str(), p["slug"].as_str()) {
                slugs.push((id.to_owned(), slug.to_owned()));
            }
        }
    }
    Ok(slugs)
}

struct File {
    url: String,
    name: String,
    sha1: String,
}

fn newest_forge_file(api: &dyn Http, project: &str) -> Result<Option<File>, Error> {
    let url = format!(
        "{API}/project/{project}/version?game_versions={}&loaders={}",
        enc(r#"["1.8.9"]"#),
        enc(r#"["forge"]"#)
    );
    let versions = api.get_json(&url)?;
    // ISO 8601 timestamps sort as text.
    let Some(v) = versions
        .as_array()
        .into_iter()
        .flatten()
        .max_by_key(|v| v["date_published"].as_str().unwrap_or(""))
    else {
        return Ok(None);
    };
    let files = v["files"].as_array().map_or(&[][..], Vec::as_slice);
    let f = files
        .iter()
        .find(|f| f["primary"] == true)
        .or(files.first());
    Ok(f.and_then(|f| {
        Some(File {
            url: f["url"].as_str()?.into(),
            name: f["filename"].as_str()?.into(),
            sha1: f["hashes"]["sha1"].as_str()?.to_ascii_lowercase(),
        })
    }))
}

fn download(api: &dyn Http, f: &File, out_dir: &Path) -> Result<PathBuf, Error> {
    // The name comes from the network: keep only its last component.
    let dest = out_dir.join(file_name(&f.name));
    if std::fs::read(&dest).is_ok_and(|b| sha1_hex(&b) == f.sha1) {
        return Ok(dest);
    }
    let bytes = api.get_bytes(&f.url)?;
    let actual = sha1_hex(&bytes);
    if actual != f.sha1 {
        return Err(Error::Sha1 {
            file: f.name.clone(),
            expected: f.sha1.clone(),
            actual,
        });
    }
    std::fs::create_dir_all(out_dir)?;
    let tmp = dest.with_extension("jar.part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

fn sha1_hex(b: &[u8]) -> String {
    Sha1::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// Percent-encode a query value.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
