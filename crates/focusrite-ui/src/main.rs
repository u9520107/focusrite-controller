use std::{
    collections::BTreeMap,
    env,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use focusrited::{
    ControlCapability, ControlId, ControlPresentation, DeviceSnapshot, PresentationKind, Value,
    ValueDomain,
    dashboard_store::{DashboardConfig, DashboardControl},
};
use serde::Deserialize;

const DASHBOARD_LIMIT: usize = 12;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const COMMAND_INTERVAL: Duration = Duration::from_millis(33);
const DEFAULT_AUTO_LOCK_AFTER: Duration = Duration::from_secs(60);
const UNLOCK_TARGET_SIZE: f32 = 64.0;
const UNLOCK_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> eframe::Result<()> {
    let (sender, receiver) = mpsc::channel();
    let demo = env::var_os("FOCUSRITE_UI_DEMO").is_some();
    let snapshot_sender = (!demo).then(|| spawn_socket_reader(sender, socket_path()));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([800.0, 480.0])
            .with_title("Focusrite Controller"),
        ..Default::default()
    };
    eframe::run_native(
        "Focusrite Controller",
        options,
        Box::new(|_| {
            Ok(Box::new(TouchscreenApp::new(
                receiver,
                snapshot_sender,
                demo,
            )))
        }),
    )
}

fn socket_path() -> PathBuf {
    env::var_os("FOCUSRITE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/focusrited/focusrited.sock"))
}

fn spawn_socket_reader(
    sender: mpsc::Sender<SocketEvent>,
    socket_path: PathBuf,
) -> mpsc::Sender<ReaderRequest> {
    let (request_sender, request_receiver) = mpsc::channel();
    thread::spawn(move || {
        'connection: loop {
            let stream = match UnixStream::connect(&socket_path) {
                Ok(stream) => stream,
                Err(_) => {
                    let _ = sender.send(SocketEvent::Disconnected);
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };
            let _ = sender.send(SocketEvent::Connected);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let mut writer = match stream.try_clone() {
                Ok(stream) => stream,
                Err(_) => continue,
            };
            if write_request(&mut writer, ReaderRequest::Snapshot).is_err() {
                continue;
            }
            let mut reader = BufReader::new(stream);
            loop {
                while let Ok(request) = request_receiver.try_recv() {
                    if write_request(&mut writer, request).is_err() {
                        continue 'connection;
                    }
                }
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => match serde_json::from_str::<Response>(&line) {
                        Ok(response) => {
                            let _ = sender.send(SocketEvent::Response(Box::new(response)));
                        }
                        Err(_) => {
                            let _ = sender.send(SocketEvent::Malformed);
                        }
                    },
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => break,
                }
            }
        }
    });
    request_sender
}

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: String,
    instance_id: Option<String>,
    revision: Option<u64>,
    online: Option<bool>,
    snapshot: Option<DeviceSnapshot>,
    dashboard: Option<DashboardConfig>,
    error: Option<String>,
}

enum SocketEvent {
    Connected,
    Disconnected,
    Malformed,
    Response(Box<Response>),
}

enum ReaderRequest {
    Snapshot,
    Command(ControlId, Value),
}

fn write_request(writer: &mut UnixStream, request: ReaderRequest) -> std::io::Result<()> {
    match request {
        ReaderRequest::Snapshot => writer.write_all(b"{\"v\":1,\"type\":\"snapshot\"}\n"),
        ReaderRequest::Command(control, value) => {
            serde_json::to_writer(
                &mut *writer,
                &serde_json::json!({
                    "v": 1,
                    "type": "command",
                    "control": control.0,
                    "value": value,
                }),
            )
            .map_err(std::io::Error::other)?;
            writer.write_all(b"\n")
        }
    }
}

struct TouchscreenApp {
    receiver: Receiver<SocketEvent>,
    snapshot_sender: Option<mpsc::Sender<ReaderRequest>>,
    connection: Connection,
    state: Option<ConfirmedState>,
    focus: Option<ControlId>,
    close_focus_next_frame: bool,
    demo: bool,
    read_only: bool,
    locked: bool,
    unlock_step: usize,
    unlock_started: Option<Instant>,
    auto_lock_after: Option<Duration>,
    last_touch: Instant,
    review: bool,
    demo_status: DemoStatus,
    demo_toast: Option<&'static str>,
    calibration_targets: bool,
    calibration: Option<CalibrationReview>,
    demo_values: BTreeMap<ControlId, Value>,
    pending_commands: BTreeMap<ControlId, Value>,
    last_command: BTreeMap<ControlId, Instant>,
    toast: Option<(String, Instant)>,
    debug: Option<DebugLog>,
}

enum Connection {
    Connecting,
    Connected,
    Offline,
    ProtocolError,
}

