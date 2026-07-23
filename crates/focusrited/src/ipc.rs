//! Bounded local Unix-socket API for local clients.

use std::{
    collections::VecDeque,
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    ControlId, DeviceSnapshot, ProfileApplyResult, ProfilePreview, ProfileReview, ServiceError,
    Value, dashboard_store::DashboardConfig, groups::GroupResult, worker::DeviceWorker,
};

const PROTOCOL_VERSION: u8 = 1;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_OUTBOUND_BYTES: usize = 64 * 1024;
const REQUEST_BATCH_LIMIT: usize = 4;
const LOOP_WAIT: Duration = Duration::from_millis(20);

pub struct LocalServer {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
}

impl LocalServer {
    pub fn start(worker: Arc<DeviceWorker>, path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_stale_socket(&path)?;
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660))?;
        listener.set_nonblocking(true)?;
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let thread_path = path.clone();
        let thread = thread::spawn(move || {
            run(listener, worker, thread_running);
            let _ = fs::remove_file(thread_path);
        });
        Ok(Self {
            running,
            thread: Some(thread),
            path,
        })
    }

    pub fn stop(mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(path) {
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "socket is already owned by a running daemon",
            )),
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => fs::remove_file(path),
            Err(error) => Err(error),
        },
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "socket path exists and is not a socket",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn run(listener: UnixListener, worker: Arc<DeviceWorker>, running: Arc<AtomicBool>) {
    let instance_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let mut clients = Vec::new();
    let mut last_revision = None;
    while running.load(Ordering::SeqCst) {
        accept_clients(&listener, &worker, &instance_id, &mut clients);
        if let Ok(state) = worker.event_state()
            && last_revision
                .replace(state.revision)
                .is_some_and(|revision| revision != state.revision)
        {
            broadcast(
                &mut clients,
                Outbound::Event(operation_state_message(&instance_id, "event", state)),
            );
        }
        clients.retain_mut(|client| {
            if client.closing {
                return flush_client(client) && client.has_pending_output();
            }
            let readable = read_client(client, &worker, &instance_id);
            let written = flush_client(client);
            readable && written && (!client.closing || client.has_pending_output())
        });
        thread::sleep(LOOP_WAIT);
    }
}

fn accept_clients(
    listener: &UnixListener,
    worker: &Arc<DeviceWorker>,
    instance_id: &str,
    clients: &mut Vec<Client>,
) {
    loop {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
            Err(_) => return,
        };
        if stream.set_nonblocking(true).is_err() {
            continue;
        }
        let mut client = Client::new(stream);
        let message = match worker.state() {
            Ok(state) => state_message(instance_id, "snapshot", state),
            Err(_) => error_message("worker_stopped"),
        };
        if client.queue(Outbound::Reply(message)) {
            clients.push(client);
        }
    }
}

