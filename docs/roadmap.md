# Execution Roadmap

## Phase 0: Planning

Current phase. Maintain docs, decide target OS/screen, verify exact hardware and
firmware. No executable code, package manifests, CI, or deployment artifacts.

Exit: architecture, safety rules, API semantics, and roadmap accepted; 16i16
target Pi OS ARM64 proves FCP install, reboot recovery, and unplug/replug
recovery with sanitized evidence.

## Phase 1: Foundation

Create Rust workspace and Fict web project. Add locked toolchains, formatting,
linting, mock-test baseline, cross-build path, and minimal CI checks.

Exit: WSL reliably builds/lints/tests x86 and arm64 artifacts; no hardware
control implementation yet.

## Phase 2: Hardware discovery spike

Implement read-only capability discovery with mock fixture format. Validate Solo
then 16i16 FCP target setup. Map supported ALSA/FCP controls, events, service
lifecycle, and external/front-panel changes.

Exit: sanitized bounded captures prove required v1 control model; unsupported
controls are explicit; FCP readiness and external mutation behavior are known.

## Phase 3: Daemon and device core

Implement capability model, mock adapter, serialized device worker, state
reconciliation, reconnect, validation, and explicit profile persistence.

Exit: mock tests cover writes, failure, disconnect/reconnect, and persistence.

## Phase 4: API and LAN access

Implement REST snapshot/commands, ticket-authenticated WebSocket events,
instance/revision resync, idempotency, bounded/coalescing queues, token
rotation/revocation, and API integration tests.

Exit: two mock clients converge after concurrent updates, restart/resync,
revision gap, ticket reuse/expiry, and reconnect.

## Phase 5: Touchscreen and web clients

Build fullscreen Rust touch UI and responsive Fict SPA using only API state.
Start with main monitor/output controls; add capability groups after hardware and
screen fit validation.

Exit: hardware controller works from Pi display and phone browser; client crash
does not affect daemon.

## Phase 6: Packaging and acceptance

Package systemd/udev/static assets as arm64 `.deb`. Run Solo and 16i16 matrix:
reboot, unplug/replug, FCP lifecycle recovery, external ALSA/front-panel
reconciliation, profile safety, dangerous-control confirmation, and two-client
last-write-wins.

Exit: clean Pi install provides stable v1 appliance.

## Later

- audio meter stream;
- 18i16 and 18i20 validation;
- multi-device support;
- reverse-proxy/TLS deployment guide;
- Pebble remote client after stable API;
- richer routing/monitor-group UI.

## Deferred decisions

1. Pi OS release/kernel/FCP installation path.
2. Touchscreen resolution, orientation, compositor/kiosk method.
3. Exact device firmware and complete control matrix.
4. Cross-build implementation: container/sysroot versus Zig.
5. Pebble SDK, companion transport, and authentication UX.
