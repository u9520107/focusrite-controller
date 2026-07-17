# Phase 3: Pi Compatibility Verification

## Status

In progress. MR 1 and MR 2 complete; MR 3 is next. Target is prepared Pi OS
ARM64 with Scarlett Solo 4th Gen connected directly by USB. Phase 2 WSL
evidence remains valid only for its WSL scope.

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

### MR 1: Safe native hardware-test entry points — complete

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

Select Solo explicitly with its ALSA card ID rather than relying on card order:

```text
FOCUSRITED_HARDWARE_CARD=Gen cargo test -p focusrited --test scarlett2_alsa -- \
  --ignored --test-threads=1
```

That command runs only ignored read-only hardware tests. A write-capable test
is excluded unless `--features hardware-write-tests` is passed. Even then, run
only after explicit session approval and filter to the intended test:

```text
FOCUSRITED_HARDWARE_CARD=Gen cargo test -p focusrited --test scarlett2_alsa \
  --features hardware-write-tests toggles_direct_monitor_and_restores_it -- \
  --ignored --test-threads=1
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

**Execution plan**

1. On the prepared Pi, confirm the Solo is directly connected and record only
   bounded, sanitized read-only evidence:

   ```text
   uname -a
   lsusb | grep -i focusrite
   cat /proc/asound/cards
   arecord -l
   aplay -l
   amixer -c CURRENT_CARD_INDEX controls
   amixer -c CURRENT_CARD_INDEX contents | sed -n '1,400p'
   ```

   Replace `CURRENT_CARD_INDEX` with the index shown for Solo in the immediately
   preceding `/proc/asound/cards` output; never assume card 0. Use its named
   ALSA ID (for example, `Gen`) for `focusrited --card` and hardware-test
   `FOCUSRITED_HARDWARE_CARD`. Redact serials, usernames, LAN addresses,
   tokens, and unrelated USB/system data before saving any evidence.
2. Build and run local checks before touching Pi hardware:

   ```text
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

3. Run the ignored read-only discovery test against that explicit card:

   ```text
   FOCUSRITED_HARDWARE_CARD=CARD cargo test -p focusrited \
     --test scarlett2_alsa discovers_attached_solo -- --ignored --test-threads=1
   ```

   Confirm it reports only non-writable capabilities, rejects a command
   without a hardware write, and exits successfully.
4. Start daemon with an empty temporary profile-store path, observe successful
   startup, then stop it without issuing a command:

   ```text
   profile_dir=$(mktemp -d)
   profile_store="$profile_dir/profiles"
   timeout --signal=INT 5s cargo run -p focusrited -- --card CARD \
     --profile-store "$profile_store"
   rmdir "$profile_dir"
   ```

   `timeout` ending this foreground daemon is expected. Treat a startup error,
   ALSA access error, or an unexpected file left in the temporary profile store
   as MR2 evidence to investigate.
5. Compare sanitized Pi controls, types, counts, and availability with
   `crates/focusrited/tests/fixtures/scarlett-solo-4th-gen.md`. Change fixture
   or parsing only for a Pi-proven difference; otherwise add a dated evidence
   summary and Pi prerequisites to the hardware/deployment documentation.

**Evidence — 2026-07-17 (complete)**

- Target reports Debian GNU/Linux 13.6 on `aarch64`, kernel
  `6.12.62+rpt-rpi-2712`.
- `/proc/asound/cards` identifies Scarlett Solo 4th Gen as card index 2 with
  ALSA ID `Gen`. Capture and playback both expose device 0.
- Direct Pi ALSA access has `/dev/snd` available and effective `audio` group
  membership. The workspace sandbox intentionally does not mount `/dev/snd`;
  hardware commands must run from a direct Pi terminal or an approved
  unsandboxed runner.
- Bounded `amixer -c 2 controls` and contents report 56 controls. Their IDs,
  types, access shape, enum domains, and integer ranges match Phase 2 fixture.
  Current control values were observed only and are not recorded here.
- `FOCUSRITED_HARDWARE_CARD=Gen` read-only `discovers_attached_solo` passes:
  all discovered capabilities remain non-writable and service commands are
  rejected before hardware write.
- `focusrited --card Gen` starts successfully for five seconds with an empty
  disposable profile-store path. It receives no command; temporary directory
  removal confirms no profile file was written.
- No ALSA control, profile, fixture, or adapter code changed. MR3 may capture
  metadata and verify an operator-initiated external Direct Monitor change.

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

- [x] Read-only and write-capable hardware tests are structurally separated;
  write tests require explicit feature plus session approval.
- [x] Solo service builds and starts natively on Pi against selected ALSA card.
- [x] Sanitized Pi discovery proves capabilities and documented prerequisites.
- [ ] External/front-panel change reconciles into authoritative service state.
- [ ] Physical unplug/replug recovers to a fresh online snapshot.
- [ ] Pi reboot returns daemon and Solo readiness without device-state mutation.
- [ ] Target-specific limits, commands, and unresolved issues are documented.

## Update rule

After every MR, record completed checks, operator approvals, sanitized evidence,
defects found, and any changed prerequisite. Do not expand scope into touchscreen,
LAN, packaging, or 16i16/FCP work; those belong to later phases.
