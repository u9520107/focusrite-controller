# Focusrite Controller Agent Guide

## Current phase

Planning only. Do not add application code, package manifests, CI workflows, or
deployment scripts until an implementation phase is explicitly requested.

## Product boundaries

- `focusrited` is sole product policy and API authority. Clients never open or
  control USB/ALSA. On FCP devices, `fcp-server` owns its device protocol and
  creates ALSA controls; `focusrited` consumes those controls, monitors FCP
  readiness, fails closed when unavailable, and reconciles external changes.
- Hardware writes need capability, range, and safety validation.
- Never perform live hardware writes, firmware updates, factory resets, or
  change clock/routing settings without explicit user approval.
- Preserve device state by default. Applying a stored profile must be explicit.
- Treat USB captures, service logs, device serials, LAN tokens, and home IP
  addresses as sensitive. Redact them before committing fixtures or docs.

## Planned repository areas

- `crates/`: Rust daemon, touchscreen, shared protocol, device adapters.
- `web/`: Fict TypeScript SPA.
- `pebble/`: future Pebble controller client; no dependency on v1.
- `docs/`: architecture and decisions. Keep these current when scope changes.
- `tests/fixtures/`: sanitized hardware-control snapshots only.

## Development rules after planning phase

- Run mock/unit tests before hardware tests.
- Hardware validation runs on target Linux hardware; QEMU cannot validate USB
  control behavior.
- First targets: `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu`.
- Keep device support capability-discovered. Do not hardcode Solo control names
  into shared models or UI.
- Prefer small, standard-library solutions. Do not add abstractions before a
  second real caller needs them.

## Required checks once code exists

- Rust: format check, Clippy with warnings denied, tests.
- Web: lint, strict typecheck, tests, production build.
- API: mock-device integration coverage for ordering, reconnect, and concurrent
  client updates.
