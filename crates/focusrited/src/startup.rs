//! Startup configuration and profile loading.

use std::{io, path::PathBuf};

use crate::{
    Device, Service, ServiceError,
    dashboard_store::{DashboardConfig, DashboardStore},
    profile_store::{ProfileStore, Profiles},
};

pub const DEFAULT_PROFILE_STORE_PATH: &str = "/var/lib/focusrited/profiles";
pub const DEFAULT_DASHBOARD_STORE_PATH: &str = "/var/lib/focusrited/dashboard.json";
pub const DEFAULT_SOCKET_PATH: &str = "/run/focusrited/focusrited.sock";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub card: Option<String>,
    pub profile_store_path: PathBuf,
    pub dashboard_store_path: PathBuf,
    pub socket_path: PathBuf,
    pub dashboard_action: Option<DashboardAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DashboardAction {
    Inspect,
    Export(PathBuf),
    Import(PathBuf),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            card: None,
            profile_store_path: DEFAULT_PROFILE_STORE_PATH.into(),
            dashboard_store_path: DEFAULT_DASHBOARD_STORE_PATH.into(),
            socket_path: DEFAULT_SOCKET_PATH.into(),
            dashboard_action: None,
        }
    }
}

impl Config {
    /// Parses arguments after the program name.
    pub fn from_args(arguments: impl IntoIterator<Item = String>) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--card" => {
                    config.card = Some(arguments.next().ok_or(ConfigError::MissingCard)?);
                }
                "--profile-store" => {
                    config.profile_store_path = arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(ConfigError::MissingProfileStorePath)?;
                }
                "--socket" => {
                    config.socket_path = arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(ConfigError::MissingSocketPath)?;
                }
                "--dashboard-store" => {
                    config.dashboard_store_path = arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(ConfigError::MissingDashboardStorePath)?;
                }
                "--dashboard-inspect" => config.set_dashboard_action(DashboardAction::Inspect)?,
                "--dashboard-export" => config.set_dashboard_action(DashboardAction::Export(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(ConfigError::MissingDashboardExportPath)?,
                ))?,
                "--dashboard-import" => config.set_dashboard_action(DashboardAction::Import(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(ConfigError::MissingDashboardImportPath)?,
                ))?,
                "--help" | "-h" => return Err(ConfigError::Help),
                _ => return Err(ConfigError::UnknownArgument(argument)),
            }
        }
        Ok(config)
    }

    fn set_dashboard_action(&mut self, action: DashboardAction) -> Result<(), ConfigError> {
        if self.dashboard_action.replace(action).is_some() {
            return Err(ConfigError::MultipleDashboardActions);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    Help,
    MissingCard,
    MissingProfileStorePath,
    MissingDashboardStorePath,
    MissingDashboardExportPath,
    MissingDashboardImportPath,
    MultipleDashboardActions,
    MissingSocketPath,
    UnknownArgument(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Help => formatter
                .write_str("usage: focusrited --card CARD [--profile-store PATH] [--dashboard-store PATH] [--socket PATH] [--dashboard-inspect|--dashboard-export PATH|--dashboard-import PATH]"),
            Self::MissingCard => formatter.write_str("--card requires an ALSA card name"),
            Self::MissingProfileStorePath => formatter.write_str("--profile-store requires a path"),
            Self::MissingDashboardStorePath => formatter.write_str("--dashboard-store requires a path"),
            Self::MissingDashboardExportPath => formatter.write_str("--dashboard-export requires a path"),
            Self::MissingDashboardImportPath => formatter.write_str("--dashboard-import requires a path"),
            Self::MultipleDashboardActions => formatter.write_str("select only one dashboard action"),
            Self::MissingSocketPath => formatter.write_str("--socket requires a path"),
            Self::UnknownArgument(argument) => write!(formatter, "unknown argument: {argument}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug)]
pub enum StartupError {
    Device(ServiceError),
    ProfileStore(io::Error),
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Device(error) => write!(formatter, "device startup failed: {error:?}"),
            Self::ProfileStore(error) => write!(formatter, "profile store load failed: {error}"),
        }
    }
}

impl std::error::Error for StartupError {}

/// Connects device, then loads stored profiles without applying hardware state.
pub fn connect<D: Device>(device: D, config: &Config) -> Result<Service<D>, StartupError> {
    let mut service = Service::connect(device).map_err(StartupError::Device)?;
    service.set_profiles(load_profiles(config).map_err(StartupError::ProfileStore)?);
    Ok(service)
}

pub fn load_profiles(config: &Config) -> io::Result<Profiles> {
    ProfileStore::new(&config.profile_store_path).load()
}

pub fn load_dashboard(config: &Config) -> io::Result<Option<DashboardConfig>> {
    DashboardStore::new(&config.dashboard_store_path).load()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{ControlCapability, ControlId, DeviceError, DeviceSnapshot, Value, ValueDomain};

    #[test]
    fn profile_store_path_defaults_and_overrides() {
        assert_eq!(
            Config::from_args([]).unwrap().profile_store_path,
            PathBuf::from(DEFAULT_PROFILE_STORE_PATH)
        );
        let config = Config::from_args([
            "--card".into(),
            "Solo".into(),
            "--profile-store".into(),
            "/tmp/profiles".into(),
        ])
        .unwrap();
        assert_eq!(config.card.as_deref(), Some("Solo"));
        assert_eq!(config.profile_store_path, PathBuf::from("/tmp/profiles"));
        assert_eq!(config.socket_path, PathBuf::from(DEFAULT_SOCKET_PATH));
        assert_eq!(config.dashboard_action, None);
    }

    #[test]
    fn dashboard_actions_require_one_unambiguous_operation() {
        let export =
            Config::from_args(["--dashboard-export".into(), "/tmp/dashboard.json".into()]).unwrap();
        assert_eq!(
            export.dashboard_action,
            Some(DashboardAction::Export("/tmp/dashboard.json".into()))
        );
        assert_eq!(
            Config::from_args([
                "--dashboard-inspect".into(),
                "--dashboard-import".into(),
                "in.json".into()
            ]),
            Err(ConfigError::MultipleDashboardActions)
        );
    }

    struct MockDevice(DeviceSnapshot);

    impl Device for MockDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.0.clone())
        }

        fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
            self.0.values.insert(control.clone(), value);
            Ok(())
        }
    }

    fn device(value: i32) -> MockDevice {
        let control = ControlId("output.level".into());
        MockDevice(DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![ControlCapability {
                id: control.clone(),
                domain: ValueDomain::Integer,
                writable: true,
                available: true,
                minimum: Some(0),
                maximum: Some(100),
                group: None,
                presentation: None,
            }],
            values: BTreeMap::from([(control, Value::Integer(value))]),
        })
    }

    #[test]
    fn startup_loads_profiles_without_applying_them() {
        let path =
            std::env::temp_dir().join(format!("focusrited-startup-test-{}", std::process::id()));
        let config = Config {
            card: None,
            profile_store_path: path.clone(),
            dashboard_store_path: path.with_extension("dashboard.json"),
            socket_path: DEFAULT_SOCKET_PATH.into(),
            dashboard_action: None,
        };
        let store = ProfileStore::new(&path);
        let mut saved = Service::connect(device(50)).unwrap();
        saved.save_profile("desk".into()).unwrap();
        store.save_service(&saved).unwrap();

        let restored = connect(device(75), &config).unwrap();

        assert_eq!(
            restored.snapshot().values[&ControlId("output.level".into())],
            Value::Integer(75)
        );
        assert!(restored.profiles().contains_key("desk"));
        std::fs::remove_file(path).unwrap();
    }
}