enum DemoStatus {
    Online,
    Offline,
    Error,
}

struct DebugLog(File);

struct StripState<'a> {
    interactive: bool,
    demo: bool,
    overrides: &'a mut BTreeMap<ControlId, Value>,
    pending_commands: &'a mut BTreeMap<ControlId, Value>,
}

struct DashboardStrip<'a> {
    capability: &'a ControlCapability,
    configured: &'a DashboardControl,
}

struct CalibrationReview {
    next: usize,
}

const CALIBRATION_POINTS: [egui::Pos2; 4] = [
    egui::pos2(70.0, 85.0),
    egui::pos2(730.0, 85.0),
    egui::pos2(730.0, 395.0),
    egui::pos2(70.0, 395.0),
];

struct ConfirmedState {
    instance_id: String,
    revision: u64,
    online: bool,
    snapshot: DeviceSnapshot,
    dashboard: DashboardConfig,
}

impl TouchscreenApp {
    fn new(
        receiver: Receiver<SocketEvent>,
        snapshot_sender: Option<mpsc::Sender<ReaderRequest>>,
        demo: bool,
    ) -> Self {
        let mut debug = debug_log();
        log_debug(&mut debug, "focusrite-ui debug started");
        Self {
            receiver,
            snapshot_sender,
            connection: if demo {
                Connection::Connected
            } else {
                Connection::Connecting
            },
            state: demo.then(demo_state),
            focus: None,
            close_focus_next_frame: false,
            demo,
            read_only: !demo && env::var_os("FOCUSRITE_UI_READ_ONLY").is_some(),
            locked: env::var_os("FOCUSRITE_UI_LOCK_ON_START").is_some(),
            unlock_step: 0,
            unlock_started: None,
            auto_lock_after: timeout_env("FOCUSRITE_UI_AUTO_LOCK_AFTER"),
            last_touch: Instant::now(),
            review: demo && env::var_os("FOCUSRITE_UI_REVIEW").is_some(),
            demo_status: DemoStatus::Online,
            demo_toast: None,
            calibration_targets: false,
            calibration: env::var_os("FOCUSRITE_UI_CALIBRATION")
                .map(|_| CalibrationReview { next: 0 }),
            demo_values: BTreeMap::new(),
            pending_commands: BTreeMap::new(),
            last_command: BTreeMap::new(),
            toast: None,
            debug,
        }
    }

    fn receive(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                SocketEvent::Connected => self.connection = Connection::Connected,
                SocketEvent::Disconnected => {
                    self.connection = Connection::Offline;
                    self.state = None;
                    self.focus = None;
                }
                SocketEvent::Malformed => {
                    self.connection = Connection::ProtocolError;
                    self.state = None;
                }
                SocketEvent::Response(response) => self.apply(*response),
            }
        }
    }

    fn apply(&mut self, response: Response) {
        if response.version != 1 {
            self.connection = Connection::ProtocolError;
            self.state = None;
            return;
        }
        if let Some(error) = response.error {
            self.toast = Some((error, Instant::now()));
            return;
        }
        let (Some(instance_id), Some(revision), Some(online), Some(snapshot), Some(dashboard)) = (
            response.instance_id,
            response.revision,
            response.online,
            response.snapshot,
            response.dashboard,
        ) else {
            return;
        };
        if !matches!(
            response.kind.as_str(),
            "snapshot" | "event" | "command_result"
        ) {
            return;
        }
        let resync = self.state.as_ref().is_some_and(|state| {
            state.instance_id != instance_id || revision > state.revision.saturating_add(1)
        });
        if resync {
            self.state = None;
            self.focus = None;
            if let Some(sender) = &self.snapshot_sender {
                let _ = sender.send(ReaderRequest::Snapshot);
            }
            return;
        }
        self.state = Some(ConfirmedState {
            instance_id,
            revision,
            online,
            snapshot,
            dashboard,
        });
        self.connection = Connection::Connected;
    }

    fn flush_commands(&mut self) {
        if self.demo || self.read_only || !matches!(self.connection, Connection::Connected) {
            return;
        }
        let now = Instant::now();
        let controls = self
            .pending_commands
            .iter()
            .filter(|(control, _)| command_due(self.last_command.get(*control), now))
            .map(|(control, value)| (control.clone(), value.clone()))
            .collect::<Vec<_>>();
        for (control, value) in controls {
            if self.snapshot_sender.as_ref().is_some_and(|sender| {
                sender
                    .send(ReaderRequest::Command(control.clone(), value))
                    .is_ok()
            }) {
                self.pending_commands.remove(&control);
                self.last_command.insert(control, now);
            }
        }
    }

    fn lock(&mut self) {
        self.locked = true;
        self.unlock_step = 0;
        self.unlock_started = None;
        self.focus = None;
        self.pending_commands.clear();
    }
}

