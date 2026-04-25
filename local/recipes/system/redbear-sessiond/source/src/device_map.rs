use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Clone, Debug)]
pub struct DeviceMap {
    static_paths: HashMap<(u32, u32), String>,
}

impl DeviceMap {
    #[cfg(test)]
    pub fn new() -> Self {
        let static_paths = HashMap::from([
            ((226, 0), String::from("/scheme/drm/card0")),
            ((226, 1), String::from("/scheme/drm/card1")),
            ((13, 64), String::from("/dev/input/event0")),
            ((13, 65), String::from("/dev/input/event1")),
            ((13, 66), String::from("/dev/input/event2")),
            ((13, 67), String::from("/dev/input/event3")),
            ((29, 0), String::from("/dev/fb0")),
            ((1, 1), String::from("/scheme/null")),
            ((1, 5), String::from("/scheme/zero")),
            ((1, 8), String::from("/scheme/rand")),
        ]);

        Self { static_paths }
    }

    /// Build a device map that merges static entries with dynamically discovered
    /// devices by scanning `/scheme/drm/card*` and `/dev/input/event*` at startup.
    /// For each discovered path, stat is used to read the rdev (device number).
    /// Entries with a nonzero rdev are inserted into the map; static entries are
    /// kept as fallback when rdev is unavailable or zero.
    pub fn discover() -> Self {
        let mut paths = HashMap::from([
            ((226, 0), String::from("/scheme/drm/card0")),
            ((226, 1), String::from("/scheme/drm/card1")),
            ((13, 64), String::from("/dev/input/event0")),
            ((13, 65), String::from("/dev/input/event1")),
            ((13, 66), String::from("/dev/input/event2")),
            ((13, 67), String::from("/dev/input/event3")),
            ((29, 0), String::from("/dev/fb0")),
            ((1, 1), String::from("/scheme/null")),
            ((1, 5), String::from("/scheme/zero")),
            ((1, 8), String::from("/scheme/rand")),
        ]);

        discover_scheme_drm(&mut paths);
        discover_dev_input(&mut paths);

        Self { static_paths: paths }
    }

    pub fn resolve(&self, major: u32, minor: u32) -> Option<String> {
        if let Some(path) = self.static_paths.get(&(major, minor)) {
            return Some(path.clone());
        }

        self.find_dynamic_path(major, minor)
            .or_else(|| self.fallback_path(major, minor))
    }

    pub fn open_device(&self, major: u32, minor: u32) -> io::Result<(String, File)> {
        let Some(path) = self.resolve(major, minor) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no Red Bear device mapping for major={major}, minor={minor}"),
            ));
        };

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .or_else(|_| OpenOptions::new().read(true).open(&path))
            .or_else(|_| OpenOptions::new().write(true).open(&path))?;

        Ok((path, file))
    }

    fn fallback_path(&self, major: u32, minor: u32) -> Option<String> {
        match (major, minor) {
            (13, minor) if minor >= 64 => {
                let path = format!("/dev/input/event{}", minor - 64);
                Path::new(&path).exists().then_some(path)
            }
            (226, minor) => {
                let path = format!("/scheme/drm/card{minor}");
                Path::new(&path).exists().then_some(path)
            }
            _ => None,
        }
    }

    fn find_dynamic_path(&self, major: u32, minor: u32) -> Option<String> {
        for path in candidate_paths() {
            if path_matches_device(&path, major, minor) {
                return Some(path.to_string_lossy().into_owned());
            }
        }

        None
    }
}

/// Scan `/scheme/drm/` for `card*` entries and merge any with a nonzero rdev
/// into the provided map. Static entries are not overwritten.
fn discover_scheme_drm(paths: &mut HashMap<(u32, u32), String>) {
    let entries = match fs::read_dir("/scheme/drm") {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("card") {
            continue;
        }

        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(&path) {
            let rdev = metadata.rdev();
            if rdev != 0 {
                let major = dev_major(rdev);
                let minor = dev_minor(rdev);
                paths
                    .entry((major, minor))
                    .or_insert_with(|| path.to_string_lossy().into_owned());
            }
        }

        #[cfg(not(unix))]
        let _ = &path;
    }
}

/// Scan `/dev/input/` for `event*` entries and merge any with a nonzero rdev
/// into the provided map. Static entries are not overwritten.
fn discover_dev_input(paths: &mut HashMap<(u32, u32), String>) {
    let entries = match fs::read_dir("/dev/input") {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }

        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(&path) {
            let rdev = metadata.rdev();
            if rdev != 0 {
                let major = dev_major(rdev);
                let minor = dev_minor(rdev);
                paths
                    .entry((major, minor))
                    .or_insert_with(|| path.to_string_lossy().into_owned());
            }
        }

        #[cfg(not(unix))]
        let _ = &path;
    }
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    paths.extend(read_dir_paths("/dev/input", |name| name.starts_with("event")));
    paths.extend(read_dir_paths("/scheme/drm", |name| name.starts_with("card")));

    for direct in ["/dev/fb0", "/scheme/null", "/scheme/zero", "/scheme/rand"] {
        let path = PathBuf::from(direct);
        if path.exists() {
            paths.push(path);
        }
    }

    paths
}

fn read_dir_paths(dir: &str, include: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return paths;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if include(name) {
            paths.push(path);
        }
    }

    paths.sort();
    paths
}

#[cfg(unix)]
fn path_matches_device(path: &Path, major: u32, minor: u32) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let rdev = metadata.rdev();
    dev_major(rdev) == major && dev_minor(rdev) == minor
}

#[cfg(not(unix))]
fn path_matches_device(_path: &Path, _major: u32, _minor: u32) -> bool {
    false
}

fn dev_major(device: u64) -> u32 {
    (((device >> 31 >> 1) & 0xfffff000) | ((device >> 8) & 0x00000fff)) as u32
}

fn dev_minor(device: u64) -> u32 {
    (((device >> 12) & 0xffffff00) | (device & 0x000000ff)) as u32
}

#[cfg(test)]
mod tests {
    use super::{dev_major, dev_minor};

    fn make_dev(major: u64, minor: u64) -> u64 {
        ((major & 0xfffff000) << 32)
            | ((major & 0x00000fff) << 8)
            | ((minor & 0xffffff00) << 12)
            | (minor & 0x000000ff)
    }

    #[test]
    fn splits_compound_dev_numbers() {
        let device = make_dev(226, 3);
        assert_eq!(dev_major(device), 226);
        assert_eq!(dev_minor(device), 3);

        let event = make_dev(13, 67);
        assert_eq!(dev_major(event), 13);
        assert_eq!(dev_minor(event), 67);
    }

    #[test]
    fn discover_returns_static_entries_when_no_dirs() {
        let map = super::DeviceMap::discover();
        assert!(map.resolve(226, 0).is_some());
        assert!(map.resolve(13, 64).is_some());
        assert!(map.resolve(29, 0).is_some());
    }
}