fn read_client(client: &mut Client, worker: &Arc<DeviceWorker>, instance_id: &str) -> bool {
    let mut remaining = REQUEST_BATCH_LIMIT;
    if !process_requests(client, worker, instance_id, &mut remaining) {
        return false;
    }
    if client.input.contains(&b'\n') {
        return true;
    }
    if client.input.len() > MAX_MESSAGE_BYTES {
        return client.reject("message_too_large");
    }
    let mut bytes = [0; 4096];
    match client.stream.read(&mut bytes) {
        Ok(0) => return false,
        Ok(count) => {
            client.input.extend_from_slice(&bytes[..count]);
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
        Err(_) => return false,
    }
    if !process_requests(client, worker, instance_id, &mut remaining) {
        return false;
    }
    if !client.input.contains(&b'\n') && client.input.len() > MAX_MESSAGE_BYTES {
        return client.reject("message_too_large");
    }
    true
}

fn process_requests(
    client: &mut Client,
    worker: &Arc<DeviceWorker>,
    instance_id: &str,
    remaining: &mut usize,
) -> bool {
    while *remaining > 0 {
        let Some(end) = client.input.iter().position(|byte| *byte == b'\n') else {
            break;
        };
        if end + 1 > MAX_MESSAGE_BYTES {
            return client.reject("message_too_large");
        }
        let line = client.input.drain(..=end).collect::<Vec<_>>();
        let line = &line[..line.len() - 1];
        match serde_json::from_slice::<Request>(line) {
            Ok(request) if request.version == PROTOCOL_VERSION => {
                if !handle_request(client, worker, instance_id, request) {
                    return false;
                }
            }
            Ok(_) => {
                return client.reject("unsupported_version");
            }
            Err(_) => {
                return client.reject("malformed_message");
            }
        }
        *remaining -= 1;
        if client.closing {
            return true;
        }
    }
    true
}

fn handle_request(
    client: &mut Client,
    worker: &Arc<DeviceWorker>,
    instance_id: &str,
    request: Request,
) -> bool {
    let message = match request.kind.as_str() {
        "snapshot" => match worker.state() {
            Ok(state) => state_message(instance_id, "snapshot", state),
            Err(_) => error_message("worker_stopped"),
        },
        "command" => match (request.control, request.value) {
            (Some(control), Some(value)) => match worker.command(ControlId(control), value) {
                Ok(state) => operation_state_message(instance_id, "command_result", state),
                Err(error) => error_message(service_error(error)),
            },
            _ => error_message("invalid_command"),
        },
        "group_command" => match (request.group, request.position) {
            (Some(group), Some(position)) => {
                match worker.command_level_group(group.clone(), position) {
                    Ok((state, result)) => group_state_message(instance_id, state, group, result),
                    Err(error) => error_message(service_error(error)),
                }
            }
            _ => error_message("invalid_group_command"),
        },
        "profile_save" => match request.name {
            Some(name) => match worker.save_profile(name) {
                Ok(profiles) => {
                    profile_message("profile_save_result", ProfileResult::Saved { profiles })
                }
                Err(error) => error_message(service_error(error)),
            },
            None => error_message("invalid_profile_save"),
        },
        "profile_list" => match worker.profiles() {
            Ok(profiles) => {
                profile_message("profile_list_result", ProfileResult::Listed { profiles })
            }
            Err(error) => error_message(service_error(error)),
        },
        "profile_review" => match request.name {
            Some(name) => match worker.review_profile(name) {
                Ok(preview) => {
                    profile_message("profile_review_result", ProfileResult::Reviewed { preview })
                }
                Err(error) => error_message(service_error(error)),
            },
            None => error_message("invalid_profile_review"),
        },
        "profile_apply" => match (request.name, request.review) {
            (Some(name), Some(review)) => match worker.apply_profile(name, review) {
                Ok((state, result)) => profile_state_message(
                    instance_id,
                    "profile_apply_result",
                    state,
                    ProfileResult::Applied { result },
                ),
                Err(error) => error_message(service_error(error)),
            },
            _ => error_message("invalid_profile_apply"),
        },
        _ => error_message("unknown_message_type"),
    };
    if client.queue(Outbound::Reply(message)) {
        true
    } else {
        client.closing = true;
        true
    }
}

fn flush_client(client: &mut Client) -> bool {
    loop {
        if let Some(message) = client.outbound.front_mut() {
            match client.stream.write(&message.bytes[message.offset..]) {
                Ok(0) => return false,
                Ok(count) => message.offset += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return true,
                Err(_) => return false,
            }
            if let Some(message) = client
                .outbound
                .pop_front_if(|message| message.offset == message.bytes.len())
            {
                client.outbound_bytes -= message.bytes.len();
            }
            continue;
        }
        if let Some(error) = client.rejection.take() {
            if !client.queue(Outbound::Reply(error_message(error))) {
                return false;
            }
            continue;
        }
        return true;
    }
}

fn broadcast(clients: &mut [Client], message: Outbound) {
    for client in clients {
        if client.closing {
            continue;
        }
        if !client.queue(message.clone()) {
            client.closing = true;
        }
    }
}

struct Client {
    stream: UnixStream,
    input: Vec<u8>,
    outbound: VecDeque<Message>,
    outbound_bytes: usize,
    closing: bool,
    rejection: Option<&'static str>,
}

impl Client {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            input: Vec::new(),
            outbound: VecDeque::new(),
            outbound_bytes: 0,
            closing: false,
            rejection: None,
        }
    }

    fn queue(&mut self, outbound: Outbound) -> bool {
        let bytes = match serde_json::to_vec(outbound.message()) {
            Ok(mut bytes) => {
                bytes.push(b'\n');
                bytes
            }
            Err(_) => return false,
        };
        if bytes.len() > MAX_MESSAGE_BYTES {
            return false;
        }
        if matches!(outbound, Outbound::Event(_))
            && let Some(message) = self
                .outbound
                .iter_mut()
                .find(|message| message.event && message.offset == 0)
        {
            self.outbound_bytes -= message.bytes.len();
            message.bytes = bytes;
            message.offset = 0;
            self.outbound_bytes += message.bytes.len();
            return self.outbound_bytes <= MAX_OUTBOUND_BYTES;
        }
        if self.outbound_bytes + bytes.len() > MAX_OUTBOUND_BYTES {
            return false;
        }
        self.outbound_bytes += bytes.len();
        self.outbound.push_back(Message {
            bytes,
            offset: 0,
            event: matches!(outbound, Outbound::Event(_)),
        });
        true
    }

    fn reject(&mut self, error: &'static str) -> bool {
        self.closing = true;
        self.rejection = Some(error);
        true
    }

    fn has_pending_output(&self) -> bool {
        !self.outbound.is_empty() || self.rejection.is_some()
    }
}