fn command_due(last: Option<&Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(*last) >= COMMAND_INTERVAL)
}

fn timeout_env(name: &str) -> Option<Duration> {
    let value = env::var_os(name).map(|value| value.to_string_lossy().into_owned());
    parse_timeout(value.as_deref())
}

fn parse_timeout(value: Option<&str>) -> Option<Duration> {
    let Some(value) = value else {
        return Some(DEFAULT_AUTO_LOCK_AFTER);
    };
    if value == "disabled" {
        return None;
    }
    value
        .strip_suffix('s')
        .and_then(|seconds| seconds.parse::<u64>().ok())
        .map(Duration::from_secs)
        .or_else(|| {
            value
                .strip_suffix('m')
                .and_then(|minutes| minutes.parse::<u64>().ok())
                .map(|minutes| Duration::from_secs(minutes * 60))
        })
        .or(Some(DEFAULT_AUTO_LOCK_AFTER))
}

fn auto_lock_due(timeout: Option<Duration>, last_touch: Instant, now: Instant) -> bool {
    timeout.is_some_and(|timeout| now.duration_since(last_touch) >= timeout)
}

fn touch_starts(context: &egui::Context) -> Vec<egui::Pos2> {
    context.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Touch {
                    phase: egui::TouchPhase::Start,
                    pos,
                    ..
                } => Some(*pos),
                _ => None,
            })
            .collect()
    })
}

fn unlock_target(rect: egui::Rect, step: usize) -> egui::Rect {
    let (x, y) = match step {
        0 => (rect.left(), rect.top()),
        1 => (rect.right() - UNLOCK_TARGET_SIZE, rect.top()),
        2 => (
            rect.right() - UNLOCK_TARGET_SIZE,
            rect.bottom() - UNLOCK_TARGET_SIZE,
        ),
        _ => (rect.left(), rect.bottom() - UNLOCK_TARGET_SIZE),
    };
    egui::Rect::from_min_size(
        egui::pos2(x, y),
        egui::vec2(UNLOCK_TARGET_SIZE, UNLOCK_TARGET_SIZE),
    )
}

fn advance_unlock(step: usize, rect: egui::Rect, touch: egui::Pos2) -> usize {
    if unlock_target(rect, step).contains(touch) {
        step + 1
    } else if unlock_target(rect, 0).contains(touch) {
        1
    } else {
        0
    }
}

fn unlock_expired(started: Option<Instant>, now: Instant) -> bool {
    started.is_some_and(|started| now.duration_since(started) > UNLOCK_TIMEOUT)
}

