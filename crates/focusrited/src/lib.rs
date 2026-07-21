//! Device-independent policy core for `focusrited`.
//!
//! Linux/ALSA adapters and client transports sit outside this module.  Keeping
//! the policy here makes discovery and state rules testable without hardware.

pub mod dashboard_store;
pub mod groups;
pub mod ipc;
pub mod profile_store;
pub mod scarlett2_alsa;
pub mod startup;
pub mod worker;

use std::{collections::BTreeMap, thread, time::Duration};

use groups::{GroupError, GroupResult, LevelGroup, map_level, unmap_level};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct ControlId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Bool(bool),
    Integer(i32),
    Integer64(i64),
    Array(Vec<Value>),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueDomain {
    Boolean,
    Integer,
    Integer64,
    Unsupported,
    Array,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationKind {
    Level,
    Mute,
}

/// Adapter-declared compound operation eligibility. Absence fails closed.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupOperation {
    RelativeLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GroupCapability {
    pub operation: GroupOperation,
}

/// Adapter-declared UI metadata. Absence means client must not guess display.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ControlPresentation {
    pub label: String,
    pub kind: PresentationKind,
    /// Lower values appear first in the adapter-default dashboard.
    pub default_dashboard_order: Option<u8>,
    /// Compatible level/mute control, when discovery proves the association.
    pub companion: Option<ControlId>,
    /// Real integer increment when the hardware declares one.
    pub step: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ControlCapability {
    pub id: ControlId,
    pub domain: ValueDomain,
    pub writable: bool,
    pub available: bool,
    pub minimum: Option<i32>,
    pub maximum: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<ControlPresentation>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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

    /// Waits for a hardware state-change notification without writing hardware.
    fn wait_for_change(&mut self, timeout: Duration) -> Result<bool, DeviceError> {
        thread::sleep(timeout);
        Ok(false)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceError {
    Device(DeviceError),
    UnknownControl,
    Unavailable,
    ReadOnly,
    InvalidValue,
    UnconfirmedWrite,
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
        if !matches_domain(&value, &capability.domain) {
            return Err(ServiceError::InvalidValue);
        }
        if let Value::Integer(number) = value
            && (capability.minimum.is_some_and(|minimum| number < minimum)
                || capability.maximum.is_some_and(|maximum| number > maximum))
        {
            return Err(ServiceError::InvalidValue);
        }
        self.device
            .write(control, value.clone())
            .map_err(ServiceError::Device)?;
        self.refresh()?;
        if self.snapshot.values.get(control) != Some(&value) {
            return Err(ServiceError::UnconfirmedWrite);
        }
        Ok(())
    }

    /// Ordered non-atomic compound operation. Stops at first failed confirmation.
    pub fn command_level_group(
        &mut self,
        group: &LevelGroup,
        position: u16,
    ) -> Result<GroupResult, GroupError> {
        if position > 1000 {
            return Err(GroupError::InvalidPosition);
        }
        group.validate(&self.snapshot.capabilities)?;
        let normalized = group
            .members
            .iter()
            .map(|member| {
                let capability = self
                    .snapshot
                    .capabilities
                    .iter()
                    .find(|item| item.id == *member)
                    .ok_or_else(|| GroupError::IneligibleMember(member.clone()))?;
                let Value::Integer(value) = self
                    .snapshot
                    .values
                    .get(member)
                    .ok_or_else(|| GroupError::UnmappableCurrentState(member.clone()))?
                else {
                    return Err(GroupError::UnmappableCurrentState(member.clone()));
                };
                Ok((
                    member.clone(),
                    unmap_level(
                        *value,
                        capability
                            .minimum
                            .ok_or_else(|| GroupError::IneligibleMember(member.clone()))?,
                        capability
                            .maximum
                            .ok_or_else(|| GroupError::IneligibleMember(member.clone()))?,
                    )
                    .map_err(|_| GroupError::UnmappableCurrentState(member.clone()))?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let anchor = normalized
            .iter()
            .find(|(member, _)| *member == group.anchor)
            .map(|(_, position)| *position)
            .ok_or(GroupError::InvalidAnchor)?;
        let delta = i32::from(position) - i32::from(anchor);
        let commands = group
            .members
            .iter()
            .zip(normalized)
            .map(|(member, (_, current))| {
                let capability = self
                    .snapshot
                    .capabilities
                    .iter()
                    .find(|item| item.id == *member)
                    .ok_or_else(|| GroupError::IneligibleMember(member.clone()))?;
                Ok((
                    member.clone(),
                    map_level(
                        (i32::from(current) + delta).clamp(0, 1000) as u16,
                        capability
                            .minimum
                            .ok_or_else(|| GroupError::IneligibleMember(member.clone()))?,
                        capability
                            .maximum
                            .ok_or_else(|| GroupError::IneligibleMember(member.clone()))?,
                    )?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = GroupResult {
            applied: Vec::new(),
            skipped: Vec::new(),
            failed: None,
        };
        for (member, value) in commands {
            if self.snapshot.values.get(&member) == Some(&Value::Integer(value)) {
                result.skipped.push(member);
                continue;
            }
            if let Err(error) = self.command(&member, Value::Integer(value)) {
                result.failed = Some((member.clone(), error));
                return Ok(result);
            }
            result.applied.push(member.clone());
        }
        Ok(result)
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

    /// Reconciles one hardware event immediately. A false result is a timeout.
    pub fn wait_for_change(&mut self, timeout: Duration) -> Result<bool, ServiceError> {
        match self.device.wait_for_change(timeout) {
            Ok(true) => {
                self.refresh()?;
                Ok(true)
            }
            Ok(false) => Ok(false),
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

fn matches_domain(value: &Value, domain: &ValueDomain) -> bool {
    matches!(
        (value, domain),
        (Value::Bool(_), ValueDomain::Boolean)
            | (Value::Integer(_), ValueDomain::Integer)
            | (Value::Integer64(_), ValueDomain::Integer64)
            | (Value::Array(_), ValueDomain::Array)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDevice {
        snapshot: DeviceSnapshot,
        failed_control: Option<ControlId>,
        ignore_write: bool,
        writes: Vec<ControlId>,
    }

    impl Device for MockDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.snapshot.clone())
        }

        fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
            self.writes.push(control.clone());
            if self.failed_control.as_ref() == Some(control) {
                return Err(DeviceError::Failed);
            }
            if !self.ignore_write {
                self.snapshot.values.insert(control.clone(), value);
            }
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
                    domain: ValueDomain::Integer,
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                    group: Some(GroupCapability {
                        operation: GroupOperation::RelativeLevel,
                    }),
                    presentation: None,
                }],
                values: BTreeMap::from([(volume, Value::Integer(50))]),
            },
            failed_control: None,
            ignore_write: false,
            writes: Vec::new(),
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
        device.failed_control = Some(volume.clone());
        let mut service = Service::connect(device).unwrap();

        assert_eq!(
            service.command(&volume, Value::Integer(75)),
            Err(ServiceError::Device(DeviceError::Failed))
        );
        assert_eq!(service.snapshot().values[&volume], Value::Integer(50));
    }

    #[test]
    fn ignored_write_is_not_reported_as_confirmed() {
        let volume = ControlId("output.volume".into());
        let mut device = mock();
        device.ignore_write = true;
        let mut service = Service::connect(device).unwrap();

        assert_eq!(
            service.command(&volume, Value::Integer(75)),
            Err(ServiceError::UnconfirmedWrite)
        );
        assert_eq!(service.snapshot().values[&volume], Value::Integer(50));
    }

    #[test]
    fn command_rejects_value_with_wrong_domain() {
        let volume = ControlId("output.volume".into());
        let mut service = Service::connect(mock()).unwrap();

        assert_eq!(
            service.command(&volume, Value::Bool(true)),
            Err(ServiceError::InvalidValue)
        );
    }

    #[test]
    fn level_group_confirms_each_member_in_order() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let mut device = mock();
        device.snapshot.capabilities.push(ControlCapability {
            id: second.clone(),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(20),
            maximum: Some(220),
            group: Some(GroupCapability {
                operation: GroupOperation::RelativeLevel,
            }),
            presentation: None,
        });
        device
            .snapshot
            .values
            .insert(second.clone(), Value::Integer(20));
        let mut service = Service::connect(device).unwrap();
        let result = service
            .command_level_group(
                &LevelGroup {
                    members: vec![first.clone(), second.clone()],
                    anchor: first.clone(),
                },
                750,
            )
            .unwrap();
        assert_eq!(result.applied, vec![first.clone(), second.clone()]);
        assert_eq!(result.skipped, Vec::<ControlId>::new());
        assert_eq!(result.failed, None);
        assert_eq!(service.snapshot().values[&first], Value::Integer(75));
        assert_eq!(service.snapshot().values[&second], Value::Integer(70));
    }

    #[test]
    fn level_group_stops_after_failed_member() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let third = ControlId("headphone.volume".into());
        let mut device = mock();
        for control in [&second, &third] {
            device.snapshot.capabilities.push(ControlCapability {
                id: control.clone(),
                domain: ValueDomain::Integer,
                writable: true,
                available: true,
                minimum: Some(0),
                maximum: Some(100),
                group: Some(GroupCapability {
                    operation: GroupOperation::RelativeLevel,
                }),
                presentation: None,
            });
            device
                .snapshot
                .values
                .insert(control.clone(), Value::Integer(0));
        }
        device.failed_control = Some(second.clone());
        let mut service = Service::connect(device).unwrap();

        let result = service
            .command_level_group(
                &LevelGroup {
                    members: vec![first.clone(), second.clone(), third.clone()],
                    anchor: first.clone(),
                },
                750,
            )
            .unwrap();

        assert_eq!(result.applied, vec![first.clone()]);
        assert_eq!(result.skipped, Vec::<ControlId>::new());
        assert_eq!(
            result.failed,
            Some((second.clone(), ServiceError::Device(DeviceError::Failed)))
        );
        assert_eq!(service.device.writes, vec![first.clone(), second]);
        assert_eq!(service.snapshot().values[&first], Value::Integer(75));
        assert_eq!(service.snapshot().values[&third], Value::Integer(0));
    }

    #[test]
    fn level_group_clamps_preserved_balance_at_limits() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let mut device = mock();
        device.snapshot.capabilities.push(ControlCapability {
            id: second.clone(),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(0),
            maximum: Some(100),
            group: Some(GroupCapability {
                operation: GroupOperation::RelativeLevel,
            }),
            presentation: None,
        });
        device
            .snapshot
            .values
            .insert(second.clone(), Value::Integer(90));
        let mut service = Service::connect(device).unwrap();

        let result = service
            .command_level_group(
                &LevelGroup {
                    members: vec![first.clone(), second.clone()],
                    anchor: first.clone(),
                },
                1000,
            )
            .unwrap();

        assert_eq!(result.applied, vec![first.clone(), second.clone()]);
        assert_eq!(service.snapshot().values[&first], Value::Integer(100));
        assert_eq!(service.snapshot().values[&second], Value::Integer(100));
    }

    #[test]
    fn level_group_rejects_invalid_baseline_without_writes() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let mut device = mock();
        device.snapshot.capabilities.push(ControlCapability {
            id: second.clone(),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(0),
            maximum: Some(100),
            group: Some(GroupCapability {
                operation: GroupOperation::RelativeLevel,
            }),
            presentation: None,
        });
        device
            .snapshot
            .values
            .insert(first.clone(), Value::Integer(101));
        device
            .snapshot
            .values
            .insert(second.clone(), Value::Integer(0));
        let mut service = Service::connect(device).unwrap();

        assert_eq!(
            service.command_level_group(
                &LevelGroup {
                    members: vec![first.clone(), second],
                    anchor: first.clone(),
                },
                500,
            ),
            Err(GroupError::UnmappableCurrentState(first))
        );
        assert!(service.device.writes.is_empty());
    }

    #[test]
    fn level_group_rejects_out_of_range_position_without_writes() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let mut device = mock();
        device.snapshot.capabilities.push(ControlCapability {
            id: second.clone(),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(0),
            maximum: Some(100),
            group: Some(GroupCapability {
                operation: GroupOperation::RelativeLevel,
            }),
            presentation: None,
        });
        device
            .snapshot
            .values
            .insert(second.clone(), Value::Integer(25));
        let mut service = Service::connect(device).unwrap();

        assert_eq!(
            service.command_level_group(
                &LevelGroup {
                    members: vec![first, second],
                    anchor: ControlId("output.volume".into()),
                },
                1001,
            ),
            Err(GroupError::InvalidPosition)
        );
        assert!(service.device.writes.is_empty());
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
