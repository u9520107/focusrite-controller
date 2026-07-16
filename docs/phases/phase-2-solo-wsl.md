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
and recovery only. Physical unplug/replug validation is deferred until native
Linux development hardware or Pi validation is available.

Profile startup uses service-owned `/var/lib/focusrited/profiles` by default;
`--profile-store PATH` overrides it. Read-only daemon startup requires
`--card CARD`, and polls state reconciliation through serial device worker.
Loading never applies hardware state.
Initial executable startup validation was blocked because WSL had Solo listed
by `/proc/asound` but no `/dev/snd` nodes. USB/IP/WSL device nodes were restored
without a daemon write.
Device nodes were restored and read-only `focusrited --card 0` ran against the
attached Solo on 2026-07-15 until Ctrl-C, with no daemon output or write.
With explicit approval, one bounded Direct Monitor test discovered its boolean
ALSA control, wrote its opposite value, confirmed it, restored original value,
and confirmed restoration on 2026-07-15. No other control was written.
Profiles bind to adapter-provided device identity and capability-schema version;
they reject mismatches before any command. Phase 2 stops at persistence and
bounded adapter-write validation; local profile save/list/dry-run/apply begins
in Phase 4a, LAN profile operations in Phase 5, and profile-safety acceptance
in Phase 8.
Integer-range and enum-item metadata are deferred to Phase 3 native-Linux
validation; those controls stay explicit but non-writable in Phase 2.

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

- [x] Mock tests cover writes, failures, disconnect/reconnect, and persistence.
- [x] Sanitized Solo discovery fixture records supported controls.
- [ ] Unsupported/unreadable controls remain explicit without taking device offline.
- [x] Solo external-control reconciliation is verified through WSL2 without a
  daemon write.
- [x] Solo USB/IP disconnect/reconnect is verified through WSL2 without a
  daemon write.
- [ ] Physical Solo unplug/replug is deferred to native Linux or Pi validation.
- [ ] Pi and 16i16 work remain deferred to Phases 3 and 7.

## Update rule

Record decisions, blockers, approvals, fixtures, and completed checks here
while Phase 2 is active.
