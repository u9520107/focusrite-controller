# Phase 3: Pi Compatibility Verification

## Status

Complete pending review and merge. Target is prepared Pi OS ARM64 with Scarlett
Solo 4th Gen connected directly by USB. Phase 2 WSL evidence remains valid
only for its WSL scope.

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
- No ALSA control, profile, fixture, or adapter code changed. Pi metadata
  already matches the Phase 2 fixture; MR3 therefore adds event-driven
  reconciliation before lifecycle recovery.

### MR 3: Event-driven Pi state reconciliation

**Scope**

- Keep a persistent ALSA control-event source for the selected card; reconcile
  external state on its events rather than using periodic full snapshots as the
  normal control-sync path.
- Preserve `focusrited` as sole device owner: event handling performs no
  command or ALSA write.
- Reconcile every received ALSA control event immediately. Do not throttle or
  coalesce service state; each event updates authoritative state.
- GUI clients cache incoming state and cap rendering at 60 Hz. This is a UI
  concern, not a service delivery limit. Meter display remains
  capability-discovered until Pi ALSA evidence proves a source.
- Retain a bounded periodic health check only to detect missed events and
  device loss, at a three-second interval. It is not the normal
  state-synchronization mechanism.

**Verification**

- MR 1 checks.
- Full Rust verification: format, Clippy with warnings denied, and workspace
  tests.
- Mock coverage for event ordering, high-rate event handling, missed-event
  health recovery, and no-write behavior.
- Read-only Pi external-change test proves event-driven state/revision update.

**Execution plan**

1. Add the smallest persistent event loop that can service both ALSA events and
   serialized worker requests. Do not add a second device owner or a polling
   thread for normal state updates.
2. Reconcile event-driven changes immediately and retain a three-second health
   fallback. Keep GUI cache/render throttling out of the service.
3. Add mock checks before Pi use. Then, with explicit approval for an
   operator-initiated front-panel state change, prove the event path on `Gen`
   without a daemon command or ALSA write.
4. Record sanitized event capability, state/revision evidence, measured update
   timing, and any unsupported meter source.

**Hardware action**

Operator front-panel Direct Monitor change requires explicit session approval.
No daemon ALSA write, routing, clock, reset, firmware, or profile apply.

**Pre-event evidence — 2026-07-17**

- With explicit approval, the read-only
  `reconciles_external_direct_monitor_change` test observed an
  operator-initiated Direct Monitor change on card `Gen`. The authoritative
  service state changed and revision advanced from 1 to 2 within 30 seconds.
- The test issued no service command or daemon ALSA write, but used manual
  250 ms polling; it does not validate the planned event path. Integer ranges
  and enum domains remain unchanged from the Pi MR2 fixture comparison.

**Event evidence — 2026-07-17**

- With explicit approval, the same read-only test passed after its polling
  refresh was removed. A front-panel Direct Monitor change on `Gen` updated
  authoritative service state and advanced revision from 1 to 2. The test made
  state requests only; reconciliation was initiated by the ALSA event loop.
- The test bounds detection to 30 seconds but does not measure operator-action
  to event-delivery latency. Meter-event availability and rate remain Phase 4b
  discovery work.
- Worker request handling is bounded to four requests per turn before a
  zero-wait ALSA event check and health-check opportunity. Mock coverage proves
  queued client requests cannot starve event reconciliation.

### MR 4: Pi lifecycle recovery and Phase 3 closeout

**Scope**

- Verify event-driven service behavior through physical Solo unplug/replug and
  Pi reboot.
- Diagnose only observed USB power, ALSA-node, event-source, and startup
  recovery failures.
- Add the smallest evidence-backed fix, regression check, and deployment
  documentation necessary; record limits and close Phase 3 if exit checks pass.

**Verification**

- MR 1 checks and full Rust verification.
- Read-only physical disconnect/reconnect and post-reboot daemon-start checks.
- Repeat one bounded event-driven external-change check on Pi.

**Hardware action**

Physical cable unplug/replug and Pi reboot each require explicit session
approval. No ALSA write, routing, clock, reset, firmware, or profile apply.

**Evidence — 2026-07-17 (in progress)**

- With explicit approval, the read-only `reconnects_after_solo_disconnect` test
  observed Solo offline at revision 3, then a fresh online snapshot at revision
  4 after reconnect. The test requested state only; recovery came from the
  worker event/health path.
- While unplugged, two bounded ALSA card-absence diagnostics occurred during
  recovery probing. The worker did not repeatedly reopen ALSA or flood logs.
- No daemon command or ALSA write occurred.
- With explicit approval, Pi rebooted successfully. Post-reboot read-only
  `discovers_attached_solo` passed on card `Gen`; five-second `focusrited`
  startup received no command and left its disposable profile store empty.

## Exit checks

- [x] Read-only and write-capable hardware tests are structurally separated;
  write tests require explicit feature plus session approval.
- [x] Solo service builds and starts natively on Pi against selected ALSA card.
- [x] Sanitized Pi discovery proves capabilities and documented prerequisites.
- [x] External/front-panel change reconciles into authoritative service state.
- [x] Physical unplug/replug recovers to a fresh online snapshot.
- [x] Pi reboot returns daemon and Solo readiness without device-state mutation.
- [x] Target-specific limits, commands, and unresolved issues are documented.

## Update rule

After every MR, record completed checks, operator approvals, sanitized evidence,
defects found, and any changed prerequisite. Do not expand scope into touchscreen,
LAN, packaging, or 16i16/FCP work; those belong to later phases.
