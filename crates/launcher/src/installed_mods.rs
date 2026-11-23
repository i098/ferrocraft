use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::{oneclient, FABRIC_JAR};

fn data_root(mods: &Path) -> Option<&Path> {
    let clusters = mods.parent()?.parent()?;
    if mods.ends_with("mods") && clusters.ends_with("clusters") {
        clusters.parent()
    } else {
        None
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || fs::canonicalize(a)
            .ok()
            .zip(fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| a == b)
}

fn case_alias(dir: &Path, name: &std::ffi::OsStr, other: &std::ffi::OsStr) -> std::io::Result<bool> {
    let exists = |name: &std::ffi::OsStr| match fs::symlink_metadata(dir.join(name)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    };
    match (exists(name)?, exists(other)?) {
        (true, true) => {
            let mut distinct = 0;
            for entry in fs::read_dir(dir)? {
                let entry = entry?.file_name();
                distinct += usize::from(entry == name || entry == other);
            }
            Ok(distinct < 2)
        }
        (false, false) => {
            let probe = match tempfile::Builder::new().prefix("Fc-case-").tempfile_in(dir) {
                Ok(probe) => probe,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(true),
                Err(e) => return Err(e),
            };
            let name = probe.path().file_name().unwrap().to_str().unwrap();
            exists(std::ffi::OsStr::new(&name.replacen("Fc-case-", "fC-case-", 1)))
        }
        _ => Ok(false),
    }
}

fn check_links(
    root: &Path,
    selected: impl Fn(&Path) -> bool,
    owns_destination: impl Fn(&Path) -> bool,
    refusal: impl Fn(&Path, bool) -> String,
) -> Result<(), String> {
    let database = root.join("user_data.db");
    let inspect = || -> Result<(), Box<dyn std::error::Error>> {
        if !database.try_exists()? {
            return Ok(());
        }

        let db = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut query = db.prepare(
            "SELECT c.folder_name, ca.cluster_file_name, a.path
             FROM cluster_artifacts ca JOIN clusters c ON c.id = ca.cluster_id
             JOIN artifacts a ON a.hash = ca.hash WHERE a.content_type = 0",
        )?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let mods = root
                .join("clusters")
                .join(row.get_ref(0)?.as_str()?)
                .join("mods");
            if !selected(&mods) {
                continue;
            }
            let target = mods.join(row.get_ref(1)?.as_str()?);
            if owns_destination(&target) {
                return Err(refusal(&target, true).into());
            }
            let cached = root.join(row.get_ref(2)?.as_str()?);
            let bootstrap = cached_bootstrap(&cached, &target)?;
            if bootstrap {
                return Err(refusal(&target, false).into());
            }
        }
        Ok(())
    };
    inspect().map_err(|e| format!("OneClient database {}: {e}", database.display()))
}

fn cached_bootstrap(cached: &Path, target: &Path) -> Result<bool, String> {
    let identify = || ferrocraft_oneclient::is_bootstrap(File::open(cached)?);
    identify().map_err(|e| {
        format!(
            "cannot identify the mod at {} using {}: {e}",
            target.display(),
            cached.display()
        )
    })
}

pub fn check_uninstall(recorded: &[&Path], app_data: &Path) -> Result<(), String> {
    if recorded.is_empty() {
        return Ok(());
    }
    check_uninstall_databases(recorded, app_data)?;
    for jar in recorded {
        match fs::symlink_metadata(jar) {
            Ok(meta) if meta.is_symlink() => {
                return Err(format!(
                    "{}: OneClient may manage this link; uninstall stopped",
                    jar.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", jar.display())),
        }
    }
    Ok(())
}

fn check_uninstall_databases(recorded: &[&Path], app_data: &Path) -> Result<(), String> {
    let (active, previous) = oneclient::data_dirs(app_data, true)
        .map_err(|e| format!("OneClient data directory: {e}"))?;
    let roots: BTreeSet<_> = recorded
        .iter()
        .filter_map(|jar| data_root(jar.parent()?))
        .collect();
    let roots = std::iter::once(active.as_path())
        .chain(previous.as_deref().filter(|path| !same_dir(path, &active)))
        .chain(roots.into_iter().filter(|root| {
            !same_dir(root, &active) && !previous.as_ref().is_some_and(|p| same_dir(root, p))
        }));
    for root in roots {
        check_links(
            root,
            |_| true,
            |target| recorded.iter().any(|jar| same_destination(jar, target)),
            |target, exact| uninstall_refusal(root, &active, previous.as_deref(), target, exact),
        )?;
    }
    Ok(())
}

fn same_destination(jar: &Path, target: &Path) -> bool {
    jar.parent()
        .zip(jar.file_name())
        .is_some_and(|(dir, name)| matches_destination(dir, name, target))
}

fn matches_destination(dir: &Path, name: &std::ffi::OsStr, target: &Path) -> bool {
    target
        .file_name()
        .zip(target.parent())
        .is_some_and(|(other, parent)| {
            same_dir(dir, parent)
                && (name == other
                    || name.eq_ignore_ascii_case(other) && case_alias(dir, name, other).unwrap_or(true))
        })
}

fn uninstall_refusal(
    root: &Path,
    active: &Path,
    previous: Option<&Path>,
    target: &Path,
    exact: bool,
) -> String {
    let issue = if exact {
        "OneClient still manages"
    } else {
        "OneClient manages a bootstrap that is not at a recorded install path:"
    };
    let recovery = if same_dir(root, active) {
        "remove it in OneClient before uninstalling".to_owned()
    } else if previous.is_some_and(|p| same_dir(root, p)) {
        format!("clear {} in OneClient Storage before uninstalling", root.display())
    } else {
        format!("remove the mod from the OneClient installation that uses {}", root.display())
    };
    format!("{issue} {}; {recovery}", target.display())
}

pub fn check_reinstall(mods: &[PathBuf], recorded: &[&Path]) -> Result<(), String> {
    let profiles = || {
        mods.iter()
            .map(PathBuf::as_path)
            .chain(recorded.iter().filter_map(|jar| jar.parent()))
    };
    let roots: BTreeSet<_> = profiles().filter_map(data_root).collect();
    for root in roots {
        check_links(
            root,
            |dir| profiles().any(|profile| same_dir(profile, dir)),
            |target| mods.iter().any(|dir| matches_destination(dir, FABRIC_JAR.as_ref(), target)),
            |target, exact| {
                let kind = if exact { "mod at the install destination" } else { "bootstrap" };
                format!("{}: remove the existing {kind} in OneClient before reinstalling", target.display())
            },
        )?;
    }
    Ok(())
}
