//! Read-only ALSA control discovery for Scarlett2-family devices.
//!
//! This module intentionally has no write API. Hardware mutation is added only
//! after explicit approval and device-specific validation.

use alsa::{
    ctl::{ElemType, ElemValue},
    hctl::HCtl,
};

use crate::{ControlCapability, ControlId, Device, DeviceError, DeviceSnapshot, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
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
    pub values: Vec<ObservedValue>,
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

/// Read-only device adapter. It can reconcile current hardware state but never
/// mutates a control.
pub struct Scarlett2Alsa {
    card: String,
}

impl Scarlett2Alsa {
    pub fn new(card: impl Into<String>) -> Self {
        Self { card: card.into() }
    }
}

impl Device for Scarlett2Alsa {
    fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
        discover(&self.card)
            .map(snapshot_from)
            .map_err(|_| DeviceError::Offline)
    }

    fn write(&mut self, _: &ControlId, _: Value) -> Result<(), DeviceError> {
        Err(DeviceError::WriteDisabled)
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
        .map(|element| {
            let id = element
                .get_id()
                .map_err(|error| DiscoveryError(error.to_string()))?;
            let info = element
                .info()
                .map_err(|error| DiscoveryError(error.to_string()))?;
            let value = element
                .read()
                .map_err(|error| DiscoveryError(error.to_string()))?;
            let numid = id.get_numid();

            Ok(Control {
                id: control_id(numid),
                name: id
                    .get_name()
                    .map_err(|error| DiscoveryError(error.to_string()))?
                    .to_owned(),
                numid,
                value_type: value_type(info.get_type()),
                values: observed_values(&value, info.get_type(), info.get_count()),
            })
        })
        .collect::<Result<Vec<_>, DiscoveryError>>()?;

    Ok(Discovery { controls })
}

fn control_id(numid: u32) -> ControlId {
    ControlId(format!("alsa-numid:{numid}"))
}

fn snapshot_from(discovery: Discovery) -> DeviceSnapshot {
    let mut capabilities = Vec::with_capacity(discovery.controls.len());
    let mut values = std::collections::BTreeMap::new();
    for control in discovery.controls {
        let value = control_value(&control.values);
        capabilities.push(ControlCapability {
            id: control.id.clone(),
            // Read-only mode is deliberate until hardware writes are approved.
            writable: false,
            available: true,
            minimum: None,
            maximum: None,
        });
        values.insert(control.id, value);
    }
    DeviceSnapshot {
        capabilities,
        values,
    }
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
        let snapshot = snapshot_from(Discovery {
            controls: vec![Control {
                id: control_id(48),
                name: "Direct Monitor Playback Switch".into(),
                numid: 48,
                value_type: ValueType::Boolean,
                values: vec![ObservedValue::Boolean(false)],
            }],
        });

        assert!(!snapshot.capabilities[0].writable);
        assert_eq!(snapshot.values[&control_id(48)], Value::Bool(false));
    }
}
