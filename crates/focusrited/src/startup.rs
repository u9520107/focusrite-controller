//! Startup configuration and profile loading.

use std::{io, path::PathBuf};

use crate::{Device, Service, ServiceError, profile_store::ProfileStore};

pub const DEFAULT_PROFILE_STORE_PATH: &str = "/var/lib/focusrited/profiles";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub profile_store_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile_store_path: DEFAULT_PROFILE_STORE_PATH.into(),
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
                "--profile-store" => {
                    config.profile_store_path = arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or(ConfigError::MissingProfileStorePath)?;
                }
                "--help" | "-h" => return Err(ConfigError::Help),
                _ => return Err(ConfigError::UnknownArgument(argument)),
            }
        }
        Ok(config)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    Help,
    MissingProfileStorePath,
    UnknownArgument(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Help => formatter.write_str("usage: focusrited [--profile-store PATH]"),
            Self::MissingProfileStorePath => formatter.write_str("--profile-store requires a path"),
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
    ProfileStore::new(&config.profile_store_path)
        .load_into(&mut service)
        .map_err(StartupError::ProfileStore)?;
    Ok(service)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{ControlCapability, ControlId, DeviceError, DeviceSnapshot, Value};

    #[test]
    fn profile_store_path_defaults_and_overrides() {
        assert_eq!(
            Config::from_args([]).unwrap().profile_store_path,
            PathBuf::from(DEFAULT_PROFILE_STORE_PATH)
        );
        assert_eq!(
            Config::from_args(["--profile-store".into(), "/tmp/profiles".into()])
                .unwrap()
                .profile_store_path,
            PathBuf::from("/tmp/profiles")
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
                writable: true,
                available: true,
                minimum: Some(0),
                maximum: Some(100),
            }],
            values: BTreeMap::from([(control, Value::Integer(value))]),
        })
    }

    #[test]
    fn startup_loads_profiles_without_applying_them() {
        let path =
            std::env::temp_dir().join(format!("focusrited-startup-test-{}", std::process::id()));
        let config = Config {
            profile_store_path: path.clone(),
        };
        let store = ProfileStore::new(&path);
        let mut saved = Service::connect(device(50)).unwrap();
        saved.save_profile("desk".into());
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
