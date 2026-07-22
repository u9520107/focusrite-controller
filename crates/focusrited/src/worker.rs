//! Single-worker boundary for all blocking device operations.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    ControlId, Device, DeviceSnapshot, GroupError, GroupResult, MirrorResult, Service,
    ServiceError, Value, dashboard_store::DashboardConfig, profile_store::Profiles,
};

const QUEUE_LIMIT: usize = 32;
const EVENT_WAIT: Duration = Duration::from_millis(10);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(3);
const REQUEST_BATCH_LIMIT: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub dashboard: DashboardConfig,
    pub snapshot: DeviceSnapshot,
    pub revision: u64,
    pub online: bool,
    pub mirror_results: Vec<MirrorResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerError {
    Dashboard(String),
    Group(GroupError),
    UnknownGroup,
    Service(ServiceError),
    Stopped,
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dashboard(error) => write!(formatter, "dashboard configuration: {error}"),
            Self::Group(error) => write!(formatter, "group command: {error:?}"),
            Self::UnknownGroup => formatter.write_str("unknown dashboard group"),
            Self::Service(error) => write!(formatter, "service error: {error:?}"),
            Self::Stopped => formatter.write_str("device worker stopped"),
        }
    }
}

impl std::error::Error for WorkerError {}

pub struct DeviceWorker {
    sender: SyncSender<Request>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

enum Request {
    State(std::sync::mpsc::Sender<State>),
    Refresh(std::sync::mpsc::Sender<Result<State, ServiceError>>),
    Command {
        control: ControlId,
        value: Value,
        reply: std::sync::mpsc::Sender<Result<State, ServiceError>>,
    },
    LevelGroup {
        group: String,
        position: u16,
        reply: std::sync::mpsc::Sender<Result<(State, GroupResult), WorkerError>>,
    },
    Stop(std::sync::mpsc::Sender<()>),
}

impl DeviceWorker {
    /// Starts one bounded queue and one serial device-owning thread.
    pub fn start<D: Device + Send + 'static>(device: D) -> Result<Self, WorkerError> {
        Self::start_with_profiles(device, Profiles::new())
    }

    /// Starts with stored profiles. Loading profiles never applies a write.
    pub fn start_with_profiles<D: Device + Send + 'static>(
        device: D,
        profiles: Profiles,
    ) -> Result<Self, WorkerError> {
        Self::start_with_dashboard(device, profiles, None)
    }

    /// Dashboard metadata is validated only after authoritative device discovery.
    pub fn start_with_dashboard<D: Device + Send + 'static>(
        device: D,
        profiles: Profiles,
        dashboard: Option<DashboardConfig>,
    ) -> Result<Self, WorkerError> {
        let (sender, receiver) = sync_channel(QUEUE_LIMIT);
        let (ready_sender, ready_receiver) = sync_channel(1);
        let thread = thread::spawn(move || match Service::connect(device) {
            Ok(mut service) => {
                service.set_profiles(profiles);
                let dashboard =
                    dashboard.unwrap_or_else(|| DashboardConfig::defaults(service.snapshot()));
                if let Err(error) = dashboard.validate_for(service.snapshot()) {
                    let _ = ready_sender.send(Err(WorkerError::Dashboard(error.to_string())));
                    return;
                }
                let _ = ready_sender.send(Ok(()));
                run(service, receiver, dashboard);
            }
            Err(error) => {
                let _ = ready_sender.send(Err(WorkerError::Service(error)));
            }
        });

        match ready_receiver.recv().map_err(|_| WorkerError::Stopped)? {
            Ok(()) => Ok(Self {
                sender,
                thread: Mutex::new(Some(thread)),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }

    pub fn state(&self) -> Result<State, WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::State(sender))
            .map_err(|_| WorkerError::Stopped)?;
        receiver.recv().map_err(|_| WorkerError::Stopped)
    }

    pub fn refresh(&self) -> Result<State, WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::Refresh(sender))
            .map_err(|_| WorkerError::Stopped)?;
        receiver
            .recv()
            .map_err(|_| WorkerError::Stopped)?
            .map_err(WorkerError::Service)
    }

    pub fn command(&self, control: ControlId, value: Value) -> Result<State, WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::Command {
                control,
                value,
                reply: sender,
            })
            .map_err(|_| WorkerError::Stopped)?;
        receiver
            .recv()
            .map_err(|_| WorkerError::Stopped)?
            .map_err(WorkerError::Service)
    }

    /// Executes one configured group command through serial device ownership.
    pub fn command_level_group(
        &self,
        group: String,
        position: u16,
    ) -> Result<(State, GroupResult), WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::LevelGroup {
                group,
                position,
                reply: sender,
            })
            .map_err(|_| WorkerError::Stopped)?;
        receiver.recv().map_err(|_| WorkerError::Stopped)?
    }

    pub fn stop(&self) -> Result<(), WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::Stop(sender))
            .map_err(|_| WorkerError::Stopped)?;
        receiver.recv().map_err(|_| WorkerError::Stopped)?;
        if let Some(thread) = self.thread.lock().map_err(|_| WorkerError::Stopped)?.take() {
            let _ = thread.join();
        }
        Ok(())
    }
}

