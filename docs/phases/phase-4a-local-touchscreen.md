# Phase 4a: Local Touchscreen

## Status

MR 1 is ready for review and merge on `phase-4a-ipc`. Mock verification and
Pi read-only socket smoke validation are complete. Phase 3 is complete. Do not
begin UI implementation until local API review is accepted.

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

**Evidence — 2026-07-17 (in progress)**

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

### MR 2: Fullscreen touch client and primary controls

**Scope**

- Add smallest Rust touchscreen executable using MR 1 API only. Select UI
  toolkit after confirming it builds and runs fullscreen on target Pi and fits
  actual screen resolution/orientation; do not add a web stack.
- Render connection/device status plus capability-discovered writable primary
  monitor/output controls. Prefer daemon-provided metadata; never hardcode
  Solo control IDs or channel count.
- Use large touch targets, show confirmed values, rate-limit rendering to
  60 Hz, and resync from snapshot after reconnect or event gap. Controls absent
  from capabilities remain absent, not disabled guesses.

**Verification**

- MR 1 checks plus UI unit/mock API tests.
- Pi fullscreen launch, touch hit-target/screen-fit check, client kill/restart,
  and daemon continuity check.
- Any command test uses mock device first; live writes require explicit approval.

**Hardware action**

Display and touch interaction. Live control write only with explicit approval.

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