impl eframe::App for TouchscreenApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.receive();
        log_raw_input(&mut self.debug, ui.ctx());
        let touches = touch_starts(ui.ctx());
        let now = Instant::now();
        if !touches.is_empty() {
            self.last_touch = now;
        }
        if !self.locked && auto_lock_due(self.auto_lock_after, self.last_touch, now) {
            self.lock();
            log_debug(&mut self.debug, "auto lock");
        }
        if self.close_focus_next_frame {
            self.focus = None;
            self.close_focus_next_frame = false;
        }
        ui.ctx().request_repaint_after(FRAME_INTERVAL);
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        apply_style(ui.ctx());
        if self.calibration.is_some() {
            self.calibration_ui(ui);
            return;
        }
        if self.locked {
            self.locked_ui(ui, &touches);
            return;
        }
        self.review_overlay(ui.ctx());
        ui.label(egui::RichText::new("FOCUSRITE CONTROLLER").small().weak());
        ui.heading("KEF level");
        let status = if self.demo {
            match self.demo_status {
                DemoStatus::Online => "● Demo — no device",
                DemoStatus::Offline => "Demo offline — reconnecting…",
                DemoStatus::Error => "Demo error — retrying…",
            }
        } else {
            match self.connection {
                Connection::Connecting => "Connecting…",
                Connection::Connected if self.read_only => "● Connected — read only",
                Connection::Connected => "● Connected",
                Connection::Offline => "Offline — reconnecting…",
                Connection::ProtocolError => "Protocol error — reconnecting…",
            }
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(status).color(egui::Color32::from_rgb(80, 211, 190)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized([64.0, 32.0], egui::Button::new("Lock"))
                    .clicked()
                {
                    self.lock();
                    log_debug(&mut self.debug, "manual lock");
                }
            });
        });
        ui.add_space(8.0);
        if self.demo && !matches!(self.demo_status, DemoStatus::Online) {
            ui.centered_and_justified(|ui| ui.label("Demo controls unavailable."));
            return;
        }
        let Some(state) = &self.state else {
            ui.centered_and_justified(|ui| ui.label("Waiting for focusrited…"));
            return;
        };
        if !state.online {
            ui.centered_and_justified(|ui| ui.label("Device unavailable — reconnecting…"));
            return;
        }
        let controls = dashboard_controls(&state.snapshot, &state.dashboard);
        if controls.is_empty() {
            ui.centered_and_justified(|ui| ui.label("No dashboard controls available."));
            return;
        }
        let card_width = ((ui.available_width() - 8.0) / 2.0).max(0.0);
        let card_height = ((ui.available_height() - 24.0) / 4.0).max(66.0);
        let mut strip_state = StripState {
            interactive: !self.read_only,
            demo: self.demo,
            overrides: &mut self.demo_values,
            pending_commands: &mut self.pending_commands,
        };
        for row in controls.chunks(2) {
            ui.horizontal(|ui| {
                for (capability, configured) in row {
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_width, card_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            strip(
                                ui,
                                DashboardStrip {
                                    capability,
                                    configured,
                                },
                                &state.snapshot,
                                &mut self.focus,
                                card_height,
                                &mut strip_state,
                                &mut self.debug,
                            );
                        },
                    );
                }
            });
            ui.add_space(8.0);
        }
        let mut close_focus = false;
        if let Some(control) = &self.focus
            && let Some(capability) = state
                .snapshot
                .capabilities
                .iter()
                .find(|capability| capability.id == *control)
        {
            let presentation = capability
                .presentation
                .as_ref()
                .expect("focus only comes from dashboard");
            let configured = state
                .dashboard
                .controls
                .iter()
                .find(|configured| configured.id == capability.id)
                .expect("focus only comes from dashboard");
            let response = egui::Modal::new(egui::Id::new("focus-panel"))
                .area(
                    egui::Modal::default_area(egui::Id::new("focus-panel"))
                        .default_size(egui::vec2(560.0, 250.0))
                        .movable(false),
                )
                .backdrop_color(egui::Color32::from_black_alpha(150))
                .show(ui.ctx(), |ui| {
                    ui.set_width(560.0);
                    ui.horizontal(|ui| {
                        ui.heading(configured.label.as_deref().unwrap_or(&presentation.label));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_sized([52.0, 44.0], egui::Button::new("Close"))
                                .clicked()
                            {
                                close_focus = true;
                            }
                        });
                    });
                    ui.add_space(20.0);
                    let mut ignore_focus = None;
                    strip(
                        ui,
                        DashboardStrip {
                            capability,
                            configured,
                        },
                        &state.snapshot,
                        &mut ignore_focus,
                        150.0,
                        &mut strip_state,
                        &mut self.debug,
                    );
                });
            close_focus |= response.should_close();
        }
        if close_focus {
            // Modal consumes this tap; close on next frame so it cannot select a strip beneath it.
            log_debug(&mut self.debug, "focus close requested");
            self.close_focus_next_frame = true;
        }
        self.flush_commands();
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, shown)| shown.elapsed() >= Duration::from_secs(3))
        {
            self.toast = None;
        }
        if let Some((message, _)) = &self.toast {
            egui::Area::new(egui::Id::new("command-toast"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(112, 57, 57))
                        .inner_margin(10.0)
                        .show(ui, |ui| ui.label(message));
                });
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }
}

