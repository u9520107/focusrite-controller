# Execution Roadmap

## Phase 0: Planning — complete

Completed design review. Maintain docs, decide target OS/screen, and record
hardware findings. No hardware control is implemented until Phase 2 discovery.

Exit: architecture, safety rules, API semantics, and roadmap accepted. 16i16
FCP install, reboot recovery, and unplug/replug evidence are deferred hardware
acceptance gates: first validate on direct Linux laptop connection when hardware
access is available, then repeat on target Pi OS ARM64 before claiming 16i16
support.

## Phase 1: WSL Rust foundation — complete

Create Rust workspace. Add locked WSL toolchain, formatting, linting,
mock-test baseline, and minimal CI checks. Web setup is deferred to Phase 5
while Fict resolves its published-package issue.

Execution record: [Phase 1 foundation plan](phases/phase-1-foundation.md).

Exit: WSL reliably formats, lints, and tests Rust workspace; no hardware
control or cross-compilation implementation.

## Phase 2: Solo service development in WSL — complete (WSL scope)

Execution record: [Phase 2 Solo WSL plan](phases/phase-2-solo-wsl.md).

Route Scarlett Solo directly into WSL2. Implement bounded read-only discovery,
mock fixtures, capability model, device worker, state reconciliation,
reconnect, validation, and explicit profile persistence. Validate Solo controls
and external/front-panel changes through WSL2, while treating it as development
evidence only.

Phase 2 owns profile storage, device/schema binding, and bounded adapter write
validation. It does not add user-facing save/list/dry-run/apply operations or
general multi-control hardware application.
Integer-range and enum-item discovery, plus controlled writes for those
domains, are deferred to Phase 3 native-Linux validation. Until then, those
domains remain explicit but non-writable.

Exit: mock and Solo-on-WSL tests cover supported control behavior, failure,
disconnect/reconnect, and persistence; unsupported controls are explicit.

## Phase 3: Pi compatibility verification — complete pending review and merge

Execution plan: [Phase 3 Pi compatibility plan](phases/phase-3-pi-compatibility.md).

Develop directly in a local Pi session, then validate current Solo service
natively on Pi Linux. SSH/Zed Remote Development is optional. Find and fix
target-only build, ALSA, USB, system-service, reboot, and unplug/replug issues.
Cross-compilation may be introduced only if native Pi development demonstrates a
real need.

MR 1 and MR 2 are complete. MR 3 adds event-driven ALSA state reconciliation;
GUI clients later cache state and cap rendering at 60 Hz. MR 4 validates
lifecycle recovery and closes Phase 3.

Before adding Phase 3 hardware coverage, split Solo tests into a read-only
hardware suite (discovery, external changes, reconnect) and a write-capable
suite. Gate the write-capable suite behind an explicit Cargo feature so running
ignored tests cannot mutate hardware accidentally.

Exit: Solo service runs reliably on prepared Pi; target-specific limits and
deployment prerequisites are recorded.

## Phase 4a: Local touchscreen — planned

Implement versioned Unix-socket snapshot, command, and event messages, then
build fullscreen Rust touch UI using only that local API. Start with main
monitor/output controls; add capability groups after hardware and screen-fit
validation. Add local profile save/list, binding/diff dry-run, reviewed apply,
and per-control applied/skipped/failed results.

Exit: hardware controller works from Pi display; touchscreen-client crash does
not affect daemon; mock IPC tests cover command ordering, reconnect, and
concurrent local-client updates.

## Phase 4b: Local metering — planned

If Phase 2 or Phase 7 discovery identifies a supported, bounded read-only
ALSA/FCP meter source, add capability-discovered meter events and touchscreen
rendering. No audio capture, recording, playback, or host-audio pipeline is
added. Devices without a proven meter source omit the feature.

Exit: supported hardware shows current meter values without affecting command
ordering or device control; unsupported hardware remains fully usable.

## Phase 5: LAN and web access — planned

First accept LAN authentication policy and when TLS becomes required. Then have
`focusrited` serve the LAN listener, REST snapshot/commands, events,
instance/revision resync, idempotency, bounded/coalescing queues, API
integration tests, and the responsive Fict SPA using only API state, including
designated main volume and mute controls. Establish the web toolchain here:
pin Node through `.nvmrc`/fnm and pnpm through Corepack, then add a verified
Fict release with Vite static builds, compatible TypeScript, Biome, and Vitest.
`focusrited` serves the resulting static assets; Vite is not a production
server dependency. Extend profile operations to LAN clients using normal
idempotency and confirmation rules.

Exit: phone browser controls hardware on accepted LAN security model; two mock
clients converge after concurrent updates, restart/resync, revision gap, and
reconnect.

## Phase 6: Optional macro-pad controller — planned

Implement optional USB macro-pad adapter through the existing local command API.
Start with one main output/mix volume and mute encoder plus two linked input-pair
level and mute encoders. Add capability-discovered, user-configured button
actions only after the initial mapping works. A proven front-panel-to-optical
mirror binding is the no-extra-hardware path for optical monitoring; macro-pad
remains an optional additional control surface.

Exit: configured macro-pad controls remain reconciled with touchscreen and web
clients; macro-pad failure or removal does not affect daemon/device operation.

## Phase 7: 16i16 verification and hardening — planned

When hardware becomes available, run bounded read-only discovery on direct
Linux first, then validate FCP, `fcp-server`, ALSA controls, lifecycle, and
reconciliation on Pi. Harden service behavior from those findings. Solo success
does not imply 16i16 routing or monitor-group support.

Exit: sanitized 16i16 evidence proves supported controls and Pi FCP readiness;
service handles FCP recovery, external changes, and documented unsupported
capabilities.

## Phase 8: Packaging and acceptance — planned

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
4. Cross-build implementation, if Pi validation shows native deployment needs it.
5. Pebble SDK, companion transport, and authentication UX.
