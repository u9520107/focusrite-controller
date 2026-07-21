# Phase 4a: Local Touchscreen

## Status

MR 1 is merged as `4698fcf` on `main`. MR 2a/2b are complete on this branch:
additive capability-presentation metadata, generic fullscreen client, mock IPC
coverage, and read-only Pi kiosk/client-restart evidence. The client correctly
shows no Solo strips when adapter presentation is absent; it never guesses
control semantics. Phase 4a retains MR 3 profiles. Live-control implementation
requires separate mock-first verification and explicit hardware-write approval.

UX review brief: [Phase 4a UX design](../design/phase-4a-ux.md).

## Goal

Run a fullscreen touchscreen client on Pi using only `focusrited`'s local
Unix-socket API. The daemon remains sole policy and hardware authority. A
client crash, restart, or slow client must not interrupt daemon or device
operation.

## Scope and guardrails

- Local Unix-domain socket only. No TCP listener, browser, LAN authentication,
  USB, or ALSA access in the client.
- Use versioned, newline-delimited JSON messages. One message is at most 64 KiB;
  malformed, unsupported-version, or oversized input receives a bounded error
  then its connection closes.
- Socket path and permissions are daemon-owned. Initial default is
  `/run/focusrited/focusrited.sock` with owner/group mode `0660`; Phase 8
  packaging creates dedicated group and runtime directory.
- Keep one serial `DeviceWorker`. IPC handlers submit worker requests; they
  never own hardware or bypass validation.
- Each connection has bounded outbound queue. State events may coalesce to its
  newest revision for a slow client; command replies never coalesce. Queue
  overflow disconnects only that client.
- Snapshot is resync authority. On reconnect, revision gap, or changed daemon
  `instance_id`, client discards cache and requests a new snapshot.
- No profile apply or hardware write during test setup without explicit
  approval. Mock tests cover mutating commands; Pi hardware validation starts
  read-only.
- Direct dependencies `serde` and `serde_json` are reviewed as
  MIT OR Apache-2.0 and compatible with project MIT distribution.
- Direct `libc 0.2.186` is MIT OR Apache-2.0 and is used only for standard
  Unix `SIGTERM`/`SIGINT` registration during graceful daemon shutdown.

## Merge-request plan

### MR 1: Versioned local IPC transport

**Scope**

- Add transport-neutral `v1` wire types for snapshot request/reply, control
  command/reply, state event, and bounded error. JSON field names are stable;
  wire values are explicit rather than Rust debug output.
- Add daemon-generated `instance_id`, per-client connection loop, Unix listener,
  signal-safe socket cleanup, and startup option for socket path.
- Send a full snapshot after successful connect and on explicit snapshot
  request. Broadcast authoritative state after worker-observed revision change,
  including external events and offline/recovery transitions.
- Serialize commands through existing `DeviceWorker`; return confirmed state or
  a mapped safe error. Do not add profiles, idempotency keys, dangerous-control
  confirmation, or compound commands yet: current service cannot implement
  them correctly.
- Add mock IPC integration tests for framing, malformed input, command order,
  reconnect/resync, external-state event delivery, slow-client isolation, and
  two local clients converging after sequential writes.

**Verification**

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- Mock integration tests exercise Unix sockets without ALSA hardware.
- On Pi, start daemon with a disposable socket/runtime directory and perform
  snapshot-only connect/disconnect check. No command or profile apply.

**Hardware action**

Read-only Pi socket smoke test only. Any touchscreen display work or hardware
write needs separate explicit approval.

**Evidence — 2026-07-17 (complete)**

- With explicit approval, `focusrited` started on Pi against ALSA ID `Gen`
  using disposable socket and profile-store paths. A local `v1` snapshot
  request received the automatic connection snapshot and explicit snapshot;
  both were online at revision 1 with the same instance ID.
- The daemon received no command, performed no ALSA write, and its disposable
  profile-store path remained absent. It then stopped cleanly.
- The current ALSA card index was 1 after reboot; named ID `Gen` remained the
  stable selector. No raw control values or device identifiers are retained.