fn run<D: Device>(
    mut service: Service<D>,
    receiver: Receiver<Request>,
    dashboard: DashboardConfig,
) {
    let mut last_health_check = Instant::now();
    let mut mirror_results = Vec::new();
    loop {
        let mut processed = 0;
        while processed < REQUEST_BATCH_LIMIT {
            let request = match receiver.try_recv() {
                Ok(request) => request,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            };
            processed += 1;
            if !handle_request(&mut service, &dashboard, &mut mirror_results, request) {
                return;
            }
        }

        if service.is_online() {
            let event_wait = if processed == REQUEST_BATCH_LIMIT {
                Duration::ZERO
            } else {
                EVENT_WAIT
            };
            let before = service.snapshot().values.clone();
            if service.wait_for_change(event_wait).unwrap_or(false) {
                mirror_results =
                    apply_mirrors(&mut service, &dashboard, &before).unwrap_or_default();
            }
        } else {
            thread::sleep(EVENT_WAIT);
        }
        if last_health_check.elapsed() >= HEALTH_CHECK_INTERVAL {
            let before = service.snapshot().values.clone();
            if service.refresh().is_ok() {
                mirror_results =
                    apply_mirrors(&mut service, &dashboard, &before).unwrap_or_default();
            }
            last_health_check = Instant::now();
        }
    }
}

fn handle_request<D: Device>(
    service: &mut Service<D>,
    dashboard: &DashboardConfig,
    mirror_results: &mut Vec<MirrorResult>,
    request: Request,
) -> bool {
    match request {
        Request::State(reply) => {
            let _ = reply.send(state(service, dashboard, mirror_results));
        }
        Request::Refresh(reply) => {
            let before = service.snapshot().values.clone();
            let _ = reply.send(service.refresh().map(|_| {
                *mirror_results = apply_mirrors(service, dashboard, &before).unwrap_or_default();
                state(service, dashboard, mirror_results)
            }));
        }
        Request::Command {
            control,
            value,
            reply,
        } => {
            let before = service.snapshot().values.clone();
            let _ = reply.send(service.command(&control, value).map(|_| {
                *mirror_results = apply_mirrors(service, dashboard, &before).unwrap_or_default();
                state(service, dashboard, mirror_results)
            }));
        }
        Request::LevelGroup {
            group,
            position,
            reply,
        } => {
            let before = service.snapshot().values.clone();
            let result = dashboard
                .validate_for(service.snapshot())
                .map_err(|error| WorkerError::Dashboard(error.to_string()))
                .and_then(|_| {
                    dashboard
                        .level_groups
                        .iter()
                        .find(|item| item.id == group)
                        .ok_or(WorkerError::UnknownGroup)
                        .and_then(|group| {
                            service
                                .command_level_group(&group.level_group(), position)
                                .map_err(WorkerError::Group)
                        })
                })
                .map(|result| {
                    *mirror_results =
                        apply_mirrors(service, dashboard, &before).unwrap_or_default();
                    (state(service, dashboard, mirror_results), result)
                });
            let _ = reply.send(result);
        }
        Request::Stop(reply) => {
            let _ = reply.send(());
            return false;
        }
    }
    true
}

