//! Atomic, explicit profile persistence.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use crate::{ControlId, Device, Profile, Service, Value};

const HEADER: &str = "focusrited-profiles-v2";

pub type Profiles = BTreeMap<String, Profile>;

pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> io::Result<Profiles> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => parse(&contents),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Profiles::new()),
            Err(error) => Err(error),
        }
    }

    /// Writes a complete replacement through a same-directory temporary file.
    pub fn save(&self, profiles: &Profiles) -> io::Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        let mut file = File::create(&temporary)?;
        file.write_all(encode(profiles).as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(temporary, &self.path)
    }

    pub fn load_into<D: Device>(&self, service: &mut Service<D>) -> io::Result<()> {
        service.set_profiles(self.load()?);
        Ok(())
    }

    pub fn save_service<D: Device>(&self, service: &Service<D>) -> io::Result<()> {
        self.save(service.profiles())
    }
}

fn encode(profiles: &Profiles) -> String {
    let mut output = format!("{HEADER}\n");
    for (name, profile) in profiles {
        output.push_str("P ");
        output.push_str(&hex(name.as_bytes()));
        output.push('\n');
        output.push_str("D ");
        output.push_str(&hex(profile.device_id.as_bytes()));
        output.push('\n');
        output.push_str("S ");
        output.push_str(&hex(profile.capability_schema.as_bytes()));
        output.push('\n');
        for (control, value) in &profile.values {
            output.push_str("C ");
            output.push_str(&hex(control.0.as_bytes()));
            output.push(' ');
            output.push_str(&encode_value(value));
            output.push('\n');
        }
        output.push_str("E\n");
    }
    output
}

fn encode_value(value: &Value) -> String {
    match value {
        Value::Bool(value) => format!("B {}", u8::from(*value)),
        Value::Integer(value) => format!("I {value}"),
        Value::Integer64(value) => format!("L {value}"),
        // Profiles capture writable controls only; arrays are not writable in
        // current adapters and must not silently serialize as a scalar.
        Value::Array(_) => "A".into(),
    }
}

fn parse(contents: &str) -> io::Result<Profiles> {
    let mut lines = contents.lines();
    if lines.next() != Some(HEADER) {
        return invalid("unknown profile store version");
    }

    let mut profiles = Profiles::new();
    let mut current = None;
    for line in lines {
        let mut fields = line.split_ascii_whitespace();
        match fields.next() {
            Some("P") => {
                if current.is_some() || fields.clone().count() != 1 {
                    return invalid("invalid profile header");
                }
                current = Some((
                    unhex(fields.next().expect("one field"))?,
                    None,
                    None,
                    BTreeMap::new(),
                ));
            }
            Some("D") => {
                let Some((_, device_id, _, _)) = current.as_mut() else {
                    return invalid("device outside profile");
                };
                if device_id.is_some() || fields.clone().count() != 1 {
                    return invalid("invalid device binding");
                }
                *device_id = Some(unhex(fields.next().expect("one field"))?);
            }
            Some("S") => {
                let Some((_, _, schema, _)) = current.as_mut() else {
                    return invalid("schema outside profile");
                };
                if schema.is_some() || fields.clone().count() != 1 {
                    return invalid("invalid schema binding");
                }
                *schema = Some(unhex(fields.next().expect("one field"))?);
            }
            Some("C") => {
                let Some((_, device_id, schema, values)) = current.as_mut() else {
                    return invalid("control outside profile");
                };
                if device_id.is_none() || schema.is_none() {
                    return invalid("control before profile binding");
                }
                let control = fields
                    .next()
                    .ok_or_else(|| invalid_error("missing control id"))?;
                let value = parse_value(&mut fields)?;
                if fields.next().is_some() {
                    return invalid("unexpected control data");
                }
                values.insert(ControlId(unhex(control)?), value);
            }
            Some("E") if fields.next().is_none() => {
                let Some((name, Some(device_id), Some(capability_schema), values)) = current.take()
                else {
                    return invalid("profile end without profile");
                };
                profiles.insert(
                    name,
                    Profile {
                        device_id,
                        capability_schema,
                        values,
                    },
                );
            }
            _ => return invalid("invalid profile record"),
        }
    }
    if current.is_some() {
        return invalid("unterminated profile");
    }
    Ok(profiles)
}

fn parse_value<'a>(fields: &mut impl Iterator<Item = &'a str>) -> io::Result<Value> {
    let value = fields
        .next()
        .ok_or_else(|| invalid_error("missing value type"))?;
    let mut number = || fields.next().ok_or_else(|| invalid_error("missing value"));
    match value {
        "B" => match number()? {
            "0" => Ok(Value::Bool(false)),
            "1" => Ok(Value::Bool(true)),
            _ => invalid("invalid boolean"),
        },
        "I" => number()?
            .parse()
            .map(Value::Integer)
            .map_err(|_| invalid_error("invalid integer")),
        "L" => number()?
            .parse()
            .map(Value::Integer64)
            .map_err(|_| invalid_error("invalid integer64")),
        "A" => invalid("array profiles are unsupported"),
        _ => invalid("unknown value type"),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(output, "{byte:02x}").expect("writing String cannot fail");
    }
    output
}

fn unhex(input: &str) -> io::Result<String> {
    if !input.len().is_multiple_of(2) {
        return invalid("invalid hex length");
    }
    let bytes = input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| invalid_error("invalid hex"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    String::from_utf8(bytes).map_err(|_| invalid_error("profile text is not UTF-8"))
}

fn invalid<T>(message: &str) -> io::Result<T> {
    Err(invalid_error(message))
}

fn invalid_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControlCapability, DeviceError, DeviceSnapshot, ValueDomain};

    #[test]
    fn save_load_round_trip_is_atomic_format() {
        let directory =
            std::env::temp_dir().join(format!("focusrited-profile-test-{}", std::process::id()));
        let path = directory.join("profiles");
        let store = ProfileStore::new(&path);
        let profiles = BTreeMap::from([(
            "desk\nprofile".into(),
            Profile {
                device_id: "mock-device".into(),
                capability_schema: "mock-v1".into(),
                values: BTreeMap::from([
                    (ControlId("output.mute".into()), Value::Bool(false)),
                    (ControlId("output.level".into()), Value::Integer(75)),
                ]),
            },
        )]);

        store.save(&profiles).unwrap();

        assert_eq!(store.load().unwrap(), profiles);
        assert!(!path.with_extension("tmp").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reject_corrupt_profile_store() {
        assert!(parse("focusrited-profiles-v1\nC 00 B 1\n").is_err());
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

    fn service(value: i32) -> Service<MockDevice> {
        let control = ControlId("output.level".into());
        Service::connect(MockDevice(DeviceSnapshot {
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
        }))
        .unwrap()
    }

    #[test]
    fn profiles_load_without_applying_hardware_state() {
        let directory = std::env::temp_dir().join(format!(
            "focusrited-profile-service-test-{}",
            std::process::id()
        ));
        let store = ProfileStore::new(directory.join("profiles"));
        let mut original = service(50);
        original.save_profile("desk".into());
        store.save_service(&original).unwrap();

        let mut restored = service(75);
        store.load_into(&mut restored).unwrap();

        assert_eq!(
            restored.snapshot().values[&ControlId("output.level".into())],
            Value::Integer(75)
        );
        restored.apply_profile("desk").unwrap();
        assert_eq!(
            restored.snapshot().values[&ControlId("output.level".into())],
            Value::Integer(50)
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
