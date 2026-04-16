use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use log::{info, warn};
use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum BlobError {
    #[error("firmware directory not found: {0}")]
    DirNotFound(PathBuf),
    #[error("failed to read firmware directory: {0}")]
    DirReadError(PathBuf, #[source] std::io::Error),
    #[error("firmware not found: {0}")]
    FirmwareNotFound(PathBuf),
    #[error("failed to read firmware blob {path}: {source}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[allow(dead_code)]
pub struct FirmwareBlob {
    #[allow(dead_code)]
    pub name: String,
    pub path: PathBuf,
}

#[allow(dead_code)]
pub struct FirmwareRegistry {
    base_dir: PathBuf,
    blobs: HashMap<String, FirmwareBlob>,
    cache: Arc<Mutex<HashMap<String, Arc<Vec<u8>>>>>,
}

impl FirmwareRegistry {
    pub fn empty(base_dir: &Path) -> Self {
        FirmwareRegistry {
            base_dir: base_dir.to_path_buf(),
            blobs: HashMap::new(),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new(base_dir: &Path) -> Result<Self, BlobError> {
        if !base_dir.exists() {
            return Err(BlobError::DirNotFound(base_dir.to_path_buf()));
        }

        let blobs = discover_firmware(base_dir)?;
        info!(
            "firmware-loader: indexed {} firmware blob(s) from {}",
            blobs.len(),
            base_dir.display()
        );

        Ok(FirmwareRegistry {
            base_dir: base_dir.to_path_buf(),
            blobs,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    #[allow(dead_code)]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    #[allow(dead_code)]
    pub fn contains(&self, key: &str) -> bool {
        self.blobs.contains_key(key)
    }

    #[allow(dead_code)]
    pub fn load(&self, key: &str) -> Result<Arc<Vec<u8>>, BlobError> {
        {
            let cache = self.cache.lock().map_err(|e| BlobError::ReadError {
                path: self.base_dir.clone(),
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            })?;
            if let Some(data) = cache.get(key) {
                return Ok(Arc::clone(data));
            }
        }

        let blob = self.blobs.get(key).ok_or_else(|| {
            warn!("firmware-loader: requested firmware not found: {}", key);
            BlobError::FirmwareNotFound(self.base_dir.join(key))
        })?;

        let data = fs::read(&blob.path).map_err(|e| BlobError::ReadError {
            path: blob.path.clone(),
            source: e,
        })?;

        info!(
            "firmware-loader: loaded firmware blob {} ({} bytes) from {}",
            key,
            data.len(),
            blob.path.display()
        );

        let data = Arc::new(data);
        {
            let mut cache = self.cache.lock().map_err(|e| BlobError::ReadError {
                path: self.base_dir.clone(),
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            })?;
            cache.insert(key.to_string(), Arc::clone(&data));
        }

        Ok(data)
    }

    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    #[allow(dead_code)]
    pub fn list_keys(&self) -> Vec<&str> {
        self.blobs.keys().map(|s| s.as_str()).collect()
    }
}

fn discover_firmware(base_dir: &Path) -> Result<HashMap<String, FirmwareBlob>, BlobError> {
    let mut blobs = HashMap::new();
    let mut stack = vec![(base_dir.to_path_buf(), String::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| BlobError::DirReadError(dir.clone(), e))?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("firmware-loader: skipping unreadable dir entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();
            let file_name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };

            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    warn!("firmware-loader: skipping {}: {}", path.display(), e);
                    continue;
                }
            };

            if metadata.is_dir() {
                let new_prefix = if prefix.is_empty() {
                    file_name
                } else {
                    format!("{}/{}", prefix, file_name)
                };
                stack.push((path, new_prefix));
            } else if metadata.is_file() {
                if is_metadata_file(&file_name) {
                    continue;
                }

                let key = if prefix.is_empty() {
                    file_name.to_string()
                } else {
                    format!("{}/{}", prefix, file_name)
                };

                blobs.insert(key.clone(), FirmwareBlob { name: key, path });
            }
        }
    }

    Ok(blobs)
}

fn is_metadata_file(file_name: &str) -> bool {
    matches!(
        file_name,
        "WHENCE" | "README" | "README.md" | "check_whence.py" | "Makefile"
    ) || file_name.starts_with("LICENCE")
        || file_name.starts_with("LICENSE")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn discovers_ucode_pnvm_and_bin_but_skips_license_metadata() {
        let root = temp_root("rbos-fw-discover");
        fs::write(root.join("demo.bin"), []).unwrap();
        fs::write(root.join("iwlwifi-bz-b0-gf-a0-92.ucode"), []).unwrap();
        fs::write(root.join("iwlwifi-bz-b0-gf-a0.pnvm"), []).unwrap();
        fs::write(root.join("LICENCE.test"), "license").unwrap();
        fs::write(root.join("WHENCE"), "meta").unwrap();

        let blobs = discover_firmware(&root).unwrap();
        assert!(blobs.contains_key("demo.bin"));
        assert!(blobs.contains_key("iwlwifi-bz-b0-gf-a0-92.ucode"));
        assert!(blobs.contains_key("iwlwifi-bz-b0-gf-a0.pnvm"));
        assert!(!blobs.contains_key("LICENCE.test"));
        assert!(!blobs.contains_key("WHENCE"));

        fs::remove_dir_all(root).unwrap();
    }
}
