use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read};
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

fn check_links(
    root: &Path,
    selected: impl Fn(&Path) -> bool,
    check: impl Fn(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let database = root.join("user_data.db");
    let inspect = || -> Result<(), Box<dyn std::error::Error>> {
        if !database.try_exists()? {
            return Ok(());
        }

        let db = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut query = db.prepare(
            "SELECT c.folder_name, ca.cluster_file_name, a.file_name, a.path
             FROM cluster_artifacts ca JOIN clusters c ON c.id = ca.cluster_id
             JOIN artifacts a ON a.hash = ca.hash WHERE a.content_type = 0",
        )?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let name = row.get_ref(1)?.as_str()?;
            let target = root
                .join("clusters")
                .join(row.get_ref(0)?.as_str()?)
                .join("mods")
                .join(name);
            if !selected(&target) {
                continue;
            }
            let cached = root.join(row.get_ref(3)?.as_str()?);
            if name.eq_ignore_ascii_case(FABRIC_JAR)
                || row.get_ref(2)?.as_str()?.eq_ignore_ascii_case(FABRIC_JAR)
                || ferrocraft_oneclient::is_bootstrap(File::open(&cached)?)
            {
                check(&target, &cached)?;
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
    let mut roots: BTreeSet<_> = recorded
        .iter()
        .filter_map(|jar| data_root(jar.parent()?))
        .collect();
    roots.insert(&active);
    for root in roots {
        check_links(
            root,
            |_| true,
            |target, _| {
                let location = if recorded.contains(&target) {
                    "is still managed"
                } else {
                    "is not at a recorded install path"
                };
                Err(format!(
                    "bootstrap {} {location}; remove it in OneClient before uninstalling",
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
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", jar.display())),
        }
    }
    Ok(())
}

pub fn check_reinstall(mods: &[PathBuf], incoming: &Path) -> Result<(), String> {
    let roots: BTreeSet<_> = mods.iter().filter_map(|dir| data_root(dir)).collect();
    for root in roots {
        let selected = |target: &Path| {
            target.parent().is_some_and(|dir| mods.iter().any(|m| m == dir))
        };
        check_links(root, selected, |target, cached| {
            let cluster = target.parent().and_then(Path::parent).unwrap();
            let fail = || {
                format!(
                    "{}: remove the existing bootstrap in OneClient before reinstalling",
                    target.display()
                )
            };
            if target.file_name().is_none_or(|name| name != FABRIC_JAR)
                || !cluster.join(".oneclient-mods.json").is_file()
                    && !same_file_contents(cached, incoming).map_err(|e| e.to_string())?
            {
                return Err(fail());
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn same_file_contents(a: &Path, b: &Path) -> io::Result<bool> {
    let mut a = File::open(a)?;
    let mut b = File::open(b)?;
    let mut left = a.metadata()?.len();
    if left != b.metadata()?.len() {
        return Ok(false);
    }
    let (mut x, mut y) = ([0; 8192], [0; 8192]);
    while left != 0 {
        let n = left.min(x.len() as u64) as usize;
        a.read_exact(&mut x[..n])?;
        b.read_exact(&mut y[..n])?;
        if x[..n] != y[..n] {
            return Ok(false);
        }
        left -= n as u64;
    }
    Ok(true)
}
