use std::path::{Path, PathBuf};

use ferrocraft_oneclient::{resolve_with, Error, Http};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

/// Replies recorded from Modrinth for the 76 hashes of a 1.8.9 OneClient cluster.
struct Recorded;

impl Http for Recorded {
    fn post_json(&self, url: &str, _: &Value) -> Result<Value, Error> {
        assert!(url.ends_with("/version_files"));
        Ok(fixture("version_files.json"))
    }
    fn get_json(&self, url: &str) -> Result<Value, Error> {
        if url.contains("/projects?ids=") {
            return Ok(fixture("projects.json"));
        }
        let id = url
            .split("/project/")
            .nth(1)
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        assert!(url.contains("%22forge%22") && url.contains("%221.8.9%22"));
        Ok(fixture("versions.json")[id].clone())
    }
    fn get_bytes(&self, _: &str) -> Result<Vec<u8>, Error> {
        unreachable!("the fixture test must not download")
    }
}

/// Serves bytes whose sha1 does not match the recorded one.
struct Corrupt;

impl Http for Corrupt {
    fn post_json(&self, u: &str, b: &Value) -> Result<Value, Error> {
        Recorded.post_json(u, b)
    }
    fn get_json(&self, u: &str) -> Result<Value, Error> {
        Recorded.get_json(u)
    }
    fn get_bytes(&self, _: &str) -> Result<Vec<u8>, Error> {
        Ok(b"not a jar".to_vec())
    }
}

fn cluster() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cluster.json")
}

#[test]
fn counts_of_the_76_jar_cluster() {
    let out = std::env::temp_dir().join(format!("fc-oneclient-ok-{}", std::process::id()));
    let r = resolve_with(&HonestFixture, &cluster(), &out).unwrap();
    let _ = std::fs::remove_dir_all(&out);
    assert_eq!(r.jars.len(), 22, "resolved");
    assert_eq!(r.denied.len(), 4, "denied {:?}", r.denied);
    assert_eq!(r.no_forge.len(), 50, "no forge build");
    for d in ["pylon", "keyyyyyyyy", "crashpatch", "legacy-betterhurtcam"] {
        assert!(r.denied.iter().any(|s| s == d), "{d}");
    }
}

#[test]
fn a_download_with_the_wrong_sha1_is_refused() {
    let out = std::env::temp_dir().join(format!("fc-oneclient-{}", std::process::id()));
    let err = resolve_with(&Corrupt, &cluster(), &out).unwrap_err();
    assert!(matches!(err, Error::Sha1 { .. }), "{err}");
    assert!(!out.exists() || std::fs::read_dir(&out).unwrap().next().is_none());
    let _ = std::fs::remove_dir_all(out);
}

/// `/projects` answers with no projects: a denied mod must not slip through under its raw id.
struct NoSlugs;

impl Http for NoSlugs {
    fn post_json(&self, u: &str, b: &Value) -> Result<Value, Error> {
        Recorded.post_json(u, b)
    }
    fn get_json(&self, u: &str) -> Result<Value, Error> {
        if u.contains("/projects?ids=") {
            return Ok(Value::Array(vec![]));
        }
        Recorded.get_json(u)
    }
    fn get_bytes(&self, u: &str) -> Result<Vec<u8>, Error> {
        Recorded.get_bytes(u)
    }
}

/// Every Forge version file lacks its sha1.
struct NoSha1;

impl Http for NoSha1 {
    fn post_json(&self, u: &str, b: &Value) -> Result<Value, Error> {
        Recorded.post_json(u, b)
    }
    fn get_json(&self, u: &str) -> Result<Value, Error> {
        let mut v = Recorded.get_json(u)?;
        if let Some(versions) = v.as_array_mut() {
            for ver in versions {
                for f in ver["files"].as_array_mut().into_iter().flatten() {
                    f["hashes"] = Value::Object(Default::default());
                }
            }
        }
        Ok(v)
    }
    fn get_bytes(&self, u: &str) -> Result<Vec<u8>, Error> {
        Recorded.get_bytes(u)
    }
}

#[test]
fn a_missing_slug_is_an_error_not_a_pass() {
    let out = std::env::temp_dir().join(format!("fc-oneclient-slug-{}", std::process::id()));
    let err = resolve_with(&NoSlugs, &cluster(), &out).unwrap_err();
    assert!(matches!(err, Error::Format(_)), "{err}");
}

#[test]
fn a_version_file_without_sha1_is_an_error_not_no_forge() {
    let out = std::env::temp_dir().join(format!("fc-oneclient-nosha-{}", std::process::id()));
    let err = resolve_with(&NoSha1, &cluster(), &out).unwrap_err();
    assert!(matches!(err, Error::Format(_)), "{err}");
}

/// Recorded replies, but each file's sha1 is the sha1 of its URL, which `get_bytes` returns:
/// the jars themselves are not in the fixture.
struct HonestFixture;

impl Http for HonestFixture {
    fn post_json(&self, u: &str, b: &Value) -> Result<Value, Error> {
        Recorded.post_json(u, b)
    }
    fn get_json(&self, u: &str) -> Result<Value, Error> {
        let mut v = Recorded.get_json(u)?;
        if let Some(vs) = v.as_array_mut() {
            for ver in vs {
                if let Some(fs) = ver["files"].as_array_mut() {
                    for f in fs {
                        let url = f["url"].as_str().unwrap().to_owned();
                        f["hashes"]["sha1"] = sha1_of(url.as_bytes()).into();
                    }
                }
            }
        }
        Ok(v)
    }
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, Error> {
        Ok(url.as_bytes().to_vec())
    }
}

fn sha1_of(b: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    Sha1::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
#[ignore = "downloads from Modrinth; needs OneClient cluster path in ONECLIENT_CLUSTER"]
fn live_download() {
    let cluster = std::env::var("ONECLIENT_CLUSTER")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cluster());
    let out = std::env::temp_dir().join("fc-oneclient-live");
    let r = ferrocraft_oneclient::resolve(&cluster, &out).unwrap();
    assert_eq!(
        (r.jars.len(), r.denied.len(), r.no_forge.len()),
        (22, 4, 50)
    );
}
