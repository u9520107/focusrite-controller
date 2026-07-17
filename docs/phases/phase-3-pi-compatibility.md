# Phase 3: Pi Compatibility Verification

## Status

Planned. Target is prepared Pi OS ARM64 with Scarlett Solo 4th Gen connected
directly by USB. Phase 2 WSL evidence remains valid only for its WSL scope.

## Goal

Prove current Solo service works on target Pi Linux without changing device
state by default. Record target prerequisites and fix only Pi-specific defects
found by that evidence.

## Guardrails

- Run mock/unit checks before every hardware check.
- Begin every hardware session with bounded, read-only discovery.
- Hardware writes, routing/clock changes, resets, firmware work, profile apply,
  and physical unplug/replug require explicit approval for that session.
- Redact serials, usernames, LAN addresses, tokens, and unrelated logs before
  adding fixtures or documentation.
- Use a named ALSA card or stable card selector; never assume Solo is card 0.
- Each MR uses its own branch, is independently reviewable, and never directly
  edits, commits, or pushes `main`.

## Merge-request plan

### MR 1: Safe native hardware-test entry points

**Scope**

- Separate existing Solo tests into read-only discovery/reconciliation/reconnect
  coverage and write-capable coverage.
- Gate write-capable coverage behind an explicit Cargo feature.
- Make the selected hardware card explicit for hardware tests and commands.
- Document exact read-only and write-capable invocation commands.

**Verification**

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- Confirm an ignored-test run without the write feature cannot compile or run a
  device-writing test.

**Hardware action**

None. Do not run ignored tests in this MR.

**Commands after MR 1**

Pi Solo is currently ALSA card 2. Select it explicitly rather than relying on
card order:

```text
FOCUSRITED_HARDWARE_CARD=2 cargo test -p focusrited --test scarlett2_alsa -- --ignored
```

That command runs only ignored read-only hardware tests. A write-capable test
is excluded unless `--features hardware-write-tests` is passed. Even then, run
only after explicit session approval and filter to the intended test:

```text
FOCUSRITED_HARDWARE_CARD=2 cargo test -p focusrited --test scarlett2_alsa \
  --features hardware-write-tests toggles_direct_monitor_and_restores_it -- --ignored
```

### MR 2: Pi read-only discovery and capability evidence

**Scope**

- Run bounded Pi read-only probes: kernel, USB identity, ALSA card/device list,
  controls, and bounded control contents.
- Run Solo discovery against selected Pi ALSA card.
- Compare discovery shape with Phase 2 fixture; update sanitized fixture or
  adapter parsing only when Pi evidence proves a difference.
- Record Pi OS/kernel, required packages, device access/group requirements, and
  stable card-selection guidance.

**Verification**

- MR 1 checks.
- Read-only hardware discovery test.
- Read-only daemon startup using `focusrited --card CARD`, then clean stop.

**Hardware action**

Read-only only. No control write or stored-profile apply.

### MR 3: Native reconciliation and controlled write metadata

**Scope**

- Verify external Direct Monitor/front-panel change reaches service state and
  advances revision without daemon write.
- Capture integer ranges and enum items exposed by Pi ALSA.
- Make only evidence-backed metadata/validation changes; unsupported domains
  remain explicit and non-writable.

**Verification**

- MR 1 checks.
- Read-only external-change test with operator interaction.
- Add or update mock tests for each parsing/validation defect found.

**Hardware action**

Read-only daemon behavior. Operator may change front-panel Direct Monitor;
daemon must not write any control.

### MR 4: Native lifecycle recovery

**Scope**

- Verify service behavior through physical Solo unplug/replug and Pi reboot.
- Diagnose USB power, ALSA-node, service-startup, and recovery ordering issues.
- Add the smallest code or deployment change proven necessary.
- Record sanitized lifecycle evidence and known limits.

**Verification**

- MR 1 checks.
- Read-only physical disconnect/reconnect test.
- Read-only daemon start after reboot and recovery after reattach.

**Hardware action**

Physical cable unplug/replug and Pi reboot require session approval. No ALSA
write, routing, clock, reset, firmware, or profile apply.

### MR 5: Phase 3 closeout

**Scope**

- Consolidate Pi prerequisites and verified commands in deployment/hardware
  documentation.
- Mark Phase 3 evidence and unresolved limits.
- Add only regression tests for defects actually found in MRs 1–4.

**Verification**

- Full Rust verification: format, Clippy with warnings denied, and workspace
  tests.
- Repeat one bounded read-only discovery and daemon-start check on Pi.

**Hardware action**

Read-only only.

## Exit checks

- [ ] Read-only and write-capable hardware tests are structurally separated;
  write tests require explicit feature plus session approval.
- [ ] Solo service builds and starts natively on Pi against selected ALSA card.
- [ ] Sanitized Pi discovery proves capabilities and documented prerequisites.
- [ ] External/front-panel change reconciles into authoritative service state.
- [ ] Physical unplug/replug recovers to a fresh online snapshot.
- [ ] Pi reboot returns daemon and Solo readiness without device-state mutation.
- [ ] Target-specific limits, commands, and unresolved issues are documented.

## Update rule

After every MR, record completed checks, operator approvals, sanitized evidence,
defects found, and any changed prerequisite. Do not expand scope into touchscreen,
LAN, packaging, or 16i16/FCP work; those belong to later phases.
