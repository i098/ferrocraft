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

#[cfg(target_os = "macos")]
fn ignores_case(dir: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    CString::new(dir.as_os_str().as_bytes()).map_or(true, |path| unsafe {
        libc::pathconf(path.as_ptr(), libc::_PC_CASE_SENSITIVE) != 1
    })
}

#[cfg(not(target_os = "macos"))]
fn ignores_case(_: &Path) -> bool {
    cfg!(windows)
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
            if ferrocraft_oneclient::is_bootstrap(File::open(&cached)?) {
                return Err(refusal(&target, false).into());
            }
        }
        Ok(())
    };
    inspect().map_err(|e| format!("OneClient database {}: {e}", database.display()))
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
            (name == other || name.eq_ignore_ascii_case(other) && ignores_case(dir))
                && same_dir(dir, parent)
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

pub fn check_reinstall(mods: &[PathBuf]) -> Result<(), String> {
    let roots: BTreeSet<_> = mods.iter().filter_map(|dir| data_root(dir)).collect();
    for root in roots {
        check_links(
            root,
            |dir| mods.iter().any(|m| same_dir(m, dir)),
            |target| mods.iter().any(|dir| matches_destination(dir, FABRIC_JAR.as_ref(), target)),
            |target, exact| {
                let kind = if exact { "mod at the install destination" } else { "bootstrap" };
                format!("{}: remove the existing {kind} in OneClient before reinstalling", target.display())
            },
        )?;
    }
    Ok(())
}
