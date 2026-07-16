//! Device-independent policy core for `focusrited`.
//!
//! Linux/ALSA adapters and client transports sit outside this module.  Keeping
//! the policy here makes discovery and state rules testable without hardware.

pub mod profile_store;
pub mod scarlett2_alsa;
pub mod startup;
pub mod worker;

use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ControlId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Bool(bool),
    Integer(i32),
    Integer64(i64),
    Array(Vec<Value>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlCapability {
    pub id: ControlId,
    pub writable: bool,
    pub available: bool,
    pub minimum: Option<i32>,
    pub maximum: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    /// Opaque adapter-provided identity. Profiles never cross this boundary.
    pub device_id: String,
    /// Adapter capability contract used to interpret profile control IDs.
    pub capability_schema: String,
    pub capabilities: Vec<ControlCapability>,
    pub values: BTreeMap<ControlId, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub device_id: String,
    pub capability_schema: String,
    pub values: BTreeMap<ControlId, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceError {
    Offline,
    Failed,
    WriteDisabled,
}

/// Hardware boundary. Implementations own Linux/ALSA details; policy never
/// assumes a device-specific control name.
pub trait Device {
    fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError>;
    fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceError {
    Device(DeviceError),
    UnknownControl,
    Unavailable,
    ReadOnly,
    InvalidValue,
    UnknownProfile,
    ProfileBindingMismatch,
}

pub struct Service<D> {
    device: D,
    snapshot: DeviceSnapshot,
    online: bool,
    revision: u64,
    profiles: BTreeMap<String, Profile>,
}

impl<D: Device> Service<D> {
    pub fn connect(mut device: D) -> Result<Self, ServiceError> {
        let snapshot = device.snapshot().map_err(ServiceError::Device)?;
        Ok(Self {
            device,
            snapshot,
            online: true,
            revision: 1,
            profiles: BTreeMap::new(),
        })
    }

    pub fn snapshot(&self) -> &DeviceSnapshot {
        &self.snapshot
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_online(&self) -> bool {
        self.online
    }

    /// Validate, write, then resnapshot. State is never advanced optimistically.
    pub fn command(&mut self, control: &ControlId, value: Value) -> Result<(), ServiceError> {
        let capability = self
            .snapshot
            .capabilities
            .iter()
            .find(|capability| &capability.id == control)
            .ok_or(ServiceError::UnknownControl)?;
        if !capability.available {
            return Err(ServiceError::Unavailable);
        }
        if !capability.writable {
            return Err(ServiceError::ReadOnly);
        }
        if let Value::Integer(number) = value
            && (capability.minimum.is_some_and(|minimum| number < minimum)
                || capability.maximum.is_some_and(|maximum| number > maximum))
        {
            return Err(ServiceError::InvalidValue);
        }
        self.device
            .write(control, value)
            .map_err(ServiceError::Device)?;
        self.refresh()
    }

    /// Reconcile ALSA/front-panel changes with authoritative hardware state.
    pub fn refresh(&mut self) -> Result<(), ServiceError> {
        match self.device.snapshot() {
            Ok(snapshot) => {
                let changed = snapshot != self.snapshot;
                let came_online = !self.online;
                self.snapshot = snapshot;
                self.online = true;
                if changed || came_online {
                    self.revision += 1;
                }
                Ok(())
            }
            Err(error) => {
                self.mark_offline();
                Err(ServiceError::Device(error))
            }
        }
    }

    pub fn mark_offline(&mut self) {
        if self.online {
            self.online = false;
            self.revision += 1;
        }
    }

    pub fn reconnect(&mut self) -> Result<(), ServiceError> {
        self.refresh()
    }

    /// Saving is explicit. Nothing is applied during connect or reconnect.
    pub fn save_profile(&mut self, name: String) {
        let values = self
            .snapshot
            .capabilities
            .iter()
            .filter(|capability| capability.available && capability.writable)
            .filter_map(|capability| {
                self.snapshot
                    .values
                    .get(&capability.id)
                    .map(|value| (capability.id.clone(), value.clone()))
            })
            .collect();
        self.profiles.insert(
            name,
            Profile {
                device_id: self.snapshot.device_id.clone(),
                capability_schema: self.snapshot.capability_schema.clone(),
                values,
            },
        );
    }

    pub fn profiles(&self) -> &BTreeMap<String, Profile> {
        &self.profiles
    }

    /// Loads stored profiles only. It never applies hardware state.
    pub fn set_profiles(&mut self, profiles: BTreeMap<String, Profile>) {
        self.profiles = profiles;
    }

    /// Profile writes are ordered by stable control ID. Hardware cannot make a
    /// multi-control apply atomic; later persistence adds reviewed dry-runs.
    pub fn apply_profile(&mut self, name: &str) -> Result<(), ServiceError> {
        let profile = self
            .profiles
            .get(name)
            .cloned()
            .ok_or(ServiceError::UnknownProfile)?;
        if profile.device_id != self.snapshot.device_id
            || profile.capability_schema != self.snapshot.capability_schema
        {
            return Err(ServiceError::ProfileBindingMismatch);
        }
        for (control, value) in profile.values {
            self.command(&control, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDevice {
        snapshot: DeviceSnapshot,
        fail_write: bool,
    }

    impl Device for MockDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.snapshot.clone())
        }

        fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
            if self.fail_write {
                return Err(DeviceError::Failed);
            }
            self.snapshot.values.insert(control.clone(), value);
            Ok(())
        }
    }

    fn mock() -> MockDevice {
        let volume = ControlId("output.volume".into());
        MockDevice {
            snapshot: DeviceSnapshot {
                device_id: "mock-device".into(),
                capability_schema: "mock-v1".into(),
                capabilities: vec![ControlCapability {
                    id: volume.clone(),
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                }],
                values: BTreeMap::from([(volume, Value::Integer(50))]),
            },
            fail_write: false,
        }
    }

    #[test]
    fn command_confirms_hardware_state() {
        let volume = ControlId("output.volume".into());
        let mut service = Service::connect(mock()).unwrap();

        service.command(&volume, Value::Integer(75)).unwrap();

        assert_eq!(service.snapshot().values[&volume], Value::Integer(75));
        assert_eq!(service.revision(), 2);
    }

    #[test]
    fn external_change_reconciles_and_disconnect_reconnect_changes_state() {
        let mut service = Service::connect(mock()).unwrap();
        service
            .device
            .snapshot
            .values
            .insert(ControlId("output.volume".into()), Value::Integer(60));

        service.refresh().unwrap();
        service.mark_offline();
        service.reconnect().unwrap();

        assert!(service.is_online());
        assert_eq!(service.revision(), 4);
    }

    #[test]
    fn failed_write_does_not_change_state() {
        let volume = ControlId("output.volume".into());
        let mut device = mock();
        device.fail_write = true;
        let mut service = Service::connect(device).unwrap();

        assert_eq!(
            service.command(&volume, Value::Integer(75)),
            Err(ServiceError::Device(DeviceError::Failed))
        );
        assert_eq!(service.snapshot().values[&volume], Value::Integer(50));
    }

    #[test]
    fn profile_needs_explicit_apply() {
        let volume = ControlId("output.volume".into());
        let mut service = Service::connect(mock()).unwrap();
        service.save_profile("desk".into());
        service.command(&volume, Value::Integer(75)).unwrap();

        service.apply_profile("desk").unwrap();

        assert_eq!(service.snapshot().values[&volume], Value::Integer(50));
    }

    #[test]
    fn profile_does_not_apply_to_different_device_or_schema() {
        let mut service = Service::connect(mock()).unwrap();
        service.save_profile("desk".into());
        service.snapshot.device_id = "other-device".into();

        assert_eq!(
            service.apply_profile("desk"),
            Err(ServiceError::ProfileBindingMismatch)
        );
        service.snapshot.device_id = "mock-device".into();
        service.snapshot.capability_schema = "mock-v2".into();
        assert_eq!(
            service.apply_profile("desk"),
            Err(ServiceError::ProfileBindingMismatch)
        );
    }
}
