use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::oneclient;

fn data_root(mods: &Path) -> Option<&Path> {
    let clusters = mods.parent()?.parent()?;
    if mods.ends_with("mods") && clusters.ends_with("clusters") {
        clusters.parent()
    } else {
        None
    }
}

fn check_links(
    root: &Path,
    selected: impl Fn(&Path) -> bool,
    check: impl Fn(&Path) -> Result<(), String>,
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
            let cached = root.join(row.get_ref(2)?.as_str()?);
            if ferrocraft_oneclient::is_bootstrap(File::open(&cached)?) {
                check(&mods.join(row.get_ref(1)?.as_str()?))?;
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
    let active = oneclient::data_dir(app_data).map_err(|e| format!("OneClient data directory: {e}"))?;
    let roots: BTreeSet<_> = recorded
        .iter()
        .filter_map(|jar| data_root(jar.parent()?))
        .collect();
    for root in std::iter::once(active.as_path()).chain(roots.into_iter().filter(|r| *r != active)) {
        check_links(
            root,
            |_| true,
            |target| {
                let location = if recorded.contains(&target) {
                    "is still managed"
                } else {
                    "is not at a recorded install path"
                };
                let recovery = if root == active {
                    "remove it in OneClient before uninstalling".to_owned()
                } else {
                    format!("clear {} in OneClient Storage before uninstalling", root.display())
                };
                Err(format!(
                    "bootstrap {} {location}; {recovery}",
                    target.display()
                ))
            },
        )?;
    }
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

pub fn check_reinstall(mods: &[PathBuf]) -> Result<(), String> {
    let roots: BTreeSet<_> = mods.iter().filter_map(|dir| data_root(dir)).collect();
    for root in roots {
        check_links(root, |dir| mods.iter().any(|m| m == dir), |target| {
            Err(format!(
                "{}: remove the existing bootstrap in OneClient before reinstalling",
                target.display()
            ))
        })?;
    }
    Ok(())
}