impl TouchscreenApp {
    fn locked_ui(&mut self, ui: &mut egui::Ui, touches: &[egui::Pos2]) {
        let rect = ui.max_rect();
        for (step, label) in ["1", "2", "3", "4"].into_iter().enumerate() {
            let target = unlock_target(rect, step);
            ui.painter().rect_filled(
                target,
                4.0,
                if step == self.unlock_step {
                    egui::Color32::from_rgb(80, 211, 190)
                } else {
                    egui::Color32::from_rgb(55, 66, 75)
                },
            );
            ui.painter().text(
                target.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
        }
        for touch in touches {
            if unlock_expired(self.unlock_started, Instant::now()) {
                self.unlock_step = 0;
                self.unlock_started = None;
            }
            self.unlock_step = advance_unlock(self.unlock_step, rect, *touch);
            if self.unlock_step == 1 {
                self.unlock_started = Some(Instant::now());
            } else if self.unlock_step == 0 {
                self.unlock_started = None;
            }
            if self.unlock_step == 4 {
                self.locked = false;
                self.unlock_step = 0;
                self.unlock_started = None;
                self.last_touch = Instant::now();
                log_debug(&mut self.debug, "unlocked");
                break;
            }
        }
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Locked");
                ui.label("Touch corners 1 to 4 clockwise to unlock");
            });
        });
    }

    fn calibration_ui(&mut self, ui: &mut egui::Ui) {
        let touches = ui.ctx().input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Touch {
                        phase: egui::TouchPhase::Start,
                        pos,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        let calibration = self.calibration.as_mut().expect("checked above");
        if let Some(observed) = touches.first()
            && calibration.next < CALIBRATION_POINTS.len()
        {
            let expected = CALIBRATION_POINTS[calibration.next];
            log_debug(
                &mut self.debug,
                format!(
                    "calibration target={} expected={expected:?} observed={observed:?}",
                    calibration.next + 1
                ),
            );
            calibration.next += 1;
        }
        ui.heading("Touch calibration");
        if calibration.next == CALIBRATION_POINTS.len() {
            ui.label("Captured. Do not tap again; report completion.");
            return;
        }
        ui.label(format!(
            "Tap target {} of {}",
            calibration.next + 1,
            CALIBRATION_POINTS.len()
        ));
        let target = CALIBRATION_POINTS[calibration.next];
        ui.painter()
            .circle_filled(target, 26.0, egui::Color32::from_rgb(80, 211, 190));
        ui.painter()
            .circle_filled(target, 12.0, egui::Color32::from_rgb(14, 18, 21));
    }

    fn review_overlay(&mut self, context: &egui::Context) {
        if !self.review {
            return;
        }
        egui::Window::new("Review")
            .default_pos(egui::pos2(590.0, 8.0))
            .resizable(false)
            .collapsible(false)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Focus").clicked() {
                        self.demo_status = DemoStatus::Online;
                        self.focus = Some(ControlId("demo.level.0".into()));
                    }
                    if ui.button("Online").clicked() {
                        self.demo_status = DemoStatus::Online;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Offline").clicked() {
                        self.demo_status = DemoStatus::Offline;
                    }
                    if ui.button("Error").clicked() {
                        self.demo_status = DemoStatus::Error;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Cut toast").clicked() {
                        self.demo_toast = Some("Cut requested — demo only");
                    }
                    if ui.button("Tap targets").clicked() {
                        self.calibration_targets = !self.calibration_targets;
                    }
                });
            });
        if let Some(message) = self.demo_toast {
            egui::Area::new(egui::Id::new("demo-toast"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .show(context, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(55, 66, 75))
                        .inner_margin(10.0)
                        .show(ui, |ui| ui.label(message));
                });
        }
        if self.calibration_targets {
            for (id, anchor) in [
                ("top-left", egui::Align2::LEFT_TOP),
                ("top-right", egui::Align2::RIGHT_TOP),
                ("bottom-left", egui::Align2::LEFT_BOTTOM),
                ("bottom-right", egui::Align2::RIGHT_BOTTOM),
            ] {
                egui::Area::new(egui::Id::new(("touch-target", id)))
                    .anchor(anchor, egui::vec2(18.0, 18.0))
                    .show(context, |ui| {
                        if ui.add_sized([44.0, 44.0], egui::Button::new("●")).clicked() {
                            self.demo_toast = Some("Touch target received");
                        }
                    });
            }
        }
    }
}

fn dashboard_controls<'a>(
    snapshot: &'a DeviceSnapshot,
    dashboard: &'a DashboardConfig,
) -> Vec<(&'a ControlCapability, &'a DashboardControl)> {
    dashboard
        .controls
        .iter()
        .filter_map(|configured| {
            snapshot
                .capabilities
                .iter()
                .find(|capability| {
                    capability.id == configured.id
                        && capability.available
                        && capability.writable
                        && capability
                            .presentation
                            .as_ref()
                            .is_some_and(|item| item.kind == PresentationKind::Level)
                })
                .map(|capability| (capability, configured))
        })
        .take(DASHBOARD_LIMIT)
        .collect()
}