struct Message {
    bytes: Vec<u8>,
    offset: usize,
    event: bool,
}

#[derive(Clone)]
enum Outbound {
    Event(Response),
    Reply(Response),
}

impl Outbound {
    fn message(&self) -> &Response {
        match self {
            Self::Event(message) | Self::Reply(message) => message,
        }
    }
}

#[derive(Deserialize)]
struct Request {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: String,
    control: Option<String>,
    value: Option<Value>,
    group: Option<String>,
    position: Option<u16>,
    name: Option<String>,
    review: Option<ProfileReview>,
}

#[derive(Clone, Serialize)]
struct Response {
    v: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<DeviceSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dashboard: Option<DashboardConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_result: Option<GroupCommandResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_result: Option<ProfileResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mirror_results: Option<Vec<MirrorCommandResult>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
enum ProfileResult {
    Saved { profiles: Vec<String> },
    Listed { profiles: Vec<String> },
    Reviewed { preview: ProfilePreview },
    Applied { result: ProfileApplyResult },
}

#[derive(Clone, Serialize)]
struct GroupCommandResult {
    group: String,
    applied: Vec<ControlId>,
    skipped: Vec<ControlId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<GroupFailure>,
}

#[derive(Clone, Serialize)]
struct GroupFailure {
    control: ControlId,
    error: &'static str,
}

#[derive(Clone, Serialize)]
struct MirrorCommandResult {
    source: ControlId,
    target: ControlId,
    applied: bool,
    skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<GroupFailure>,
}

fn state_message(instance_id: &str, kind: &'static str, state: crate::worker::State) -> Response {
    Response {
        v: PROTOCOL_VERSION,
        kind,
        instance_id: Some(instance_id.into()),
        revision: Some(state.revision),
        online: Some(state.online),
        snapshot: Some(state.snapshot),
        dashboard: Some(state.dashboard),
        error: None,
        group_result: None,
        profile_result: None,
        mirror_results: None,
    }
}

fn operation_state_message(
    instance_id: &str,
    kind: &'static str,
    state: crate::worker::State,
) -> Response {
    let mirror_results = (!state.mirror_results.is_empty()).then(|| {
        state
            .mirror_results
            .iter()
            .cloned()
            .map(|result| {
                let target = result.target;
                MirrorCommandResult {
                    source: result.source,
                    target: target.clone(),
                    applied: result.applied,
                    skipped: result.skipped,
                    failed: result.failed.map(|error| GroupFailure {
                        control: target,
                        error: group_service_error(error),
                    }),
                }
            })
            .collect()
    });
    let mut response = state_message(instance_id, kind, state);
    response.mirror_results = mirror_results;
    response
}

fn error_message(error: &'static str) -> Response {
    Response {
        v: PROTOCOL_VERSION,
        kind: "error",
        instance_id: None,
        revision: None,
        online: None,
        snapshot: None,
        dashboard: None,
        error: Some(error),
        group_result: None,
        profile_result: None,
        mirror_results: None,
    }
}

fn profile_message(kind: &'static str, profile_result: ProfileResult) -> Response {
    Response {
        v: PROTOCOL_VERSION,
        kind,
        instance_id: None,
        revision: None,
        online: None,
        snapshot: None,
        dashboard: None,
        error: None,
        group_result: None,
        profile_result: Some(profile_result),
        mirror_results: None,
    }
}

fn profile_state_message(
    instance_id: &str,
    kind: &'static str,
    state: crate::worker::State,
    profile_result: ProfileResult,
) -> Response {
    let mut response = operation_state_message(instance_id, kind, state);
    response.profile_result = Some(profile_result);
    response
}

fn group_state_message(
    instance_id: &str,
    state: crate::worker::State,
    group: String,
    result: GroupResult,
) -> Response {
    let mut response = operation_state_message(instance_id, "group_command_result", state);
    response.group_result = Some(GroupCommandResult {
        group,
        applied: result.applied,
        skipped: result.skipped,
        failed: result.failed.map(|(control, error)| GroupFailure {
            control,
            error: group_service_error(error),
        }),
    });
    response
}

fn group_service_error(error: ServiceError) -> &'static str {
    match error {
        ServiceError::Device(_) => "device_error",
        ServiceError::UnknownControl => "unknown_control",
        ServiceError::Unavailable => "unavailable",
        ServiceError::ReadOnly => "read_only",
        ServiceError::InvalidValue => "invalid_value",
        ServiceError::InvalidProfileName => "invalid_profile_name",
        ServiceError::UnconfirmedWrite => "unconfirmed_write",
        ServiceError::UnknownProfile => "unknown_profile",
        ServiceError::ProfileBindingMismatch => "profile_binding_mismatch",
        ServiceError::ProfileReviewMismatch => "profile_review_mismatch",
    }
}

