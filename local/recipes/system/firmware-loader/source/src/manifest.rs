use std::fs;
use std::io::{Error, ErrorKind};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::blob::{discover_firmware, BlobError};

pub fn generate_manifest(firmware_dir: &str) -> Result<(), std::io::Error> {
    let base_dir = Path::new(firmware_dir);
    let blobs = discover_firmware(base_dir).map_err(blob_error_to_io)?;

    let mut keys: Vec<String> = blobs.keys().cloned().collect();
    keys.sort_unstable();

    let mut manifest = String::new();
    for key in keys {
        let path = base_dir.join(&key);
        let bytes = fs::read(&path)?;
        let digest = Sha256::digest(&bytes);
        manifest.push_str(&encode_hex(&digest));
        manifest.push_str("  ");
        manifest.push_str(&bytes.len().to_string());
        manifest.push_str("  ");
        manifest.push_str(&key);
        manifest.push('\n');
    }

    fs::write(base_dir.join("MANIFEST.txt"), manifest)
}

fn blob_error_to_io(err: BlobError) -> std::io::Error {
    match err {
        BlobError::DirNotFound(path) => Error::new(
            ErrorKind::NotFound,
            format!("firmware directory not found: {}", path.display()),
        ),
        BlobError::DirReadError(_, source) | BlobError::ReadError { source, .. } => source,
        BlobError::FirmwareNotFound(path) => Error::new(
            ErrorKind::NotFound,
            format!("firmware not found: {}", path.display()),
        ),
        BlobError::LoadTimeout { key, timeout } => Error::new(
            ErrorKind::TimedOut,
            format!("firmware load timed out for {key} after {timeout:?}"),
        ),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::generate_manifest;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(err) => panic!("system clock error while creating temp path: {err}"),
        };
        let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
        if let Err(err) = fs::create_dir_all(&path) {
            panic!("failed to create temp directory {}: {err}", path.display());
        }
        path
    }

    #[test]
    fn generates_sha256_size_manifest_for_firmware_blobs_only() {
        let root = temp_root("rbos-fw-manifest");
        let intel_dir = root.join("intel");
        if let Err(err) = fs::create_dir_all(&intel_dir) {
            panic!("failed to create nested firmware directory: {err}");
        }
        if let Err(err) = fs::write(root.join("iwlwifi-test.ucode"), [1u8, 2, 3]) {
            panic!("failed to write ucode blob: {err}");
        }
        if let Err(err) = fs::write(intel_dir.join("ibt-test.sfi"), [4u8, 5, 6, 7]) {
            panic!("failed to write bluetooth blob: {err}");
        }
        if let Err(err) = fs::write(root.join("README"), "metadata") {
            panic!("failed to write metadata file: {err}");
        }

        let root_str = root.to_string_lossy().into_owned();
        if let Err(err) = generate_manifest(&root_str) {
            panic!("failed to generate manifest: {err}");
        }

        let manifest_path = root.join("MANIFEST.txt");
        let manifest = match fs::read_to_string(&manifest_path) {
            Ok(manifest) => manifest,
            Err(err) => panic!("failed to read generated manifest: {err}"),
        };

        assert!(manifest.contains("  3  iwlwifi-test.ucode\n"));
        assert!(manifest.contains("  4  intel/ibt-test.sfi\n"));
        assert!(!manifest.contains("README"));

        if let Err(err) = fs::remove_dir_all(&root) {
            panic!("failed to remove temp directory {}: {err}", root.display());
        }
    }
}
