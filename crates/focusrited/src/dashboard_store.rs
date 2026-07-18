//! Versioned, per-device dashboard metadata. Never contains device state.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::{ControlId, DeviceSnapshot, PresentationKind, groups::LevelGroup};

pub const DASHBOARD_CONFIG_VERSION: u8 = 2;
pub const DASHBOARD_LIMIT: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub version: u8,
    pub device_id: String,
    pub capability_schema: String,
    pub controls: Vec<DashboardControl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub level_groups: Vec<DashboardLevelGroup>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DashboardControl {
    pub id: ControlId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Persisted virtual level track. Its members are discovered leaf controls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DashboardLevelGroup {
    pub id: String,
    pub label: String,
    pub members: Vec<ControlId>,
    pub anchor: ControlId,
}

impl DashboardLevelGroup {
    pub fn level_group(&self) -> LevelGroup {
        LevelGroup {
            members: self.members.clone(),
            anchor: self.anchor.clone(),
        }
    }
}

impl DashboardConfig {
    /// Adapter defaults remain available until an owner saves a dashboard file.
    pub fn defaults(snapshot: &DeviceSnapshot) -> Self {
        let mut capabilities = snapshot
            .capabilities
            .iter()
            .filter_map(|capability| {
                capability.presentation.as_ref().and_then(|presentation| {
                    (presentation.kind == PresentationKind::Level)
                        .then_some(presentation.default_dashboard_order)
                        .flatten()
                        .map(|order| (order, capability.id.clone()))
                })
            })
            .collect::<Vec<_>>();
        capabilities.sort();
        Self {
            version: DASHBOARD_CONFIG_VERSION,
            device_id: snapshot.device_id.clone(),
            capability_schema: snapshot.capability_schema.clone(),
            controls: capabilities
                .into_iter()
                .take(DASHBOARD_LIMIT)
                .map(|(_, id)| DashboardControl { id, label: None })
                .collect(),
            level_groups: Vec::new(),
        }
    }

    pub fn validate_for(&self, snapshot: &DeviceSnapshot) -> io::Result<()> {
        if self.version != 1 && self.version != DASHBOARD_CONFIG_VERSION {
            return invalid("unknown dashboard config version");
        }
        if self.version == 1 && !self.level_groups.is_empty() {
            return invalid("dashboard groups require config version 2");
        }
        if self.device_id != snapshot.device_id
            || self.capability_schema != snapshot.capability_schema
        {
            return invalid("dashboard binding does not match device");
        }
        if self.controls.len() + self.level_groups.len() > DASHBOARD_LIMIT {
            return invalid("dashboard exceeds control limit");
        }
        let mut ids = BTreeSet::new();
        for control in &self.controls {
            if control
                .label
                .as_ref()
                .is_some_and(|label| label.trim().is_empty())
            {
                return invalid("dashboard label is empty");
            }
            if !ids.insert(&control.id) {
                return invalid("dashboard repeats control id");
            }
            if !snapshot
                .capabilities
                .iter()
                .any(|capability| capability.id == control.id && capability.presentation.is_some())
            {
                return invalid("dashboard control is unavailable");
            }
        }
        let mut group_ids = BTreeSet::new();
        for group in &self.level_groups {
            if group.id.trim().is_empty() || group.label.trim().is_empty() {
                return invalid("dashboard group id or label is empty");
            }
            if !group_ids.insert(&group.id) {
                return invalid("dashboard repeats group id");
            }
            group
                .level_group()
                .validate(&snapshot.capabilities)
                .map_err(|error| invalid_error(format!("dashboard group is invalid: {error:?}")))?;
        }
        Ok(())
    }
}

pub struct DashboardStore {
    path: PathBuf,
}

impl DashboardStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Missing file deliberately selects adapter defaults rather than writing one.
    pub fn load(&self) -> io::Result<Option<DashboardConfig>> {
        match fs::read(&self.path) {
            Ok(contents) => serde_json::from_slice(&contents)
                .map(Some)
                .map_err(|error| invalid_error(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn load_required(&self) -> io::Result<DashboardConfig> {
        self.load()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "dashboard configuration does not exist",
            )
        })
    }

    /// Writes complete configuration through a same-directory temporary file.
    pub fn save(&self, config: &DashboardConfig, snapshot: &DeviceSnapshot) -> io::Result<()> {
        config.validate_for(snapshot)?;
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        let mut file = File::create(&temporary)?;
        serde_json::to_writer_pretty(&mut file, config).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        fs::rename(temporary, &self.path)
    }
}

fn invalid<T>(message: &str) -> io::Result<T> {
    Err(invalid_error(message))
}

fn invalid_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{ControlCapability, ControlPresentation, PresentationKind, Value, ValueDomain};

    fn snapshot() -> DeviceSnapshot {
        let shown = ControlId("shown".into());
        DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![
                ControlCapability {
                    id: shown.clone(),
                    domain: ValueDomain::Integer,
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                    group: Some(crate::GroupCapability {
                        operation: crate::GroupOperation::RelativeLevel,
                    }),
                    presentation: Some(ControlPresentation {
                        label: "Shown".into(),
                        kind: PresentationKind::Level,
                        default_dashboard_order: Some(1),
                        companion: None,
                        step: Some(1),
                    }),
                },
                ControlCapability {
                    id: ControlId("raw".into()),
                    domain: ValueDomain::Integer,
                    writable: false,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(184),
                    group: None,
                    presentation: None,
                },
            ],
            values: BTreeMap::from([(shown, Value::Integer(50))]),
        }
    }

    #[test]
    fn round_trip_is_bound_and_atomic() {
        let directory =
            std::env::temp_dir().join(format!("focusrited-dashboard-test-{}", std::process::id()));
        let path = directory.join("dashboard.json");
        let store = DashboardStore::new(&path);
        let snapshot = snapshot();
        let config = DashboardConfig {
            version: 2,
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            controls: vec![DashboardControl {
                id: ControlId("shown".into()),
                label: Some("KEF level".into()),
            }],
            level_groups: vec![DashboardLevelGroup {
                id: "linked-outputs".into(),
                label: "KEF".into(),
                members: vec![ControlId("shown".into()), ControlId("other".into())],
                anchor: ControlId("shown".into()),
            }],
        };
        let mut snapshot = snapshot;
        snapshot.capabilities.push(ControlCapability {
            id: ControlId("other".into()),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(0),
            maximum: Some(100),
            group: Some(crate::GroupCapability {
                operation: crate::GroupOperation::RelativeLevel,
            }),
            presentation: None,
        });
        snapshot
            .values
            .insert(ControlId("other".into()), Value::Integer(50));
        store.save(&config, &snapshot).unwrap();
        assert_eq!(store.load().unwrap(), Some(config));
        assert!(!path.with_extension("tmp").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_invalid_or_unavailable_controls() {
        let snapshot = snapshot();
        let mut config = DashboardConfig::defaults(&snapshot);
        config.controls.push(DashboardControl {
            id: ControlId("raw".into()),
            label: None,
        });
        assert_eq!(
            config.validate_for(&snapshot).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        config.controls = vec![DashboardControl {
            id: ControlId("shown".into()),
            label: None,
        }];
        config.device_id = "other-device".into();
        assert_eq!(
            config.validate_for(&snapshot).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        config.device_id = "mock-device".into();
        config.level_groups = vec![DashboardLevelGroup {
            id: "bad".into(),
            label: "Bad".into(),
            members: vec![ControlId("shown".into()), ControlId("raw".into())],
            anchor: ControlId("shown".into()),
        }];
        assert_eq!(
            config.validate_for(&snapshot).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn version_one_without_groups_remains_valid() {
        let config: DashboardConfig = serde_json::from_str(
            r#"{
                "version": 1,
                "device_id": "mock-device",
                "capability_schema": "mock-v1",
                "controls": [{"id":"shown"}]
            }"#,
        )
        .unwrap();
        assert!(config.level_groups.is_empty());
        config.validate_for(&snapshot()).unwrap();
    }
}