fn strip(
    ui: &mut egui::Ui,
    strip: DashboardStrip<'_>,
    snapshot: &DeviceSnapshot,
    focus: &mut Option<ControlId>,
    height: f32,
    state: &mut StripState<'_>,
    debug: &mut Option<DebugLog>,
) {
    let capability = strip.capability;
    let presentation = capability.presentation.as_ref().expect("filtered above");
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(27, 34, 40))
        .corner_radius(6.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            let inner_height = (height - 20.0).max(0.0);
            ui.set_min_height(inner_height);
            ui.add_space(4.0);
            let (label_rect, label_response) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
            ui.painter().text(
                label_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                strip
                    .configured
                    .label
                    .as_deref()
                    .unwrap_or(&presentation.label),
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            );
            if label_response.clicked() {
                log_debug(
                    debug,
                    format!(
                        "focus hit control={} rect={label_rect:?} pointer={:?}",
                        capability.id.0,
                        ui.ctx().input(|input| input.pointer.interact_pos()),
                    ),
                );
                *focus = Some(capability.id.clone());
            }
            ui.add_space(2.0);
            let value = normalized_value(capability, snapshot, state.overrides);
            ui.horizontal(|ui| {
                let mute_width = if presentation.companion.is_some() {
                    54.0
                } else {
                    0.0
                };
                let slider_width = (ui.available_width() - mute_width - 6.0).max(20.0);
                if let Some(value) =
                    level_rail(ui, slider_width, value, state.interactive, &capability.id)
                    && let (Some(minimum), Some(maximum)) = (capability.minimum, capability.maximum)
                {
                    let raw = f64::from(minimum) + (f64::from(maximum - minimum) * value).round();
                    let requested = Value::Integer(raw as i32);
                    if state.demo {
                        state.overrides.insert(capability.id.clone(), requested);
                    } else {
                        state
                            .pending_commands
                            .insert(capability.id.clone(), requested);
                    }
                    log_debug(
                        debug,
                        format!(
                            "level set control={} normalized={value:.3}",
                            capability.id.0
                        ),
                    );
                }
                if let Some(companion) = &presentation.companion
                    && mute_button(ui, mute_width, state.interactive, companion)
                {
                    let muted = if state.demo {
                        matches!(
                            state
                                .overrides
                                .get(companion)
                                .or_else(|| snapshot.values.get(companion)),
                            Some(Value::Bool(true))
                        )
                    } else {
                        matches!(
                            state
                                .pending_commands
                                .get(companion)
                                .or_else(|| snapshot.values.get(companion)),
                            Some(Value::Bool(true))
                        )
                    };
                    if state.demo {
                        state
                            .overrides
                            .insert(companion.clone(), Value::Bool(!muted));
                    } else {
                        state
                            .pending_commands
                            .insert(companion.clone(), Value::Bool(!muted));
                    }
                    log_debug(
                        debug,
                        format!("mute set control={} value={}", companion.0, !muted),
                    );
                }
            });
        });
}

fn debug_log() -> Option<DebugLog> {
    env::var_os("FOCUSRITE_UI_DEBUG").and_then(|_| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/focusrite-ui-debug.log")
            .ok()
            .map(DebugLog)
    })
}

fn log_debug(debug: &mut Option<DebugLog>, message: impl std::fmt::Display) {
    if let Some(DebugLog(file)) = debug {
        let _ = writeln!(file, "{message}");
        let _ = file.flush();
    }
}

fn log_raw_input(debug: &mut Option<DebugLog>, context: &egui::Context) {
    if debug.is_none() {
        return;
    }
    for event in context.input(|input| {
        input
            .raw
            .events
            .iter()
            .map(|event| ("raw", event.clone()))
            .chain(
                input
                    .events
                    .iter()
                    .map(|event| ("delivered", event.clone())),
            )
            .collect::<Vec<_>>()
    }) {
        if matches!(
            &event.1,
            egui::Event::Touch { .. }
                | egui::Event::PointerMoved(_)
                | egui::Event::PointerButton { .. }
        ) {
            log_debug(debug, format!("{} input {:?}", event.0, event.1));
        }
    }
}

fn level_rail(
    ui: &mut egui::Ui,
    width: f32,
    value: f64,
    interactive: bool,
    id: &ControlId,
) -> Option<f64> {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 28.0),
        if interactive {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        },
    );
    let rail = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 12.0));
    let value_x = egui::lerp(rail.left()..=rail.right(), value as f32);
    let painter = ui.painter();
    painter.rect_filled(rail, 3.0, egui::Color32::from_rgb(55, 66, 75));
    painter.rect_filled(
        egui::Rect::from_min_max(rail.left_top(), egui::pos2(value_x, rail.bottom())),
        3.0,
        egui::Color32::from_rgb(80, 211, 190),
    );
    for step in 1..10 {
        let x = egui::lerp(rail.left()..=rail.right(), step as f32 / 10.0);
        let height = if step == 5 { 12.0 } else { 8.0 };
        painter.line_segment(
            [
                egui::pos2(x, rail.center().y - height / 2.0),
                egui::pos2(x, rail.center().y + height / 2.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(27, 34, 40)),
        );
    }
    painter.circle_filled(
        egui::pos2(value_x, rail.center().y),
        12.0,
        egui::Color32::from_rgb(231, 237, 239),
    );
    painter.circle_stroke(
        egui::pos2(value_x, rail.center().y),
        12.0,
        egui::Stroke::new(5.0, egui::Color32::from_rgb(80, 211, 190)),
    );
    if interactive && (response.clicked() || response.dragged()) {
        let pos = ui.ctx().input(|input| input.pointer.interact_pos())?;
        return Some(((pos.x - rail.left()) / rail.width()).clamp(0.0, 1.0) as f64);
    }
    let _ = id;
    None
}

fn mute_button(ui: &mut egui::Ui, width: f32, interactive: bool, id: &ControlId) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 28.0),
        if interactive {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    ui.painter()
        .rect_filled(rect, 4.0, egui::Color32::from_rgb(55, 66, 75));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Mute",
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );
    let _ = id;
    response.clicked()
}