fn apply_mirrors<D: Device>(
    service: &mut Service<D>,
    dashboard: &DashboardConfig,
    before: &BTreeMap<ControlId, Value>,
) -> Result<Vec<MirrorResult>, WorkerError> {
    dashboard
        .validate_for(service.snapshot())
        .map_err(|error| WorkerError::Dashboard(error.to_string()))?;
    let mut changed = service
        .snapshot()
        .values
        .iter()
        .filter_map(|(control, value)| {
            (before.get(control) != Some(value)).then_some(control.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut mirrored_sources = BTreeSet::new();
    let mut results = Vec::new();
    loop {
        let pending = dashboard
            .mirrors
            .iter()
            .filter(|mirror| {
                mirror.enabled
                    && changed.contains(&mirror.source)
                    && mirrored_sources.insert(mirror.source.clone())
            })
            .cloned()
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(results);
        }
        for mirror in pending {
            let result = service
                .command_mirror(&mirror.source, &mirror.target)
                .map_err(WorkerError::Group)?;
            if result.applied {
                changed.insert(mirror.target.clone());
            }
            results.push(result);
        }
    }
}

fn state<D: Device>(
    service: &Service<D>,
    dashboard: &DashboardConfig,
    mirror_results: &[MirrorResult],
) -> State {
    State {
        dashboard: dashboard.clone(),
        snapshot: service.snapshot().clone(),
        revision: service.revision(),
        online: service.is_online(),
        mirror_results: mirror_results.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::*;
    use crate::{
        ControlCapability, DeviceError, GroupCapability, GroupOperation, ValueDomain,
        dashboard_store::{DashboardLevelGroup, DashboardMirror},
        profile_store::Profiles,
    };

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

    struct SchemaDriftDevice {
        snapshot: DeviceSnapshot,
        drifted: bool,
        writes: Arc<Mutex<Vec<(ControlId, Value)>>>,
    }

    impl Device for SchemaDriftDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            let mut snapshot = self.snapshot.clone();
            if self.drifted {
                snapshot.capability_schema = "mock-v2".into();
            }
            self.drifted = true;
            Ok(snapshot)
        }

        fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
            self.writes.lock().unwrap().push((control.clone(), value));
            Ok(())
        }
    }

    struct EventDevice {
        snapshot: DeviceSnapshot,
        changed: bool,
    }

    struct QueuedEventDevice {
        snapshot: DeviceSnapshot,
        event_ready: Arc<AtomicBool>,
    }

    struct MirrorEventDevice {
        snapshot: DeviceSnapshot,
        changed: bool,
        source: ControlId,
    }

    impl Device for QueuedEventDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.snapshot.clone())
        }

        fn write(&mut self, _: &ControlId, _: Value) -> Result<(), DeviceError> {
            Ok(())
        }

        fn wait_for_change(&mut self, timeout: Duration) -> Result<bool, DeviceError> {
            if self.event_ready.swap(false, Ordering::SeqCst) {
                self.snapshot
                    .values
                    .insert(ControlId("direct-monitor".into()), Value::Bool(true));
                return Ok(true);
            }
            thread::sleep(timeout);
            Ok(false)
        }
    }

    impl Device for EventDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.snapshot.clone())
        }

        fn write(&mut self, _: &ControlId, _: Value) -> Result<(), DeviceError> {
            Ok(())
        }

        fn wait_for_change(&mut self, _: Duration) -> Result<bool, DeviceError> {
            if self.changed {
                return Ok(false);
            }
            self.changed = true;
            self.snapshot
                .values
                .insert(ControlId("direct-monitor".into()), Value::Bool(true));
            Ok(true)
        }
    }

    impl Device for MirrorEventDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.snapshot.clone())
        }

        fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
            self.snapshot.values.insert(control.clone(), value);
            Ok(())
        }

        fn wait_for_change(&mut self, _: Duration) -> Result<bool, DeviceError> {
            if self.changed {
                return Ok(false);
            }
            self.changed = true;
            self.snapshot
                .values
                .insert(self.source.clone(), Value::Integer(75));
            Ok(true)
        }
    }

    #[test]
    fn serial_worker_confirms_command_before_returning_state() {
        let volume = ControlId("output.volume".into());
        let worker = DeviceWorker::start(MockDevice(DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![ControlCapability {
                id: volume.clone(),
                domain: ValueDomain::Integer,
                writable: true,
                available: true,
                minimum: Some(0),
                maximum: Some(100),
                group: None,
                presentation: None,
            }],
            values: BTreeMap::from([(volume.clone(), Value::Integer(50))]),
        }))
        .unwrap();

        let state = worker.command(volume.clone(), Value::Integer(75)).unwrap();

        assert_eq!(state.snapshot.values[&volume], Value::Integer(75));
        assert_eq!(state.revision, 2);
        worker.stop().unwrap();
    }

    #[test]
    fn serial_worker_confirms_configured_group() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let snapshot = DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![
                ControlCapability {
                    id: first.clone(),
                    domain: ValueDomain::Integer,
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                    group: Some(GroupCapability {
                        operation: GroupOperation::RelativeLevel,
                    }),
                    presentation: None,
                },
                ControlCapability {
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
                },
            ],
            values: BTreeMap::from([
                (first.clone(), Value::Integer(50)),
                (second.clone(), Value::Integer(25)),
            ]),
        };
        let dashboard = DashboardConfig {
            version: 2,
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            controls: Vec::new(),
            level_groups: vec![DashboardLevelGroup {
                id: "linked".into(),
                label: "Linked".into(),
                members: vec![first.clone(), second.clone()],
                anchor: first.clone(),
            }],
            mirrors: Vec::new(),
        };
        let worker = DeviceWorker::start_with_dashboard(
            MockDevice(snapshot),
            Profiles::new(),
            Some(dashboard),
        )
        .unwrap();

        let (state, result) = worker.command_level_group("linked".into(), 750).unwrap();

        assert_eq!(result.applied, vec![first.clone(), second.clone()]);
        assert_eq!(state.snapshot.values[&first], Value::Integer(75));
        assert_eq!(state.snapshot.values[&second], Value::Integer(50));
        worker.stop().unwrap();
    }

    #[test]
    fn serial_worker_mirrors_confirmed_source_command() {
        let source = ControlId("output.volume".into());
        let target = ControlId("optical.volume".into());
        let snapshot = DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![source.clone(), target.clone()]
                .into_iter()
                .map(|id| ControlCapability {
                    id,
                    domain: ValueDomain::Integer,
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                    group: Some(GroupCapability {
                        operation: GroupOperation::RelativeLevel,
                    }),
                    presentation: None,
                })
                .collect(),
            values: BTreeMap::from([
                (source.clone(), Value::Integer(50)),
                (target.clone(), Value::Integer(25)),
            ]),
        };
        let worker = DeviceWorker::start_with_dashboard(
            MockDevice(snapshot),
            Profiles::new(),
            Some(DashboardConfig {
                version: 3,
                device_id: "mock-device".into(),
                capability_schema: "mock-v1".into(),
                controls: Vec::new(),
                level_groups: Vec::new(),
                mirrors: vec![DashboardMirror {
                    source: source.clone(),
                    target: target.clone(),
                    enabled: true,
                }],
            }),
        )
        .unwrap();

        let state = worker.command(source.clone(), Value::Integer(75)).unwrap();

        assert_eq!(state.snapshot.values[&source], Value::Integer(75));
        assert_eq!(state.snapshot.values[&target], Value::Integer(75));
        assert_eq!(state.mirror_results.len(), 1);
        assert!(state.mirror_results[0].applied);
        worker.stop().unwrap();
    }

    #[test]
    fn serial_worker_mirrors_confirmed_source_event() {
        let source = ControlId("output.volume".into());
        let target = ControlId("optical.volume".into());
        let snapshot = DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![source.clone(), target.clone()]
                .into_iter()
                .map(|id| ControlCapability {
                    id,
                    domain: ValueDomain::Integer,
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                    group: Some(GroupCapability {
                        operation: GroupOperation::RelativeLevel,
                    }),
                    presentation: None,
                })
                .collect(),
            values: BTreeMap::from([
                (source.clone(), Value::Integer(50)),
                (target.clone(), Value::Integer(25)),
            ]),
        };
        let worker = DeviceWorker::start_with_dashboard(
            MirrorEventDevice {
                snapshot,
                changed: false,
                source: source.clone(),
            },
            Profiles::new(),
            Some(DashboardConfig {
                version: 3,
                device_id: "mock-device".into(),
                capability_schema: "mock-v1".into(),
                controls: Vec::new(),
                level_groups: Vec::new(),
                mirrors: vec![DashboardMirror {
                    source,
                    target: target.clone(),
                    enabled: true,
                }],
            }),
        )
        .unwrap();

        for _ in 0..10 {
            let state = worker.state().unwrap();
            if state.snapshot.values[&target] == Value::Integer(75) {
                assert_eq!(state.mirror_results.len(), 1);
                worker.stop().unwrap();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        worker.stop().unwrap();
        panic!("event did not mirror");
    }

    #[test]
    fn schema_drift_rejects_persisted_group_without_writes() {
        let first = ControlId("output.volume".into());
        let second = ControlId("optical.volume".into());
        let snapshot = DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![
                ControlCapability {
                    id: first.clone(),
                    domain: ValueDomain::Integer,
                    writable: true,
                    available: true,
                    minimum: Some(0),
                    maximum: Some(100),
                    group: Some(GroupCapability {
                        operation: GroupOperation::RelativeLevel,
                    }),
                    presentation: None,
                },
                ControlCapability {
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
                },
            ],
            values: BTreeMap::from([
                (first.clone(), Value::Integer(50)),
                (second.clone(), Value::Integer(25)),
            ]),
        };
        let dashboard = DashboardConfig {
            version: 2,
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            controls: Vec::new(),
            level_groups: vec![DashboardLevelGroup {
                id: "linked".into(),
                label: "Linked".into(),
                members: vec![first, second],
                anchor: ControlId("output.volume".into()),
            }],
            mirrors: Vec::new(),
        };
        let writes = Arc::new(Mutex::new(Vec::new()));
        let worker = DeviceWorker::start_with_dashboard(
            SchemaDriftDevice {
                snapshot,
                drifted: false,
                writes: Arc::clone(&writes),
            },
            Profiles::new(),
            Some(dashboard),
        )
        .unwrap();

        worker.refresh().unwrap();
        assert!(matches!(
            worker.command_level_group("linked".into(), 750),
            Err(WorkerError::Dashboard(_))
        ));
        assert!(writes.lock().unwrap().is_empty());
        worker.stop().unwrap();
    }

    #[test]
    fn event_reconciles_without_a_refresh_request() {
        let control = ControlId("direct-monitor".into());
        let worker = DeviceWorker::start(EventDevice {
            snapshot: DeviceSnapshot {
                device_id: "mock-device".into(),
                capability_schema: "mock-v1".into(),
                capabilities: Vec::new(),
                values: BTreeMap::from([(control.clone(), Value::Bool(false))]),
            },
            changed: false,
        })
        .unwrap();

        for _ in 0..10 {
            let state = worker.state().unwrap();
            if state.snapshot.values[&control] == Value::Bool(true) {
                assert_eq!(state.revision, 2);
                worker.stop().unwrap();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }

        worker.stop().unwrap();
        panic!("event did not reconcile");
    }

    #[test]
    fn queued_requests_do_not_starve_event_reconciliation() {
        let control = ControlId("direct-monitor".into());
        let event_ready = Arc::new(AtomicBool::new(false));
        let worker = DeviceWorker::start(QueuedEventDevice {
            snapshot: DeviceSnapshot {
                device_id: "mock-device".into(),
                capability_schema: "mock-v1".into(),
                capabilities: Vec::new(),
                values: BTreeMap::from([(control.clone(), Value::Bool(false))]),
            },
            event_ready: event_ready.clone(),
        })
        .unwrap();
        let mut replies = Vec::with_capacity(QUEUE_LIMIT);
        for _ in 0..QUEUE_LIMIT {
            let (sender, receiver) = std::sync::mpsc::channel();
            worker.sender.send(Request::State(sender)).unwrap();
            replies.push(receiver);
        }
        event_ready.store(true, Ordering::SeqCst);

        let states = replies
            .into_iter()
            .map(|receiver| receiver.recv_timeout(Duration::from_secs(1)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            states[REQUEST_BATCH_LIMIT].snapshot.values[&control],
            Value::Bool(true)
        );
        assert_eq!(states[REQUEST_BATCH_LIMIT].revision, 2);
        worker.stop().unwrap();
    }
}
