# Phase 2: Solo Service Development in WSL

## Status

Active on branch `phase-2-solo-wsl`.

Completed: WSL2 USB handoff documented; sanitized read-only Solo control
[fixture](../../crates/focusrited/tests/fixtures/scarlett-solo-4th-gen.md)
captured; `Scarlett2Alsa` read-only discovery passes against the attached Solo;
device-independent service policy core has mock coverage for confirmed writes,
hardware failure, reconciliation, disconnect/reconnect, and explicit in-memory
profile apply; a bounded serial device worker owns all blocking device calls;
external front-panel Direct Monitor state change is reconciled through WSL2
without a daemon write.
USB/IP detach/re-attach recovery also passed: offline state advanced its
revision, then recovery resnapshotted the Solo. This validates WSL2 device loss
and recovery only; it does not replace later physical unplug evidence.

Next: use observed capabilities to extend state reconciliation. Disk-backed
profile persistence and local IPC remain later Phase 2/4 work respectively.

Dependency decision: direct ALSA access uses `alsa` 0.12.0 (MIT OR Apache-2.0),
reviewed compatible with this project's MIT distribution. It needs `pkg-config`
and ALSA development headers in the development environment.

## Goal

Develop and test the Rust service with mocks and a Scarlett Solo attached
directly to WSL2. This is development evidence, not Pi compatibility proof.

## Scope

- Add bounded read-only Solo discovery and sanitized mock fixtures.
- Implement capability discovery, serialized device work, state reconciliation,
  reconnect handling, validation, and explicit profile persistence.
- Test supported Solo controls, external changes, and disconnect/reconnect.

## Guardrails

- Start with mock tests, then read-only hardware discovery.
- Do not write device state, change routing/clock, reset, or update firmware
  without explicit approval.
- Redact serials, tokens, home addresses, and unrelated system details from
  saved captures.
- Treat WSL2 results as insufficient for Pi or 16i16 support claims.

## Exit checks

- [ ] Mock tests cover writes, failures, disconnect/reconnect, and persistence.
- [x] Sanitized Solo discovery fixture records supported controls.
- [x] Solo external-control reconciliation is verified through WSL2 without a
  daemon write.
- [x] Solo USB/IP disconnect/reconnect is verified through WSL2 without a
  daemon write.
- [ ] Pi and 16i16 work remain deferred to Phases 3 and 7.

## Update rule

Record decisions, blockers, approvals, fixtures, and completed checks here
while Phase 2 is active.
