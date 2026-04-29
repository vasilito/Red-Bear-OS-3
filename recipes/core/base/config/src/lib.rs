use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

pub fn config(name: &str) -> Result<Vec<PathBuf>, io::Error> {
    config_for_dirs(&[
        &Path::new("/usr/lib").join(format!("{name}.d")),
        &Path::new("/etc").join(format!("{name}.d")),
    ])
}

pub fn config_for_initfs(name: &str) -> Result<Vec<PathBuf>, io::Error> {
    config_for_dirs(&[
        &Path::new("/scheme/initfs/lib").join(format!("{name}.d")),
        &Path::new("/scheme/initfs/etc").join(format!("{name}.d")),
    ])
}

pub fn config_for_dirs(dirs: &[impl AsRef<Path>]) -> Result<Vec<PathBuf>, io::Error> {
    // This must be a BTreeMap to iterate in sorted order.
    let mut entries = BTreeMap::new();

    for dir in dirs {
        let dir = dir.as_ref();
        if !dir.exists() {
            // Skip non-existent dirs
            continue;
        }

        for entry_res in fs::read_dir(&dir)? {
            // This intentionally overwrites older entries with
            // the same filename to allow overriding entries in
            // one search dir with those in a later search dir.
            let entry = entry_res?;
            entries.insert(entry.file_name(), entry.path());
        }
    }

    Ok(entries.into_values().collect())
}