fn apply_style(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(14, 18, 21);
    visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(55, 66, 75);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(80, 211, 190);
    context.set_visuals(visuals);
    context.global_style_mut(|style| style.animation_time = 0.0);
}

fn demo_state() -> ConfirmedState {
    let levels = [
        ("KEF level", 72),
        ("Optical mix 1", 54),
        ("Headphones", 36),
        ("Cue", 48),
        ("Mix 1", 63),
        ("Mix 2", 41),
        ("Output 3", 80),
        ("Output 4", 24),
    ];
    let mut capabilities = Vec::new();
    let mut values = BTreeMap::new();
    for (order, (label, value)) in levels.into_iter().enumerate() {
        let level = ControlId(format!("demo.level.{order}"));
        let mute = ControlId(format!("demo.mute.{order}"));
        capabilities.push(ControlCapability {
            id: level.clone(),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(0),
            maximum: Some(100),
            presentation: Some(ControlPresentation {
                label: label.into(),
                kind: PresentationKind::Level,
                default_dashboard_order: Some(order as u8),
                companion: Some(mute.clone()),
                step: Some(1),
            }),
        });
        capabilities.push(ControlCapability {
            id: mute.clone(),
            domain: ValueDomain::Boolean,
            writable: true,
            available: true,
            minimum: None,
            maximum: None,
            presentation: Some(ControlPresentation {
                label: format!("{label} mute"),
                kind: PresentationKind::Mute,
                default_dashboard_order: None,
                companion: Some(level),
                step: None,
            }),
        });
        values.insert(mute, Value::Bool(false));
        values.insert(
            ControlId(format!("demo.level.{order}")),
            Value::Integer(value),
        );
    }
    let snapshot = DeviceSnapshot {
        device_id: "display-demo".into(),
        capability_schema: "display-demo-v1".into(),
        capabilities,
        values,
    };
    ConfirmedState {
        instance_id: "demo".into(),
        revision: 1,
        online: true,
        dashboard: DashboardConfig::defaults(&snapshot),
        snapshot,
    }
}

