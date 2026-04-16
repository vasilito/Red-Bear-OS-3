use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_BOND_STORE_ROOT: &str = "/var/lib/bluetooth";
pub const STUB_BOND_SOURCE: &str = "stub-cli";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondRecord {
    pub bond_id: String,
    pub alias: Option<String>,
    pub created_at_epoch: u64,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct BondStore {
    root: PathBuf,
}

impl BondStore {
    pub fn from_env() -> Self {
        Self {
            root: env::var_os("REDBEAR_BTCTL_BOND_STORE_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_BOND_STORE_ROOT)),
        }
    }

    #[cfg(test)]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn adapter_bonds_dir(&self, adapter: &str) -> PathBuf {
        debug_assert!(validate_adapter_name(adapter).is_ok());
        self.root.join(adapter).join("bonds")
    }

    pub fn load(&self, adapter: &str) -> io::Result<Vec<BondRecord>> {
        validate_adapter_name(adapter)?;
        let dir = self.adapter_bonds_dir(adapter);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut bonds = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            bonds.push(parse_record(&path)?);
        }
        bonds.sort_by(|left, right| left.bond_id.cmp(&right.bond_id));
        Ok(bonds)
    }

    pub fn add_stub(
        &self,
        adapter: &str,
        bond_id: &str,
        alias: Option<&str>,
    ) -> io::Result<BondRecord> {
        validate_adapter_name(adapter)?;
        validate_component("bond_id", bond_id)?;
        validate_optional_field("alias", alias)?;

        let record = BondRecord {
            bond_id: bond_id.to_string(),
            alias: alias.map(str::to_string),
            created_at_epoch: current_epoch_seconds(),
            source: STUB_BOND_SOURCE.to_string(),
        };

        let dir = self.adapter_bonds_dir(adapter);
        fs::create_dir_all(&dir)?;
        fs::write(
            self.record_path(adapter, bond_id),
            serialize_record(&record),
        )?;
        Ok(record)
    }

    pub fn remove(&self, adapter: &str, bond_id: &str) -> io::Result<bool> {
        validate_adapter_name(adapter)?;
        validate_component("bond_id", bond_id)?;

        let path = self.record_path(adapter, bond_id);
        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(path)?;
        Ok(true)
    }

    fn record_path(&self, adapter: &str, bond_id: &str) -> PathBuf {
        self.adapter_bonds_dir(adapter)
            .join(format!("{}.bond", hex_encode(bond_id.as_bytes())))
    }
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn validate_adapter_name(adapter: &str) -> io::Result<()> {
    validate_component("adapter", adapter)?;
    if adapter == "." || adapter == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "adapter cannot be a dot path segment",
        ));
    }
    Ok(())
}

fn validate_component(name: &str, value: &str) -> io::Result<()> {
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} cannot be empty"),
        ));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} cannot contain path separators"),
        ));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} cannot contain newlines"),
        ));
    }
    Ok(())
}

fn validate_optional_field(name: &str, value: Option<&str>) -> io::Result<()> {
    if let Some(value) = value {
        if value.contains('\n') || value.contains('\r') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} cannot contain newlines"),
            ));
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn serialize_record(record: &BondRecord) -> String {
    let mut lines = vec![format!("bond_id={}", record.bond_id)];
    if let Some(alias) = &record.alias {
        lines.push(format!("alias={alias}"));
    }
    lines.push(format!("created_at_epoch={}", record.created_at_epoch));
    lines.push(format!("source={}", record.source));
    format!("{}\n", lines.join("\n"))
}

fn parse_record(path: &Path) -> io::Result<BondRecord> {
    let content = fs::read_to_string(path)?;
    let mut bond_id = None;
    let mut alias = None;
    let mut created_at_epoch = None;
    let mut source = None;

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(value) = line.strip_prefix("bond_id=") {
            bond_id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("alias=") {
            alias = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("created_at_epoch=") {
            created_at_epoch = value.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("source=") {
            source = Some(value.to_string());
        }
    }

    let Some(bond_id) = bond_id else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing bond_id in {}", path.display()),
        ));
    };
    let Some(created_at_epoch) = created_at_epoch else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing created_at_epoch in {}", path.display()),
        ));
    };
    let Some(source) = source else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing source in {}", path.display()),
        ));
    };

    Ok(BondRecord {
        bond_id,
        alias,
        created_at_epoch,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{stamp}"))
    }

    #[test]
    fn stub_bond_records_round_trip_through_files() {
        let root = temp_path("rbos-bond-store");
        let store = BondStore::new(root.clone());

        let first = store
            .add_stub("hci0", "AA:BB:CC:DD:EE:FF", Some("demo-sensor"))
            .unwrap();
        let second = store.add_stub("hci0", "11:22:33:44:55:66", None).unwrap();

        let bonds = store.load("hci0").unwrap();
        assert_eq!(bonds.len(), 2);
        assert_eq!(bonds[0].bond_id, second.bond_id);
        assert_eq!(bonds[0].alias, None);
        assert_eq!(bonds[1].bond_id, first.bond_id);
        assert_eq!(bonds[1].alias.as_deref(), Some("demo-sensor"));
        assert!(store.remove("hci0", "AA:BB:CC:DD:EE:FF").unwrap());
        assert!(!store.remove("hci0", "AA:BB:CC:DD:EE:FF").unwrap());
        assert_eq!(store.load("hci0").unwrap().len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_components_are_rejected() {
        let root = temp_path("rbos-bond-store-invalid");
        let store = BondStore::new(root.clone());

        let err = store.add_stub("hci0", "demo/bond", None).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn load_rejects_invalid_adapter_component() {
        let root = temp_path("rbos-bond-store-invalid-load");
        let store = BondStore::new(root.clone());

        let err = store.load("../escape").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let err = store.load("..").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        fs::remove_dir_all(root).ok();
    }
}
