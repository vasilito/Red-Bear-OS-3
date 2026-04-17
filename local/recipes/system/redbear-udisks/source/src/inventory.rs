use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use zbus::zvariant::OwnedObjectPath;

pub const ROOT_PATH: &str = "/org/freedesktop/UDisks2";
pub const MANAGER_PATH: &str = "/org/freedesktop/UDisks2/Manager";
pub const BLOCK_DEVICES_PREFIX: &str = "/org/freedesktop/UDisks2/block_devices";
pub const DRIVES_PREFIX: &str = "/org/freedesktop/UDisks2/drives";

#[derive(Clone, Debug)]
pub struct Inventory {
    manager_path: OwnedObjectPath,
    drives: Vec<DriveDevice>,
    blocks: Vec<BlockDevice>,
}

#[derive(Clone, Debug)]
pub struct DriveDevice {
    pub object_path: OwnedObjectPath,
    pub scheme_identity: String,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct BlockDevice {
    pub object_path: OwnedObjectPath,
    pub drive_object_path: OwnedObjectPath,
    pub device_path: String,
    pub size: u64,
    // UDisks2's base Drive/Block interfaces do not expose logical block size directly,
    // but Red Bear still derives and retains it from real file metadata.
    pub logical_block_size: u64,
    pub read_only: bool,
    pub hint_partitionable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RootKey {
    disk_number: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct PartitionKey {
    disk_number: u32,
    partition_number: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    Root(RootKey),
    Partition(PartitionKey),
}

#[derive(Clone, Copy, Debug)]
struct DeviceMetadata {
    size: u64,
    logical_block_size: u64,
    read_only: bool,
}

impl Inventory {
    pub fn scan() -> Self {
        let mut drives = Vec::new();
        let mut blocks = Vec::new();

        for scheme_name in read_dir_names("/scheme")
            .unwrap_or_default()
            .into_iter()
            .filter(|name| name.starts_with("disk."))
        {
            let scheme_path = PathBuf::from("/scheme").join(&scheme_name);
            let scheme_identity = scheme_name
                .strip_prefix("disk.")
                .unwrap_or(&scheme_name)
                .to_string();
            let entries = read_dir_names(&scheme_path).unwrap_or_default();

            let mut roots = BTreeMap::new();
            let mut partitions = Vec::new();

            for entry_name in entries {
                match parse_entry_name(&entry_name) {
                    Some(EntryKind::Root(root_key)) => {
                        roots.insert(root_key, entry_name);
                    }
                    Some(EntryKind::Partition(partition_key)) => {
                        partitions.push((partition_key, entry_name));
                    }
                    None => {}
                }
            }

            let mut drive_paths = BTreeMap::new();

            for (root_key, entry_name) in roots {
                let device_path = format!("{}/{entry_name}", scheme_path.display());
                let Some(metadata) = read_device_metadata(Path::new(&device_path)) else {
                    continue;
                };

                let drive = DriveDevice {
                    object_path: owned_object_path(&format!(
                        "{DRIVES_PREFIX}/{}",
                        stable_object_name(&scheme_name, &entry_name)
                    )),
                    scheme_identity: scheme_identity.clone(),
                    size: metadata.size,
                };

                drive_paths.insert(root_key, drive.object_path.clone());

                blocks.push(BlockDevice {
                    object_path: owned_object_path(&format!(
                        "{BLOCK_DEVICES_PREFIX}/{}",
                        stable_object_name(&scheme_name, &entry_name)
                    )),
                    drive_object_path: drive.object_path.clone(),
                    device_path,
                    size: metadata.size,
                    logical_block_size: metadata.logical_block_size,
                    read_only: metadata.read_only,
                    hint_partitionable: true,
                });

                drives.push(drive);
            }

            partitions.sort_by_key(|(partition_key, _)| *partition_key);
            for (partition_key, entry_name) in partitions {
                let Some(drive_object_path) = drive_paths.get(&RootKey {
                    disk_number: partition_key.disk_number,
                }) else {
                    continue;
                };

                let device_path = format!("{}/{entry_name}", scheme_path.display());
                let Some(metadata) = read_device_metadata(Path::new(&device_path)) else {
                    continue;
                };

                blocks.push(BlockDevice {
                    object_path: owned_object_path(&format!(
                        "{BLOCK_DEVICES_PREFIX}/{}",
                        stable_object_name(&scheme_name, &entry_name)
                    )),
                    drive_object_path: drive_object_path.clone(),
                    device_path,
                    size: metadata.size,
                    logical_block_size: metadata.logical_block_size,
                    read_only: metadata.read_only,
                    hint_partitionable: false,
                });
            }
        }

        Self {
            manager_path: owned_object_path(MANAGER_PATH),
            drives,
            blocks,
        }
    }

    pub fn manager_path(&self) -> OwnedObjectPath {
        self.manager_path.clone()
    }

    pub fn drives(&self) -> &[DriveDevice] {
        &self.drives
    }

    pub fn blocks(&self) -> &[BlockDevice] {
        &self.blocks
    }

    pub fn drive_paths(&self) -> Vec<OwnedObjectPath> {
        self.drives
            .iter()
            .map(|drive| drive.object_path.clone())
            .collect()
    }

    pub fn block_paths(&self) -> Vec<OwnedObjectPath> {
        self.blocks
            .iter()
            .map(|block| block.object_path.clone())
            .collect()
    }
}

fn read_dir_names(path: impl AsRef<Path>) -> Option<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_str()?.to_string();
        names.push(name);
    }
    names.sort();
    Some(names)
}

fn parse_entry_name(entry_name: &str) -> Option<EntryKind> {
    if let Some(position) = entry_name.find('p') {
        let disk_number = entry_name[..position].parse().ok()?;
        let partition_number = entry_name[position + 1..].parse().ok()?;
        return Some(EntryKind::Partition(PartitionKey {
            disk_number,
            partition_number,
        }));
    }

    Some(EntryKind::Root(RootKey {
        disk_number: entry_name.parse().ok()?,
    }))
}

fn read_device_metadata(path: &Path) -> Option<DeviceMetadata> {
    let metadata = fs::metadata(path).ok()?;
    let logical_block_size = metadata_logical_block_size(&metadata);

    Some(DeviceMetadata {
        size: metadata.len(),
        logical_block_size,
        read_only: metadata.permissions().readonly(),
    })
}

#[cfg(unix)]
fn metadata_logical_block_size(metadata: &fs::Metadata) -> u64 {
    metadata.blksize()
}

#[cfg(not(unix))]
fn metadata_logical_block_size(_metadata: &fs::Metadata) -> u64 {
    0
}

fn stable_object_name(scheme_name: &str, entry_name: &str) -> String {
    format!(
        "{}_{}",
        encode_path_component(scheme_name),
        encode_path_component(entry_name)
    )
}

fn encode_path_component(component: &str) -> String {
    let mut encoded = String::new();

    for byte in component.bytes() {
        if byte.is_ascii_alphanumeric() {
            encoded.push(byte as char);
        } else {
            encoded.push('_');
            encoded.push(hex_char(byte >> 4));
            encoded.push(hex_char(byte & 0x0f));
        }
    }

    if encoded.is_empty() {
        encoded.push('_');
        encoded.push('0');
        encoded.push('0');
    }

    encoded
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("hex nibble out of range"),
    }
}

fn owned_object_path(path: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path.to_string()).expect("generated object path must be valid")
}