fn normalized_value(
    capability: &ControlCapability,
    snapshot: &DeviceSnapshot,
    overrides: &BTreeMap<ControlId, Value>,
) -> f64 {
    let (Some(minimum), Some(maximum), Some(Value::Integer(value))) = (
        capability.minimum,
        capability.maximum,
        overrides
            .get(&capability.id)
            .or_else(|| snapshot.values.get(&capability.id)),
    ) else {
        return 0.0;
    };
    let span = maximum - minimum;
    if span == 0 {
        0.0
    } else {
        f64::from(*value - minimum) / f64::from(span)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        io::{BufRead, BufReader},
        os::unix::net::{UnixListener, UnixStream},
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::Duration,
    };

    use super::*;
    use focusrited::{ControlPresentation, ValueDomain};

    fn capability(order: Option<u8>, kind: PresentationKind) -> ControlCapability {
        ControlCapability {
            id: ControlId(format!("control-{order:?}")),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(0),
            maximum: Some(100),
            presentation: Some(ControlPresentation {
                label: "Control".into(),
                kind,
                default_dashboard_order: order,
                companion: None,
                step: None,
            }),
        }
    }

    #[test]
    fn dashboard_uses_configured_levels_in_order() {
        let snapshot = DeviceSnapshot {
            device_id: "mock".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![
                capability(Some(2), PresentationKind::Level),
                capability(None, PresentationKind::Level),
                capability(Some(1), PresentationKind::Mute),
                capability(Some(1), PresentationKind::Level),
            ],
            values: BTreeMap::new(),
        };
        let dashboard = DashboardConfig::defaults(&snapshot);
        let controls = dashboard_controls(&snapshot, &dashboard);
        assert_eq!(controls.len(), 2);
        assert_eq!(
            controls[0]
                .0
                .presentation
                .as_ref()
                .unwrap()
                .default_dashboard_order,
            Some(1)
        );
    }

    #[test]
    fn command_rate_limit_keeps_latest_value_pending() {
        let now = Instant::now();
        assert!(command_due(None, now));
        assert!(!command_due(Some(&now), now));
        assert!(command_due(Some(&now), now + COMMAND_INTERVAL));
    }

    #[test]
    fn auto_lock_timeout_accepts_duration_or_disabled() {
        assert_eq!(parse_timeout(None), Some(Duration::from_secs(60)));
        assert_eq!(parse_timeout(Some("90s")), Some(Duration::from_secs(90)));
        assert_eq!(parse_timeout(Some("2m")), Some(Duration::from_secs(120)));
        assert_eq!(parse_timeout(Some("disabled")), None);
        assert_eq!(
            parse_timeout(Some("invalid")),
            Some(Duration::from_secs(60))
        );
        let now = Instant::now();
        assert!(auto_lock_due(
            Some(Duration::from_secs(1)),
            now,
            now + Duration::from_secs(1)
        ));
        assert!(!auto_lock_due(None, now, now + Duration::from_secs(999)));
    }

    #[test]
    fn unlock_requires_clockwise_corners() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 480.0));
        let mut step = 0;
        for expected in 0..4 {
            step = advance_unlock(step, rect, unlock_target(rect, expected).center());
        }
        assert_eq!(step, 4);
        assert_eq!(advance_unlock(2, rect, egui::pos2(400.0, 240.0)), 0);
        let now = Instant::now();
        assert!(!unlock_expired(Some(now), now + UNLOCK_TIMEOUT));
        assert!(unlock_expired(
            Some(now),
            now + UNLOCK_TIMEOUT + Duration::from_millis(1)
        ));
    }

    #[test]
    fn read_only_mode_keeps_commands_local() {
        let (_event_sender, event_receiver) = mpsc::channel();
        let (request_sender, request_receiver) = mpsc::channel();
        let mut app = TouchscreenApp::new(event_receiver, Some(request_sender), false);
        app.connection = Connection::Connected;
        app.read_only = true;
        app.pending_commands
            .insert(ControlId("level".into()), Value::Integer(42));
        app.flush_commands();
        assert!(
            request_receiver
                .recv_timeout(Duration::from_millis(20))
                .is_err()
        );
        assert_eq!(app.pending_commands.len(), 1);
    }

    #[test]
    fn command_request_uses_v1_tagged_value() {
        let (mut client, server) = UnixStream::pair().unwrap();
        write_request(
            &mut client,
            ReaderRequest::Command(ControlId("level".into()), Value::Integer(42)),
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(server).read_line(&mut line).unwrap();
        assert_eq!(
            line,
            "{\"control\":\"level\",\"type\":\"command\",\"v\":1,\"value\":{\"type\":\"integer\",\"value\":42}}\n"
        );
    }

    #[test]
    fn ui_command_round_trip_uses_confirmed_daemon_state() {
        static SOCKET_NUMBER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "focusrite-ui-{}-{}.sock",
            std::process::id(),
            SOCKET_NUMBER.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let initial = DeviceSnapshot {
            device_id: "mock".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![capability(Some(0), PresentationKind::Level)],
            values: BTreeMap::from([(ControlId("control-Some(0)".into()), Value::Integer(10))]),
        };
        let confirmed = DeviceSnapshot {
            values: BTreeMap::from([(ControlId("control-Some(0)".into()), Value::Integer(42))]),
            ..initial.clone()
        };
        let dashboard = DashboardConfig::defaults(&initial);
        let (result_sent, result_ready) = mpsc::channel();
        let (finish_sender, finish_received) = mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert_eq!(line, "{\"v\":1,\"type\":\"snapshot\"}\n");
            writeln!(
                writer,
                "{}",
                serde_json::json!({
                    "v": 1, "type": "snapshot", "instance_id": "mock",
                    "revision": 1, "online": true, "snapshot": initial, "dashboard": dashboard,
                })
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&line).unwrap(),
                serde_json::json!({
                    "v": 1, "type": "command", "control": "control-Some(0)",
                    "value": {"type": "integer", "value": 42},
                })
            );
            writeln!(
                writer,
                "{}",
                serde_json::json!({
                    "v": 1, "type": "command_result", "instance_id": "mock",
                    "revision": 2, "online": true, "snapshot": confirmed, "dashboard": dashboard,
                })
            )
            .unwrap();
            result_sent.send(()).unwrap();
            finish_received.recv().unwrap();
        });
        let (event_sender, event_receiver) = mpsc::channel();
        let request_sender = spawn_socket_reader(event_sender, path.clone());
        let mut app = TouchscreenApp::new(event_receiver, Some(request_sender), false);
        let control = ControlId("control-Some(0)".into());

        for _ in 0..100 {
            app.receive();
            if app.state.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            app.state.as_ref().unwrap().snapshot.values.get(&control),
            Some(&Value::Integer(10))
        );
        app.pending_commands
            .insert(control.clone(), Value::Integer(42));
        app.flush_commands();
        result_ready.recv_timeout(Duration::from_secs(1)).unwrap();
        for _ in 0..100 {
            app.receive();
            if app.state.as_ref().is_some_and(|state| state.revision == 2) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            app.state.unwrap().snapshot.values.get(&control),
            Some(&Value::Integer(42))
        );
        finish_sender.send(()).unwrap();
        server.join().unwrap();
        fs::remove_file(path).unwrap();
    }
}
