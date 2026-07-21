#[cfg(feature = "hardware-write-tests")]
use std::collections::BTreeSet;
use std::sync::Mutex;

#[cfg(feature = "hardware-write-tests")]
use focusrited::Device;
use focusrited::{
    ControlId, Value,
    scarlett2_alsa::{Scarlett2Alsa, ValueType, discover},
    worker::DeviceWorker,
};

static HARDWARE_LOCK: Mutex<()> = Mutex::new(());

fn hardware_card() -> String {
    std::env::var("FOCUSRITED_HARDWARE_CARD")
        .expect("FOCUSRITED_HARDWARE_CARD must identify the attached Solo")
}

#[test]
fn fixture_records_solo_controls() {
    let fixture = include_str!("fixtures/scarlett-solo-4th-gen.md");

    assert!(fixture.contains("Direct Monitor Playback Switch"));
    assert!(fixture.contains("0–184"));
    assert!(fixture.contains("step 1"));
}

#[test]
#[ignore = "requires an attached Scarlett Solo ALSA card"]
fn discovers_attached_solo() {
    let _hardware_guard = HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let card = hardware_card();
    let discovery = discover(&card).unwrap();

    assert!(discovery.controls.len() >= 56);
    assert!(discovery.controls.iter().any(|control| {
        control.name == "Level Meter" && control.value_type == ValueType::Integer
    }));
    assert!(discovery.controls.iter().any(|control| {
        control.name.starts_with("Monitor Mix ")
            && control.integer_range
                == Some(focusrited::scarlett2_alsa::IntegerRange {
                    minimum: 0,
                    maximum: 184,
                    step: 1,
                })
    }));

    let worker = DeviceWorker::start(Scarlett2Alsa::new(card)).unwrap();
    let snapshot = worker.state().unwrap().snapshot;
    assert_eq!(snapshot.capabilities.len(), discovery.controls.len());
    assert!(
        snapshot
            .capabilities
            .iter()
            .all(|control| !control.writable)
    );

    assert_eq!(
        worker.command(
            ControlId("alsa-numid:48".into()),
            focusrited::Value::Bool(true)
        ),
        Err(focusrited::worker::WorkerError::Service(
            focusrited::ServiceError::ReadOnly
        ))
    );
    assert_eq!(worker.state().unwrap().revision, 1);
    worker.stop().unwrap();
}

#[test]
#[ignore = "toggle the Solo Direct Monitor control during this 30-second read-only check"]
fn reconciles_external_direct_monitor_change() {
    let _hardware_guard = HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let control = ControlId("alsa-numid:48".into());
    let worker = DeviceWorker::start(Scarlett2Alsa::new(hardware_card())).unwrap();
    let before = worker.state().unwrap();
    let previous = before.snapshot.values[&control].clone();
    println!("Direct Monitor before: {previous:?}. Toggle Direct now.");

    for _ in 0..120 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let current = worker.state().unwrap();
        if current.snapshot.values[&control] != previous {
            println!(
                "Direct Monitor after: {:?}; revision {} to {}.",
                current.snapshot.values[&control], before.revision, current.revision
            );
            assert!(current.revision > before.revision);
            assert!(matches!(current.snapshot.values[&control], Value::Bool(_)));
            worker.stop().unwrap();
            return;
        }
    }

    let after = worker.state().unwrap();
    worker.stop().unwrap();
    panic!(
        "Direct Monitor did not change within 30 seconds: before {previous:?}, after {:?}",
        after.snapshot.values[&control]
    );
}

#[test]
#[ignore = "disconnect then reconnect the Solo during this 60-second read-only check"]
fn reconnects_after_solo_disconnect() {
    let _hardware_guard = HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let worker = DeviceWorker::start(Scarlett2Alsa::new(hardware_card())).unwrap();
    let initial = worker.state().unwrap();
    let mut saw_offline = false;
    println!("Disconnect Solo now, then reconnect it.");

    for _ in 0..240 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let state = worker.state().unwrap();
        if saw_offline && state.online {
            println!(
                "Solo reconnected; revision {} to {}.",
                initial.revision, state.revision
            );
            assert!(state.revision > initial.revision);
            worker.stop().unwrap();
            return;
        }
        if !state.online && !saw_offline {
            println!("Solo offline at revision {}.", state.revision);
            saw_offline = true;
        }
    }

    worker.stop().unwrap();
    panic!("did not observe both Solo disconnect and reconnect within 60 seconds");
}

#[cfg(feature = "hardware-write-tests")]
#[test]
#[ignore = "requires explicit approval; toggles Direct Monitor once and restores it"]
fn toggles_direct_monitor_and_restores_it() {
    let _hardware_guard = HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let card = hardware_card();
    let control = discover(&card)
        .unwrap()
        .controls
        .into_iter()
        .find(|control| control.name == "Direct Monitor Playback Switch")
        .expect("attached Solo must expose Direct Monitor Playback Switch");
    assert_eq!(control.value_type, ValueType::Boolean);
    assert_eq!(control.values.len(), 1);
    let id = control.id;
    let before = match control.values[0] {
        focusrited::scarlett2_alsa::ObservedValue::Boolean(value) => value,
        _ => unreachable!("Direct Monitor must be boolean"),
    };
    let mut device = Scarlett2Alsa::with_writable_controls(card, BTreeSet::from([id.clone()]));
    let target = !before;

    let changed = device.write(&id, Value::Bool(target));
    let after_change = device.snapshot();
    let restored = device.write(&id, Value::Bool(before));
    let after_restore = device.snapshot();

    assert!(changed.is_ok(), "change failed: {changed:?}");
    assert_eq!(after_change.unwrap().values[&id], Value::Bool(target));
    assert!(restored.is_ok(), "restore failed: {restored:?}");
    assert_eq!(after_restore.unwrap().values[&id], Value::Bool(before));
}
