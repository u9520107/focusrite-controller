//! ALSA control discovery for Scarlett2-family devices.
//!
//! Instances stay read-only unless constructed with an explicitly approved
//! discovered boolean control for a bounded hardware test.

use std::collections::BTreeSet;

use alsa::{
    ctl::{Ctl, ElemType, ElemValue},
    hctl::HCtl,
};

use crate::{
    ControlCapability, ControlId, Device, DeviceError, DeviceSnapshot, Value, ValueDomain,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
    pub device_id: String,
    pub controls: Vec<Control>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Control {
    /// Stable only within this discovered device capability set.
    pub id: ControlId,
    /// ALSA display metadata; never use it as a shared-model identifier.
    pub name: String,
    pub numid: u32,
    pub value_type: ValueType,
    pub count: u32,
    pub values: Vec<ObservedValue>,
    /// A single unreadable control must not make the whole card offline.
    pub available: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueType {
    Boolean,
    Integer,
    Integer64,
    Enumerated,
    Bytes,
    Iec958,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservedValue {
    Boolean(bool),
    Integer(i32),
    Integer64(i64),
    Enumerated(u32),
    Byte(u8),
}

#[derive(Debug)]
pub struct DiscoveryError(String);

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DiscoveryError {}

pub struct Scarlett2Alsa {
    card: String,
    writable_controls: BTreeSet<ControlId>,
}

impl Scarlett2Alsa {
    pub fn new(card: impl Into<String>) -> Self {
        Self {
            card: card.into(),
            writable_controls: BTreeSet::new(),
        }
    }

    /// Enables only specified discovered boolean controls.
    pub fn with_writable_controls(
        card: impl Into<String>,
        writable_controls: BTreeSet<ControlId>,
    ) -> Self {
        Self {
            card: card.into(),
            writable_controls,
        }
    }
}

impl Device for Scarlett2Alsa {
    fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
        discover(&self.card)
            .map(|discovery| snapshot_from(discovery, &self.writable_controls))
            .map_err(|_| DeviceError::Offline)
    }

    fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
        if !self.writable_controls.contains(control) {
            return Err(DeviceError::WriteDisabled);
        }
        let Value::Bool(value) = value else {
            return Err(DeviceError::WriteDisabled);
        };
        write_boolean(&self.card, control, value)
    }
}

/// Opens `hw:<card>` and reads every ALSA control once.
pub fn discover(card: &str) -> Result<Discovery, DiscoveryError> {
    let control = HCtl::new(&format!("hw:{card}"), false)
        .map_err(|error| DiscoveryError(error.to_string()))?;
    control
        .load()
        .map_err(|error| DiscoveryError(error.to_string()))?;

    let controls = control
        .elem_iter()
        .filter_map(|element| {
            let id = element.get_id().ok()?;
            let numid = id.get_numid();
            let name = id
                .get_name()
                .map(str::to_owned)
                .unwrap_or_else(|_| format!("alsa-numid:{numid}"));
            let Ok(info) = element.info() else {
                return Some(Control {
                    id: control_id(numid),
                    name,
                    numid,
                    value_type: ValueType::None,
                    count: 0,
                    values: Vec::new(),
                    available: false,
                });
            };
            let value_type = value_type(info.get_type());
            let count = info.get_count();
            match element.read() {
                Ok(value) => Some(Control {
                    id: control_id(numid),
                    name,
                    numid,
                    value_type,
                    count,
                    values: observed_values(&value, info.get_type(), count),
                    available: true,
                }),
                Err(_) => Some(Control {
                    id: control_id(numid),
                    name,
                    numid,
                    value_type,
                    count,
                    values: Vec::new(),
                    available: false,
                }),
            }
        })
        .collect();

    Ok(Discovery {
        device_id: device_id(card),
        controls,
    })
}

fn device_id(card: &str) -> String {
    let Ok(control) = Ctl::new(&format!("hw:{card}"), false) else {
        return format!("alsa-card:{card}");
    };
    let Ok(info) = control.card_info() else {
        return format!("alsa-card:{card}");
    };
    let index = info.get_card().get_index();
    let serial = std::fs::read_to_string(format!("/sys/class/sound/card{index}/device/serial"))
        .ok()
        .map(|serial| serial.trim().to_owned())
        .filter(|serial| !serial.is_empty());
    let driver = info.get_driver().unwrap_or("unknown");
    let id = info.get_id().unwrap_or("unknown");
    match serial {
        Some(serial) => format!("alsa-usb:{driver}:{id}:{serial}"),
        None => format!("alsa-card:{driver}:{id}"),
    }
}

fn control_id(numid: u32) -> ControlId {
    ControlId(format!("alsa-numid:{numid}"))
}

fn snapshot_from(discovery: Discovery, writable_controls: &BTreeSet<ControlId>) -> DeviceSnapshot {
    let mut capabilities = Vec::with_capacity(discovery.controls.len());
    let mut values = std::collections::BTreeMap::new();
    let capability_schema = schema_fingerprint(&discovery.controls);
    for control in discovery.controls {
        capabilities.push(ControlCapability {
            id: control.id.clone(),
            domain: value_domain(control.value_type, control.count),
            writable: control.value_type == ValueType::Boolean
                && control.values.len() == 1
                && writable_controls.contains(&control.id),
            available: control.available,
            minimum: None,
            maximum: None,
        });
        if control.available {
            values.insert(control.id, control_value(&control.values));
        }
    }
    DeviceSnapshot {
        device_id: discovery.device_id,
        capability_schema,
        capabilities,
        values,
    }
}

fn value_domain(value_type: ValueType, count: u32) -> ValueDomain {
    if count != 1 {
        return ValueDomain::Array;
    }
    match value_type {
        ValueType::Boolean => ValueDomain::Boolean,
        ValueType::Integer => ValueDomain::Integer,
        ValueType::Integer64 => ValueDomain::Integer64,
        ValueType::Enumerated | ValueType::Bytes | ValueType::Iec958 | ValueType::None => {
            ValueDomain::Unsupported
        }
    }
}

fn schema_fingerprint(controls: &[Control]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for control in controls {
        for byte in format!(
            "{}:{:?}:{};",
            control.id.0, control.value_type, control.count
        )
        .bytes()
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("scarlett2-alsa-{hash:016x}")
}

fn write_boolean(card: &str, control: &ControlId, value: bool) -> Result<(), DeviceError> {
    let numid = control
        .0
        .strip_prefix("alsa-numid:")
        .and_then(|number| number.parse().ok())
        .ok_or(DeviceError::WriteDisabled)?;
    let hctl = HCtl::new(&format!("hw:{card}"), false).map_err(|_| DeviceError::Offline)?;
    hctl.load().map_err(|_| DeviceError::Offline)?;
    let element = hctl
        .elem_iter()
        .find(|element| element.get_id().is_ok_and(|id| id.get_numid() == numid))
        .ok_or(DeviceError::WriteDisabled)?;
    let info = element.info().map_err(|_| DeviceError::Failed)?;
    if info.get_type() != ElemType::Boolean || info.get_count() != 1 {
        return Err(DeviceError::WriteDisabled);
    }
    let mut current = element.read().map_err(|_| DeviceError::Failed)?;
    current
        .set_boolean(0, value)
        .ok_or(DeviceError::WriteDisabled)?;
    element.write(&current).map_err(|_| DeviceError::Failed)?;
    Ok(())
}

fn control_value(values: &[ObservedValue]) -> Value {
    let mut values = values.iter().map(observed_value).collect::<Vec<_>>();
    if values.len() == 1 {
        values.pop().expect("one value")
    } else {
        Value::Array(values)
    }
}

fn observed_value(value: &ObservedValue) -> Value {
    match value {
        ObservedValue::Boolean(value) => Value::Bool(*value),
        ObservedValue::Integer(value) => Value::Integer(*value),
        ObservedValue::Integer64(value) => Value::Integer64(*value),
        ObservedValue::Enumerated(value) => Value::Integer(*value as i32),
        ObservedValue::Byte(value) => Value::Integer((*value).into()),
    }
}

fn value_type(value_type: ElemType) -> ValueType {
    match value_type {
        ElemType::Boolean => ValueType::Boolean,
        ElemType::Integer => ValueType::Integer,
        ElemType::Integer64 => ValueType::Integer64,
        ElemType::Enumerated => ValueType::Enumerated,
        ElemType::Bytes => ValueType::Bytes,
        ElemType::IEC958 => ValueType::Iec958,
        ElemType::None => ValueType::None,
    }
}

fn observed_values(value: &ElemValue, value_type: ElemType, count: u32) -> Vec<ObservedValue> {
    (0..count)
        .filter_map(|index| match value_type {
            ElemType::Boolean => value.get_boolean(index).map(ObservedValue::Boolean),
            ElemType::Integer => value.get_integer(index).map(ObservedValue::Integer),
            ElemType::Integer64 => value.get_integer64(index).map(ObservedValue::Integer64),
            ElemType::Enumerated => value.get_enumerated(index).map(ObservedValue::Enumerated),
            ElemType::Bytes => value.get_byte(index).map(ObservedValue::Byte),
            ElemType::IEC958 | ElemType::None => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_ids_do_not_depend_on_display_labels() {
        assert_eq!(control_id(48), ControlId("alsa-numid:48".into()));
    }

    #[test]
    fn snapshots_are_read_only() {
        let snapshot = snapshot_from(
            Discovery {
                device_id: "mock-device".into(),
                controls: vec![Control {
                    id: control_id(48),
                    name: "Direct Monitor Playback Switch".into(),
                    numid: 48,
                    value_type: ValueType::Boolean,
                    count: 1,
                    values: vec![ObservedValue::Boolean(false)],
                    available: true,
                }],
            },
            &BTreeSet::new(),
        );

        assert!(!snapshot.capabilities[0].writable);
        assert_eq!(snapshot.values[&control_id(48)], Value::Bool(false));
    }

    #[test]
    fn approved_boolean_control_is_writable() {
        let control = control_id(48);
        let snapshot = snapshot_from(
            Discovery {
                device_id: "mock-device".into(),
                controls: vec![Control {
                    id: control.clone(),
                    name: "Direct Monitor Playback Switch".into(),
                    numid: 48,
                    value_type: ValueType::Boolean,
                    count: 1,
                    values: vec![ObservedValue::Boolean(false)],
                    available: true,
                }],
            },
            &BTreeSet::from([control]),
        );

        assert!(snapshot.capabilities[0].writable);
    }

    #[test]
    fn schema_fingerprint_ignores_availability_but_tracks_shape() {
        let mut controls = vec![Control {
            id: control_id(48),
            name: "Direct Monitor Playback Switch".into(),
            numid: 48,
            value_type: ValueType::Boolean,
            count: 1,
            values: vec![ObservedValue::Boolean(false)],
            available: true,
        }];
        let original = schema_fingerprint(&controls);
        controls[0].available = false;

        assert_eq!(original, schema_fingerprint(&controls));
        controls[0].count = 2;

        assert_ne!(original, schema_fingerprint(&controls));
    }
}
