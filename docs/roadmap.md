# Execution Roadmap

## Phase 0: Planning — complete

Completed design review. Maintain docs, decide target OS/screen, and record
hardware findings. No hardware control is implemented until Phase 2 discovery.

Exit: architecture, safety rules, API semantics, and roadmap accepted. 16i16
FCP install, reboot recovery, and unplug/replug evidence are deferred hardware
acceptance gates: first validate on direct Linux laptop connection when hardware
access is available, then repeat on target Pi OS ARM64 before claiming 16i16
support.

## Phase 1: Foundation — in progress

Create Rust workspace and Fict web project. Add locked toolchains, formatting,
linting, mock-test baseline, cross-build path, and minimal CI checks.

Active execution record: [Phase 1 foundation plan](phases/phase-1-foundation.md).

Exit: WSL reliably builds/lints/tests x86 and arm64 artifacts; no hardware
control implementation yet.

## Phase 2: Hardware discovery spike — planned

Implement read-only capability discovery with mock fixture format. Validate Solo
first. When 16i16 access is available, validate FCP on direct Linux laptop USB
connection, then repeat target Pi setup. Map supported ALSA/FCP controls, events,
service lifecycle, external/front-panel changes, and any bounded read-only meter
source.

Exit: sanitized bounded captures prove required v1 control model; unsupported
controls are explicit; FCP readiness and external mutation behavior are known.

## Phase 3: Daemon and device core — planned

Implement capability model, mock adapter, serialized device worker, state
reconciliation, reconnect, validation, and explicit profile persistence.

Exit: mock tests cover writes, failure, disconnect/reconnect, and persistence.

## Phase 4a: Local touchscreen — planned

Implement versioned Unix-socket snapshot, command, and event messages, then
build fullscreen Rust touch UI using only that local API. Start with main
monitor/output controls; add capability groups after hardware and screen-fit
validation.

Exit: hardware controller works from Pi display; touchscreen-client crash does
not affect daemon; mock IPC tests cover command ordering, reconnect, and
concurrent local-client updates.

## Phase 4b: Local metering — planned

If Phase 2 discovery identifies a supported, bounded read-only ALSA/FCP meter
source, add capability-discovered meter events and touchscreen rendering. No
audio capture, recording, playback, or host-audio pipeline is added. Devices
without a proven meter source omit the feature.

Exit: supported hardware shows current meter values without affecting command
ordering or device control; unsupported hardware remains fully usable.

## Phase 5: LAN and web access — planned

First accept LAN authentication policy and when TLS becomes required. Then have
`focusrited` serve the LAN listener, REST snapshot/commands, events,
instance/revision resync, idempotency, bounded/coalescing queues, API
integration tests, and the responsive Fict SPA using only API state, including
designated main volume and mute controls.

Exit: phone browser controls hardware on accepted LAN security model; two mock
clients converge after concurrent updates, restart/resync, revision gap, and
reconnect.

## Phase 6: Macro-pad controller — planned

Implement optional USB macro-pad adapter through the existing local command API.
Start with one main output/mix volume and mute encoder plus two linked input-pair
level and mute encoders. Add capability-discovered, user-configured button
actions only after the initial mapping works.

Exit: configured macro-pad controls remain reconciled with touchscreen and web
clients; macro-pad failure or removal does not affect daemon/device operation.

## Phase 7: Packaging and acceptance — planned

Package systemd/udev/static assets as arm64 `.deb`. Run Solo and 16i16 matrix:
reboot, unplug/replug, FCP lifecycle recovery, external ALSA/front-panel
reconciliation, profile safety, dangerous-control confirmation, and two-client
last-write-wins.

Exit: clean Pi install provides stable v1 appliance.

## Later

- 18i16 and 18i20 validation;
- multi-device support;
- Pebble remote client after stable API;
- richer routing/monitor-group UI.

## Deferred decisions

1. Pi OS release/kernel/FCP installation path.
2. Touchscreen resolution, orientation, compositor/kiosk method.
3. Exact device firmware and complete control matrix.
4. Cross-build implementation: container/sysroot versus Zig.
5. Pebble SDK, companion transport, and authentication UX.
