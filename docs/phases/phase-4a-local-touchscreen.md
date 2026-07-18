# Phase 4a: Local Touchscreen

## Status

MR 1 is merged as `4698fcf` on `main`. Mock verification and Pi read-only
socket smoke validation are complete. Next proposed work is MR 2a: add the
small capability-presentation contract required for a generic touchscreen.
Do not begin client or live-control implementation until this proposal and the
target display/toolkit decision are accepted.

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

### MR 2a proposal: Capability presentation contract

**Why first**

MR 1 exposes opaque control IDs, value domains, and bounds. That is sufficient
for safe commands, but not for a capability-only UI: it cannot derive a safe
display label, order, writable primary role, or integer increment. Choosing
controls by ID in the client would break the no-device-specific-UI rule.

**Scope**

- Add compact optional presentation data to each adapter-discovered capability
  and serialize it in existing snapshot/event messages. Keep current wire
  fields and semantics unchanged.
- Presentation data is declarative: display label, group, ordering, and a
  generic proven role (`main_output_level` or `main_output_mute`). Adapter maps
  ALSA controls to roles; client never maps device IDs or labels itself.
- For writable integer controls, expose an adapter-declared positive step. Do
  not infer a step from current value. Bool controls need no step; values with
  no usable presentation remain hidden from first UI.
- Start with only `main` group and declared main roles. No mixer, inputs,
  advanced controls, user labels, linked groups, profile actions, or preferences.
- Add mock tests proving snapshots/events preserve presentation data and client
  selection needs no device control ID.

**Non-goals**

- No new command type, write path, toolkit, executable, metadata persistence,
  or hardware action.
- No display strings guessed from opaque IDs. If Solo discovery cannot prove a
  role or sensible increment, omit control and record gap.

**Acceptance**

- Mock snapshot identifies zero or more primary level/mute controls entirely
  from capability data.
- Existing IPC v1 clients remain compatible because fields are additive.
- Full Rust verification passes; no Pi interaction required.

### MR 2b proposal: Fullscreen touch client and primary controls

**Scope**

- Add one `focusrite-ui` Rust executable using MR 1 API only. It reconnects to
  configured Unix socket, discards cache on changed `instance_id` or revision
  gap, and requests snapshot before rendering controls.
- Render connection/device status and only writable, available controls from
  MR 2a `main` group. Render absent/unsupported controls nowhere, not disabled
  guesses. Confirmed snapshot/event values are only displayed values; no
  optimistic state remains after reply/error.
- First layout: one status area, one level control, one mute control. Use native
  large touch targets. Cap event redraws at 60 Hz; gestures send at most 30
  commands/second, retaining newest pending value.
- Toolkit decision gate: first run no-control fullscreen spike on actual Pi
  display. Compare `egui/eframe` and `gtk4` only for Pi build, fullscreen/kiosk,
  touch input, binary/runtime size, and needed system packages. Pick one; do
  not add both or web stack. Record resolution, orientation, compositor, and
  selected toolkit before client-control code starts.
- UI disconnect/crash/restart must not alter daemon/device state. UI never
  accesses ALSA, USB, or profiles.

**Verification**

- MR 1 checks plus deterministic mock-server tests for initial snapshot,
  reconnect, changed instance, revision gap, malformed reply, event coalescing,
  confirmed command/error rendering, and 30-command/sec gesture bound.
- Pi fullscreen launch and read-only touch/screen-fit check, then client
  kill/restart and daemon continuity check. Record runtime facts without IDs.
- Command tests use mock device first. Live control write needs separate
  explicit approval and reviewed reversible procedure.

**Hardware action**

Display and touch interaction. Read-only fullscreen test needs explicit
approval. Live control write needs separate explicit approval.

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

- [ ] Daemon exposes versioned, bounded local snapshot/command/event API.
- [ ] Socket clients cannot access USB or ALSA and cannot interrupt daemon on
  failure.
- [ ] Mock IPC tests cover ordering, reconnect/resync, and concurrent local
  client updates.
- [ ] Fullscreen Pi touchscreen exposes only capability-discovered controls and
  remains usable after client restart.
- [ ] Local profile save/list/dry-run/reviewed apply returns safe per-control
  result and never auto-applies.

## Update rule

After each MR, record completed verification, screen/runtime findings, explicit
hardware approvals, and sanitized evidence. Do not expand into metering, LAN,
packaging, or FCP device support.
