//! Versioned, per-device dashboard metadata. Never contains device state.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::{ControlId, DeviceSnapshot, PresentationKind};

pub const DASHBOARD_CONFIG_VERSION: u8 = 1;
pub const DASHBOARD_LIMIT: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub version: u8,
    pub device_id: String,
    pub capability_schema: String,
    pub controls: Vec<DashboardControl>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DashboardControl {
    pub id: ControlId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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
        }
    }

    pub fn validate_for(&self, snapshot: &DeviceSnapshot) -> io::Result<()> {
        if self.version != DASHBOARD_CONFIG_VERSION {
            return invalid("unknown dashboard config version");
        }
        if self.device_id != snapshot.device_id
            || self.capability_schema != snapshot.capability_schema
        {
            return invalid("dashboard binding does not match device");
        }
        if self.controls.len() > DASHBOARD_LIMIT {
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
            version: 1,
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            controls: vec![DashboardControl {
                id: ControlId("shown".into()),
                label: Some("KEF level".into()),
            }],
        };
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
    }
}