- Mock IPC coverage passes for framing errors, command ordering, reconnect
  snapshots, external worker events, two-client convergence, event coalescing,
  slow-client queue overflow, and bounded per-client request turns.

- After merge, `cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, and `cargo test --workspace` passed. The test suite has 32
  passing non-hardware tests; three Pi hardware tests remain ignored because
  they require attached hardware. Unix-socket tests require an environment
  permitted to bind local sockets; they passed outside this development sandbox.

### MR 2a: Capability presentation contract — complete

**Why first**

MR 1 exposes opaque control IDs, value domains, and bounds. That is sufficient
for safe commands, but not for a capability-only UI: it cannot derive a safe
display label, control kind, default dashboard placement, compatible mute, or
integer increment. Choosing controls by ID in client would break the
no-device-specific-UI rule.

**Scope**

- Add compact optional presentation data to each adapter-discovered capability
  and serialize it in existing snapshot/event messages. Keep current wire
  fields and semantics unchanged.
- Presentation data is declarative: display label, `level`/`mute` kind,
  default dashboard visibility/order, and proven level-to-mute association.
  Adapter maps ALSA controls to this metadata; client never maps device IDs,
  labels, or routing itself.
- For writable integer controls, expose an adapter-declared positive step. Do
  not infer a step from current value. Bool controls need no step; values with
  no usable presentation remain hidden from first UI.
- Render at most 12 adapter-default dashboard controls. P4c later replaces
  those defaults with user visibility/order/labels, virtual groups, and sync
  sets. No metadata persistence or preference API in this MR.
- Add mock tests proving snapshots/events preserve presentation data and client
  selection needs no device control ID.

**Non-goals**

- No new command type, write path, toolkit, executable, metadata persistence,
  or hardware action.
- No display strings guessed from opaque IDs. If Solo discovery cannot prove a
  role or sensible increment, omit control and record gap.

**Acceptance**

- Mock snapshot identifies zero or more default level/mute strips entirely from
  capability data.
- Existing IPC v1 clients remain compatible because fields are additive.
- Full Rust verification passes; no Pi interaction required.

**Evidence — complete**

- `ControlPresentation` is additive in snapshot/event IPC messages. Mock
  coverage proves serialized display metadata and dashboard selection without
  client control-ID matching.
- Existing v1 clients remain compatible; presentation is optional and absent
  controls stay hidden.

**Solo adapter evidence — implementation deferred to Phase 4c MR2c**

Phase 4a owns generic presentation metadata and a client that consumes only
daemon-declared controls. It does not need to lock down a hardware capability
mapping. Phase 4c MR2c owns adaptive structural inference, integer metadata,
logical compound operations, and any later write approval. This evidence stays
here as Phase 4a discovery context; its implementation plan is authoritative in
[Phase 4c](phase-4c-dashboard-groups.md).

Dynamic ALSA discovery is necessary but not sufficient: it proves a control's
existence, type, bounds, and current availability, but not its user meaning,
routing effect, or safe write policy. Each supported adapter therefore carries
a small versioned availability mapping from a proven discovered control shape
to canonical presentation/permission. Runtime availability is the intersection
of that mapping and current discovery. The touchscreen consumes only resulting
capabilities; it never matches hardware names. Unknown, changed, or ambiguous
controls remain read-only/unavailable until evidence adds a mapping.

Read-only discovery found 24 `Mix A` through `Mix F` input playback-volume
controls and eight `Monitor Mix A/B` input playback-volume controls. Each is a
single writable ALSA integer with observed declared range `0..184` and step
one. Upstream driver evidence identifies `Mix A` through `Mix F` as six generic
matrix buses; they stay hidden on Solo. `Monitor Mix A/B Input 1..4` is the
Solo Direct Monitor 2-by-4 gain matrix: two output sides and four sources.
The attached Solo is upstream driver product `0x8218`, whose capability shape
has one Direct Monitor selection and four mixer inputs; that produces exactly
eight monitor-mix cells. Focusrite documents those sources as Analogue 1,
Analogue 2, and stereo Playback 1-2. The source-to-cell balance rule still
belongs in a compound group declaration, never in the client.
The Solo exposes no hardware main-output fader. Direct Monitor and phantom
power are writable booleans, not faders, and remain excluded from this slice.
All four Solo Direct Monitor sources feed one shared monitor/headphone mix;
line and headphone output paths duplicate that mix after it, with physical
knobs. They are not separate software output tracks.

- Phase 4c MR2c extends raw ALSA discovery with integer minimum, maximum, and
  step, then resolves an adaptive structural mapping. Preserve raw ALSA name
  only inside adapter discovery; clients continue receive opaque IDs plus
  adapter-declared presentation.
- Phase 4c MR2c may map the eight discovered `Monitor Mix` cells only after a
  unique reviewed structural match. Do not use client-side name matching or
  make broad integer controls writable.
- A user-facing Direct Monitor source is a compound matrix operation, not one
  raw cell: its group changes required A/B cells together while retaining
  source balance/pan rules. Therefore `USB Playback 1/2` and analogue source
  strips are deferred: they are not needed for current personal workflows.
  Raw cells remain hidden/non-writable.
- All other internal mixer controls, Direct Monitor, phantom power, routing
  enums, meters, and arrays remain hidden/non-writable.
- Phase 4c keeps hardware write policy fail-closed until mock command coverage
  proves range/type rejection and explicit user approval permits one reversible
  Solo operation. No implementation step sends a live command by default.
- Dashboard capacity remains 12. No default-control selection is needed for
  Solo until the four Direct Monitor tracks are proven; larger devices use
  Settings availability/list behavior from Phase 4c.
- Solo Direct Monitor source tracks, including `USB Playback 1/2`, are
  deferred. Input 1 and Input 2 retain their physical preamp-gain knobs;
  current use does not need touchscreen direct-mix control.
- Main output is hardware-knob-only. A later read-only output meter may use the
  discovered meter capability after its format and scale are proven; it is not
  a substitute for an output fader and is outside this first control slice.
- Phase 4c verifies fixture shape/inference plus mock IPC command confirmation
  and rejection. A read-only Pi snapshot and any approved reversible hardware
  operation use the Phase 4c restore procedure.

### MR 2b: Fullscreen touch client and primary controls — complete

**Scope**

- Add one `focusrite-ui` Rust executable using MR 1 API only. It reconnects to
  configured Unix socket, discards cache on changed `instance_id` or revision
  gap, and requests snapshot before rendering controls.
- Render connection/device status and only writable, available adapter-default
  controls from MR 2a. Render absent/unsupported controls nowhere, not disabled
  guesses. P4c later supplies configured dashboard items. Confirmed
  snapshot/event values are displayed; no optimistic state remains after reply
  or error.
- First layout: compact two-column level strips, each with label, horizontal
  slider, optional compatible mute, 10 visual rail divisions, and label/card
  Focus panel. Cap default dashboard at 12 controls; use scrolling after eight.
  Use native touch targets. Cap event redraws at 60 Hz; gestures send at most 30
  commands/second, retaining newest pending value.
- Editable Settings is Phase 4c work. User labels, visibility/order persistence,
  import/export, virtual groups, and sync sets are not silently included in
  P4a. The design mockup retains Settings as P4c review material.
- Toolkit decision gate: first run no-control fullscreen spike on actual Pi
  display. Compare `egui/eframe` and `gtk4` only for Pi build, fullscreen/kiosk,
  touch input, binary/runtime size, and needed system packages. Pick one; do
  not add both or web stack. Record resolution, orientation, compositor, and
  selected toolkit before client-control code starts.
- UI disconnect/crash/restart must not alter daemon/device state. UI never
  accesses ALSA, USB, or profiles.
- A development-only demo/review mode may render synthetic capability data and
  expose state controls (Focus open/close, offline/reconnect, error toast,
  Cut, and touch-calibration targets). It is opt-in, never the normal kiosk
  mode, never connects to the daemon, and never sends device commands. Use it
  to capture deterministic screenshots and, after state behavior stabilizes,
  short transition video for visual review.
- `FOCUSRITE_UI_READ_ONLY=1` may connect the native client to a real daemon for
  snapshot/layout review while disabling every touch action and suppressing all
  command transmission. It is the required mode for first live UI inspection.

**Verification**

- MR 1 checks plus deterministic mock-server tests for initial snapshot,
  reconnect, changed instance, revision gap, malformed reply, event coalescing,
  confirmed command/error toast rendering, muted state, Focus panel, and
  30-command/sec gesture bound.
- Pi fullscreen launch and read-only touch/screen-fit check, then client
  kill/restart and daemon continuity check. Record runtime facts without IDs.
- Capture a screenshot for each reviewed demo state and after every native
  layout/input fix; use a short local compositor capture only when animation or
  transition timing needs review. Do not retain device identifiers, raw levels,
  or other sensitive runtime data in committed artifacts.
- Command tests use mock device first. Live control write needs separate
  explicit approval and reviewed reversible procedure.

**Hardware action**

Display and touch interaction. Read-only fullscreen test needs explicit
approval. Live control write needs separate explicit approval.

**Implementation evidence — complete**

- `focusrite-ui` uses only local v1 IPC, resyncs from authoritative snapshots,
  and includes deterministic mock coverage for command framing, confirmed
  state, reconnect behavior, groups, rate limiting, lock, and read-only mode.
- Pi read-only fullscreen/kiosk evidence is recorded above. The attached Solo
  published no safe presentation controls, so UI rendered no guessed strips
  and sent no hardware command.
- **Evidence — 2026-07-20 (complete):** with explicit read-only approval, a
  disposable daemon/socket/profile/dashboard setup ran on Pi. A snapshot-only
  socket probe succeeded before and after killing the read-only UI. The daemon
  remained alive, restarted UI remained alive, and no temporary profile or
  dashboard file was created. No device command was sent.

### Runtime safety proposal: idle, cat lock, and shutdown

These behaviors are required before kiosk packaging, but do not authorize a
control write or change hardware state.

- The application auto-locks after 60 seconds without local touch input by
  default; the owner may also lock immediately from a stable header control.
  `auto_lock_after` is later user configuration, not an adapter setting, and
  accepts a duration or explicit `disabled` value.
- A locked application draws no active strips and does not hit-test faders,
  mute buttons, labels, or Focus. The only accepted input is unlock.
- Default unlock is a visible clockwise sequence through four 64 px corner
  targets within five seconds. A failed/incomplete sequence resets. This is
  deliberately harder for incidental cat touches than one tap, while requiring
  no keyboard. A future settings choice may use a different local unlock
  gesture; no password is stored in the touchscreen client.
- Display wake and control unlock are separate. Phase 8 kiosk session uses
  `swayidle` plus `wlopm` to power off the DSI output after ten idle minutes
  by default and power it on when input resumes. `display_poweroff_after` is
  independent user configuration and accepts a duration or explicit `disabled`
  value. A wake touch is consumed while the app stays locked; it can never
  adjust audio. The normal UI remains visible during shorter idle periods, so
  it does not constantly redraw or burn a transition.
- Initial local test configuration uses `FOCUSRITE_UI_AUTO_LOCK_AFTER` with
  `60s`, `10m`, or `disabled`, and `FOCUSRITE_UI_LOCK_ON_START=1`. Phase 4c
  replaces these launcher values with persisted settings. Phase 8 maps the
  independent `display_poweroff_after` setting into the kiosk `swayidle`
  invocation rather than letting the application control compositor power.
- Lock state is local presentation safety only. It neither changes daemon state
  nor writes device controls. Restart begins locked when kiosk mode is enabled.
- `LocalServer` removes its socket on ordinary `stop`/`Drop`; startup also
  removes a refused stale socket but never a live daemon socket. Current
  process `SIGTERM` does not reach that graceful path. Final systemd service
  must handle `SIGTERM`/`SIGINT` by stopping IPC and joining its thread, and
  use `RuntimeDirectory=focusrited` as a crash-cleanup backstop. Verification:
  graceful stop removes socket; forced death leaves no persistent runtime
  directory; restart never removes a live peer's socket.

**Display/toolkit gate evidence — 2026-07-17 (complete)**

- The target DSI panel is landscape `800x480`. It was deliberately re-enabled
  for a read-only fullscreen test after explicit approval.
- `eframe 0.35.0` was selected: it supports workspace Rust 1.97, is
  MIT OR Apache-2.0, and builds with the small `default_fonts`/`glow`/`wayland`
  feature set. GTK4 was not selected because the target has runtime but not
  development packages.
- A temporary LightDM auto-login session starts Labwc and the no-control
  `focusrite-ui` display spike. On-screen verification confirmed the controller
  check screen, not a TTY login or Raspberry Pi desktop. The daemon, socket,
  USB, ALSA, and device controls were not touched.
- Native touchscreen mapping was verified against four read-only kernel corner
  captures. The active user Labwc configuration, rather than the system
  fallback, required the exact attached touchscreen rule. It now maps direct
  touch to the DSI output and applies a device-specific calibration matrix.
  Mouse emulation is disabled. This corrected global-desktop scaling and is
  outside product packaging; retain a local rollback backup until Phase 8.
- Temporary Pi boot/session configuration is intentionally outside repository
  packaging. Phase 8 will replace it with a reviewed, installable service and
  explicit rollback procedure.
- `focusrite-ui` mock IPC round trip now verifies initial authoritative
  snapshot, a rate-limited queued level command, v1 command framing, and a
  `command_result` replacing the displayed state. It uses a temporary local
  Unix socket only; no daemon, ALSA, USB, or device control is started.
- With explicit read-only approval, a disposable daemon instance accepted a
  snapshot-only local request against the attached target. Its automatic and
  explicit snapshots were online at revision 1; no command was sent and no
  temporary profile store was created.
- A real `FOCUSRITE_UI_READ_ONLY=1` kiosk run connected successfully, but
  rendered no dashboard strips. The current Scarlett2 ALSA adapter deliberately
  publishes no presentation metadata and permits no integer writes, so the UI
  correctly refuses to guess controls, labels, ranges, or safe mutations.
  The temporary kiosk and daemon were restored/removed after the check.
- The graceful shutdown path was exercised against the attached target with a
  disposable socket: `SIGTERM` caused the daemon to stop and remove its socket
  without creating a profile artifact or receiving a control command.

### MR 3: Local profile workflow

**Scope**

- Complete service-side profile operations before exposing them: named save and
  list, device/schema binding result, deterministic dry-run diff, explicit
  reviewed apply, and per-control applied/skipped/failed report.
- Persist profile changes through existing store atomically. No auto-apply,
  rollback, or profile write on daemon startup/reconnect.
- Extend local IPC and touchscreen only after daemon behavior and mock tests
  prove those semantics.

**Verification**

- MR 1 checks plus mock coverage for binding mismatch, unavailable controls,
  ordered partial failure, reconnect, and concurrent local clients.
- Pi profile exercise only after explicit approval; preserve and restore device
  state through a reviewed test plan.

**Hardware action**

Profile save may write only service storage. Profile apply writes hardware and
requires explicit approval.

## Exit checks

- [x] Daemon exposes versioned, bounded local snapshot/command/event API.
- [x] Socket clients cannot access USB or ALSA and cannot interrupt daemon on
  failure.
- [x] Mock IPC tests cover ordering, reconnect/resync, and concurrent local
  client updates.
- [x] Fullscreen Pi touchscreen exposes only daemon-declared controls and
  remains usable after client restart. Hardware capability inference is Phase
  4c MR2c, not a Phase 4a gate.
- [ ] Local profile save/list/dry-run/reviewed apply returns safe per-control
  result and never auto-applies.

## Update rule

After each MR, record completed verification, screen/runtime findings, explicit
hardware approvals, and sanitized evidence. Do not expand into metering, LAN,
packaging, or FCP device support.
