use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Deserialize;

use crate::script::{Command, Script};
use crate::service::Service;

pub struct UnitStore {
    pub config_dirs: Vec<PathBuf>,
    units: BTreeMap<UnitId, Unit>,
    runtime_target: Option<UnitId>,
}

impl UnitStore {
    pub fn new() -> Self {
        UnitStore {
            config_dirs: vec![],
            units: BTreeMap::new(),
            runtime_target: None,
        }
    }

    pub fn set_runtime_target(&mut self, unit_id: UnitId) {
        assert!(self.runtime_target.is_none());
        assert!(self.units.contains_key(&unit_id));
        self.runtime_target = Some(unit_id);
    }

    fn load_single_unit(&mut self, unit_id: UnitId, errors: &mut Vec<String>) -> Option<UnitId> {
        let (filename, instance) = if let Some((base_service, rest)) = unit_id.0.split_once('@') {
            let Some((instance, ext)) = rest.rsplit_once('.') else {
                errors.push(format!("script {} can't be instanced", unit_id.0));
                return None;
            };
            (format!("{base_service}@.{ext}"), Some(instance))
        } else {
            (unit_id.0.clone(), None)
        };

        let Some(path) = self
            .config_dirs
            .iter()
            .rev()
            .map(|dir| dir.join(&filename))
            .find(|path| path.exists())
        else {
            errors.push(format!("unit {} not found", unit_id.0));
            return None;
        };

        let mut unit = match Unit::from_file(unit_id.clone(), &path, instance, errors) {
            Ok(unit) => unit,
            Err(err) => {
                errors.push(format!("{}: {err}", path.display()));
                return None;
            }
        };

        if unit.info.default_dependencies {
            if let Some(runtime_target) = self.runtime_target.clone() {
                unit.info.requires_weak.push(runtime_target);
            } else {
                errors.push(format!(
                    "{}: dependency of the runtime target must have default dependencies disabled",
                    path.display(),
                ));
            }
        }

        self.units.insert(unit_id.clone(), unit);

        Some(unit_id)
    }

    pub fn load_units(&mut self, root_unit: UnitId, errors: &mut Vec<String>) -> Vec<UnitId> {
        let mut loaded_units = vec![];
        let mut pending_units = vec![root_unit];

        while let Some(unit_id) = pending_units.pop() {
            if self.units.contains_key(&unit_id) {
                continue;
            }
            let unit = self.load_single_unit(unit_id, errors);
            if let Some(unit) = unit {
                loaded_units.push(unit.clone());
                for dep in &self.unit(&unit).info.requires_weak {
                    pending_units.push(dep.clone());
                }
            }
        }

        loaded_units
    }

    pub fn unit(&self, unit: &UnitId) -> &Unit {
        self.units.get(unit).unwrap()
    }

    pub fn unit_mut(&mut self, unit: &UnitId) -> &mut Unit {
        self.units.get_mut(unit).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(transparent)]
pub struct UnitId(pub String);

pub struct Unit {
    pub id: UnitId,

    pub info: UnitInfo,
    pub kind: UnitKind,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitInfo {
    pub description: Option<String>,
    #[serde(default = "true_bool")]
    pub default_dependencies: bool,
    #[serde(default)]
    pub requires_weak: Vec<UnitId>,
    pub condition_architecture: Option<Vec<String>>,
    // FIXME replace this with hwd reading from the devicetree
    pub condition_board: Option<Vec<String>>,
}

fn true_bool() -> bool {
    true
}

pub enum UnitKind {
    LegacyScript { script: Vec<Command> },
    Service { service: Service },
    Target {},
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SerializedService {
    unit: UnitInfo,
    service: Service,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SerializedTarget {
    unit: UnitInfo,
}

fn instance_toml(value: toml::Value, instance: &str) -> toml::Value {
    match value {
        toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => value,
        toml::Value::String(s) => toml::Value::String(s.replace("$INSTANCE", instance)),
        toml::Value::Array(values) => toml::Value::Array(
            values
                .into_iter()
                .map(|value| instance_toml(value, instance))
                .collect(),
        ),
        toml::Value::Table(map) => toml::Value::Table(
            map.into_iter()
                .map(|(key, value)| (key, instance_toml(value, instance)))
                .collect(),
        ),
    }
}

impl Unit {
    pub fn from_file(
        id: UnitId,
        config_path: &Path,
        instance: Option<&str>,
        errors: &mut Vec<String>,
    ) -> io::Result<Self> {
        let config = fs::read_to_string(config_path)?;

        let Some(ext) = config_path.extension().map(|ext| ext.to_str().unwrap()) else {
            let script = Script::from_str(&config, errors)?;
            return Ok(Unit {
                id,
                info: UnitInfo {
                    description: None,
                    default_dependencies: true,
                    requires_weak: script.1,
                    condition_architecture: None,
                    condition_board: None,
                },
                kind: UnitKind::LegacyScript { script: script.0 },
            });
        };

        let toml_value: toml::Value = toml::from_str(&config)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let toml_value = if let Some(instance) = instance {
            instance_toml(toml_value, instance)
        } else {
            toml_value
        };

        let (info, kind) = match ext {
            "service" => {
                let service: SerializedService = toml_value
                    .try_into()
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                (
                    service.unit,
                    UnitKind::Service {
                        service: service.service,
                    },
                )
            }
            "target" => {
                let target: SerializedTarget = toml_value
                    .try_into()
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                (target.unit, UnitKind::Target {})
            }
            _ => return Err(io::Error::other("invalid file extension")),
        };

        Ok(Unit { id, info, kind })
    }

    pub fn conditions_met(&self) -> bool {
        if let Some(condition_architecture) = &self.info.condition_architecture {
            if !condition_architecture
                .iter()
                .any(|arch| arch == std::env::consts::ARCH)
            {
                return false;
            }
        }

        if let Some(condition_board) = &self.info.condition_board {
            if !condition_board
                .iter()
                .any(|board| Some(&**board) == option_env!("BOARD"))
            {
                return false;
            }
        }

        true
    }
}