fn service_error(error: crate::worker::WorkerError) -> &'static str {
    match error {
        crate::worker::WorkerError::Dashboard(_) => "dashboard_invalid",
        crate::worker::WorkerError::Group(_) => "invalid_group_command",
        crate::worker::WorkerError::UnknownGroup => "unknown_group",
        crate::worker::WorkerError::ProfileStore(_) => "profile_store_error",
        crate::worker::WorkerError::Stopped => "worker_stopped",
        crate::worker::WorkerError::Service(ServiceError::Device(_)) => "device_error",
        crate::worker::WorkerError::Service(ServiceError::UnknownControl) => "unknown_control",
        crate::worker::WorkerError::Service(ServiceError::Unavailable) => "unavailable",
        crate::worker::WorkerError::Service(ServiceError::ReadOnly) => "read_only",
        crate::worker::WorkerError::Service(ServiceError::InvalidValue) => "invalid_value",
        crate::worker::WorkerError::Service(ServiceError::UnconfirmedWrite) => "unconfirmed_write",
        crate::worker::WorkerError::Service(ServiceError::InvalidProfileName) => {
            "invalid_profile_name"
        }
        crate::worker::WorkerError::Service(ServiceError::UnknownProfile) => "unknown_profile",
        crate::worker::WorkerError::Service(ServiceError::ProfileBindingMismatch) => {
            "profile_binding_mismatch"
        }
        crate::worker::WorkerError::Service(ServiceError::ProfileReviewMismatch) => {
            "profile_review_mismatch"
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        io::{BufRead, BufReader},
        sync::atomic::AtomicBool,
        time::Duration,
    };

    use super::*;
    use crate::{
        ControlCapability, ControlPresentation, Device, DeviceError, PresentationKind, ValueDomain,
        dashboard_store::{DashboardControl, DashboardLevelGroup},
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

    struct EventDevice {
        snapshot: DeviceSnapshot,
        event_ready: Arc<AtomicBool>,
    }

    impl Device for EventDevice {
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
                    .insert(ControlId("output.level".into()), Value::Integer(75));
                return Ok(true);
            }
            thread::sleep(timeout);
            Ok(false)
        }
    }

    fn server() -> (LocalServer, Arc<DeviceWorker>, PathBuf) {
        let control = ControlId("output.level".into());
        let worker = Arc::new(
            DeviceWorker::start(MockDevice(DeviceSnapshot {
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
                    presentation: Some(ControlPresentation {
                        label: "KEF level".into(),
                        kind: PresentationKind::Level,
                        default_dashboard_order: Some(1),
                        companion: None,
                        step: None,
                    }),
                }],
                values: BTreeMap::from([(control, Value::Integer(50))]),
            }))
            .unwrap(),
        );
        let path = std::env::temp_dir().join(format!(
            "focusrited-ipc-test-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let server = LocalServer::start(Arc::clone(&worker), path.clone()).unwrap();
        (server, worker, path)
    }

    fn event_server() -> (LocalServer, Arc<DeviceWorker>, PathBuf, Arc<AtomicBool>) {
        let event_ready = Arc::new(AtomicBool::new(false));
        let control = ControlId("output.level".into());
        let worker = Arc::new(
            DeviceWorker::start(EventDevice {
                snapshot: DeviceSnapshot {
                    device_id: "mock-device".into(),
                    capability_schema: "mock-v1".into(),
                    capabilities: vec![ControlCapability {
                        id: control.clone(),
                        domain: ValueDomain::Integer,
                        writable: false,
                        available: true,
                        minimum: Some(0),
                        maximum: Some(100),
                        group: None,
                        presentation: None,
                    }],
                    values: BTreeMap::from([(control, Value::Integer(50))]),
                },
                event_ready: Arc::clone(&event_ready),
            })
            .unwrap(),
        );
        let path = std::env::temp_dir().join(format!(
            "focusrited-ipc-event-test-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let server = LocalServer::start(Arc::clone(&worker), path.clone()).unwrap();
        (server, worker, path, event_ready)
    }

    fn group_server() -> (LocalServer, Arc<DeviceWorker>, PathBuf) {
        let first = ControlId("output.level".into());
        let second = ControlId("optical.level".into());
        let worker = Arc::new(
            DeviceWorker::start_with_dashboard(
                MockDevice(DeviceSnapshot {
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
                            group: Some(crate::GroupCapability {
                                operation: crate::GroupOperation::RelativeLevel,
                            }),
                            presentation: Some(ControlPresentation {
                                label: "Output level".into(),
                                kind: PresentationKind::Level,
                                default_dashboard_order: Some(1),
                                companion: None,
                                step: None,
                            }),
                        },
                        ControlCapability {
                            id: second.clone(),
                            domain: ValueDomain::Integer,
                            writable: true,
                            available: true,
                            minimum: Some(0),
                            maximum: Some(100),
                            group: Some(crate::GroupCapability {
                                operation: crate::GroupOperation::RelativeLevel,
                            }),
                            presentation: None,
                        },
                    ],
                    values: BTreeMap::from([
                        (first.clone(), Value::Integer(50)),
                        (second.clone(), Value::Integer(25)),
                    ]),
                }),
                Profiles::new(),
                Some(DashboardConfig {
                    version: 2,
                    device_id: "mock-device".into(),
                    capability_schema: "mock-v1".into(),
                    controls: vec![DashboardControl {
                        id: first.clone(),
                        label: None,
                    }],
                    level_groups: vec![DashboardLevelGroup {
                        id: "linked".into(),
                        label: "Linked".into(),
                        members: vec![first.clone(), second],
                        anchor: first,
                    }],
                    mirrors: Vec::new(),
                }),
            )
            .unwrap(),
        );
        let path = std::env::temp_dir().join(format!(
            "focusrited-ipc-group-test-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let server = LocalServer::start(Arc::clone(&worker), path.clone()).unwrap();
        (server, worker, path)
    }

    fn read_json(reader: &mut BufReader<UnixStream>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn read_type(reader: &mut BufReader<UnixStream>, expected: &str) -> serde_json::Value {
        for _ in 0..4 {
            let message = read_json(reader);
            if message["type"] == expected {
                return message;
            }
        }
        panic!("did not receive {expected}");
    }

    #[test]
    fn snapshots_then_confirms_commands_over_local_socket() {
        let (server, worker, path) = server();
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        let snapshot = read_json(&mut reader);
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["revision"], 1);
        assert_eq!(
            snapshot["snapshot"]["capabilities"][0]["presentation"]["label"],
            "KEF level"
        );
        assert_eq!(
            snapshot["snapshot"]["capabilities"][0]["presentation"]["kind"],
            "level"
        );
        stream
            .write_all(
                b"{\"v\":1,\"type\":\"command\",\"control\":\"output.level\",\"value\":{\"type\":\"integer\",\"value\":75}}\n",
            )
            .unwrap();
        let result = read_type(&mut reader, "command_result");
        assert_eq!(result["revision"], 2);
        assert_eq!(result["snapshot"]["values"]["output.level"]["value"], 75);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn profile_requests_require_current_review_and_return_confirmed_state() {
        let (server, worker, path) = server();
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let _ = read_type(&mut reader, "snapshot");

        stream
            .write_all(b"{\"v\":1,\"type\":\"profile_save\",\"name\":\"desk\"}\n")
            .unwrap();
        assert_eq!(
            read_type(&mut reader, "profile_save_result")["profile_result"]["profiles"],
            serde_json::json!(["desk"])
        );
        stream
            .write_all(
                b"{\"v\":1,\"type\":\"command\",\"control\":\"output.level\",\"value\":{\"type\":\"integer\",\"value\":75}}\n",
            )
            .unwrap();
        let _ = read_type(&mut reader, "command_result");
        stream
            .write_all(b"{\"v\":1,\"type\":\"profile_review\",\"name\":\"desk\"}\n")
            .unwrap();
        let preview = read_type(&mut reader, "profile_review_result");
        let review = preview["profile_result"]["preview"]["review"].clone();
        assert_eq!(preview["profile_result"]["preview"]["binding"], "match");

        serde_json::to_writer(
            &mut stream,
            &serde_json::json!({
                "v": 1,
                "type": "profile_apply",
                "name": "desk",
                "review": review,
            }),
        )
        .unwrap();
        stream.write_all(b"\n").unwrap();
        let applied = read_type(&mut reader, "profile_apply_result");
        assert_eq!(applied["snapshot"]["values"]["output.level"]["value"], 50);
        assert_eq!(
            applied["profile_result"]["result"]["entries"][0]["status"],
            "applied"
        );

        serde_json::to_writer(
            &mut stream,
            &serde_json::json!({
                "v": 1,
                "type": "profile_apply",
                "name": "desk",
                "review": preview["profile_result"]["preview"]["review"],
            }),
        )
        .unwrap();
        stream.write_all(b"\n").unwrap();
        assert_eq!(
            read_type(&mut reader, "error")["error"],
            "profile_review_mismatch"
        );

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn group_commands_return_confirmed_member_results() {
        let (server, worker, path) = group_server();
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let _ = read_type(&mut reader, "snapshot");

        stream
            .write_all(
                b"{\"v\":1,\"type\":\"group_command\",\"group\":\"linked\",\"position\":750}\n",
            )
            .unwrap();
        let result = read_type(&mut reader, "group_command_result");
        assert_eq!(result["group_result"]["group"], "linked");
        assert_eq!(
            result["group_result"]["applied"],
            serde_json::json!(["output.level", "optical.level"])
        );
        assert_eq!(result["snapshot"]["values"]["output.level"]["value"], 75);
        assert_eq!(result["snapshot"]["values"]["optical.level"]["value"], 50);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn two_clients_converge_on_confirmed_state() {
        let (server, worker, path) = server();
        let mut first = UnixStream::connect(&path).unwrap();
        let mut second = UnixStream::connect(&path).unwrap();
        for stream in [&first, &second] {
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
        }
        let mut first_reader = BufReader::new(first.try_clone().unwrap());
        let mut second_reader = BufReader::new(second.try_clone().unwrap());
        let _ = read_type(&mut first_reader, "snapshot");
        let _ = read_type(&mut second_reader, "snapshot");

        first
            .write_all(
                b"{\"v\":1,\"type\":\"command\",\"control\":\"output.level\",\"value\":{\"type\":\"integer\",\"value\":60}}\n",
            )
            .unwrap();
        assert_eq!(
            read_type(&mut first_reader, "command_result")["revision"],
            2
        );
        assert_eq!(read_type(&mut first_reader, "event")["revision"], 2);
        assert_eq!(read_type(&mut second_reader, "event")["revision"], 2);
        second
            .write_all(
                b"{\"v\":1,\"type\":\"command\",\"control\":\"output.level\",\"value\":{\"type\":\"integer\",\"value\":80}}\n",
            )
            .unwrap();
        assert_eq!(
            read_type(&mut second_reader, "command_result")["revision"],
            3
        );
        let event = read_type(&mut first_reader, "event");
        assert_eq!(event["revision"], 3);
        assert_eq!(event["snapshot"]["values"]["output.level"]["value"], 80);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn external_worker_event_reaches_local_client() {
        let (server, worker, path, event_ready) = event_server();
        let stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut reader = BufReader::new(stream);
        let _ = read_type(&mut reader, "snapshot");
        event_ready.store(true, Ordering::SeqCst);

        let event = read_type(&mut reader, "event");
        assert_eq!(event["revision"], 2);
        assert_eq!(event["snapshot"]["values"]["output.level"]["value"], 75);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn unsent_events_coalesce_and_queue_overflow_rejects_client() {
        let (stream, _) = UnixStream::pair().unwrap();
        let mut client = Client::new(stream);
        let event = |revision| {
            Outbound::Event(Response {
                v: PROTOCOL_VERSION,
                kind: "event",
                instance_id: Some("test".into()),
                revision: Some(revision),
                online: Some(true),
                snapshot: None,
                dashboard: None,
                error: None,
                group_result: None,
                profile_result: None,
                mirror_results: None,
            })
        };
        assert!(client.queue(event(1)));
        assert!(client.queue(event(2)));
        assert_eq!(client.outbound.len(), 1);
        assert!(
            client.outbound[0]
                .bytes
                .windows(12)
                .any(|bytes| bytes == b"\"revision\":2")
        );

        let large_reply = || {
            Outbound::Reply(Response {
                v: PROTOCOL_VERSION,
                kind: "snapshot",
                instance_id: Some("x".repeat(8 * 1024)),
                revision: None,
                online: None,
                snapshot: None,
                dashboard: None,
                error: None,
                group_result: None,
                profile_result: None,
                mirror_results: None,
            })
        };
        while client.queue(large_reply()) {}
        assert!(client.outbound_bytes <= MAX_OUTBOUND_BYTES);

        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut full_client = Client::new(stream);
        full_client.outbound.push_back(Message {
            bytes: vec![b'x'; MAX_OUTBOUND_BYTES],
            offset: 0,
            event: false,
        });
        full_client.outbound_bytes = MAX_OUTBOUND_BYTES;
        let mut clients = vec![full_client];
        broadcast(&mut clients, event(3));
        assert!(clients[0].closing);
        let queued = clients[0].outbound_bytes;
        broadcast(&mut clients, event(4));
        assert_eq!(clients[0].outbound_bytes, queued);
    }

    #[test]
    fn reply_overflow_stops_remaining_commands() {
        let (server, worker, _) = server();
        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut client = Client::new(stream);
        client.outbound_bytes = MAX_OUTBOUND_BYTES;
        client.input = b"{\"v\":1,\"type\":\"command\",\"control\":\"output.level\",\"value\":{\"type\":\"integer\",\"value\":60}}\n{\"v\":1,\"type\":\"command\",\"control\":\"output.level\",\"value\":{\"type\":\"integer\",\"value\":80}}\n".to_vec();
        let mut remaining = REQUEST_BATCH_LIMIT;

        assert!(process_requests(
            &mut client,
            &worker,
            "test",
            &mut remaining
        ));
        assert!(client.closing);
        assert_eq!(
            worker.state().unwrap().snapshot.values[&ControlId("output.level".into())],
            Value::Integer(60)
        );

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn valid_pipelined_requests_are_limited_per_message() {
        let (server, worker, _) = server();
        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut client = Client::new(stream);
        client.input = b"{\"v\":1,\"type\":\"snapshot\"}\n".repeat(3_000);

        assert!(read_client(&mut client, &worker, "test"));
        assert!(!client.closing);
        assert!(client.input.len() > MAX_MESSAGE_BYTES);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn completed_frame_does_not_count_against_next_partial_frame() {
        let (server, worker, _) = server();
        let (stream, mut peer) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut client = Client::new(stream);
        let prefix = "{\"v\":1,\"type\":\"snapshot\",\"padding\":\"";
        let suffix = "\"}";
        let frame = format!(
            "{prefix}{}{suffix}",
            "x".repeat(MAX_MESSAGE_BYTES - 1 - prefix.len() - suffix.len())
        );
        let split = frame.len() - 4;
        client.input = frame.as_bytes()[..split].to_vec();
        peer.write_all(&frame.as_bytes()[split..]).unwrap();
        peer.write_all(b"\n{\"v\":1,\"type\":\"snapshot\"").unwrap();

        assert!(read_client(&mut client, &worker, "test"));
        assert!(!client.closing);
        assert!(client.input.len() < MAX_MESSAGE_BYTES);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn rejection_waits_for_queued_output_before_disconnect() {
        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut client = Client::new(stream);
        assert!(client.queue(Outbound::Reply(Response {
            v: PROTOCOL_VERSION,
            kind: "snapshot",
            instance_id: None,
            revision: None,
            online: None,
            snapshot: None,
            dashboard: None,
            error: None,
            group_result: None,
            profile_result: None,
            mirror_results: None,
        })));
        assert!(client.reject("malformed_message"));
        assert!(client.has_pending_output());
        assert!(flush_client(&mut client));
        assert!(!client.has_pending_output());
    }

    #[test]
    fn profile_result_is_additive_v1_json() {
        let response = profile_message(
            "profile_list_result",
            ProfileResult::Listed {
                profiles: vec!["desk".into()],
            },
        );

        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({
                "v": 1,
                "type": "profile_list_result",
                "profile_result": {"status": "listed", "profiles": ["desk"]},
            })
        );
    }

    #[test]
    fn malformed_message_gets_error_then_disconnect() {
        let (server, worker, path) = server();
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let _ = read_json(&mut reader);
        stream.write_all(b"not json\n").unwrap();
        let error = read_json(&mut reader);
        assert_eq!(error["type"], "error");
        assert_eq!(error["error"], "malformed_message");
        let mut line = String::new();
        assert_eq!(reader.read_line(&mut line).unwrap(), 0);

        server.stop();
        worker.stop().unwrap();
    }

    #[test]
    fn startup_keeps_live_daemon_socket_intact() {
        let (server, worker, path) = server();
        assert!(LocalServer::start(Arc::clone(&worker), path.clone()).is_err());
        assert!(UnixStream::connect(&path).is_ok());

        server.stop();
        worker.stop().unwrap();
    }
}
