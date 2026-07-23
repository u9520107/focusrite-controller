# Phase 4c: Dashboard Configuration and Virtual Groups

## Status

In progress. MR 1 delivers versioned, device-bound dashboard metadata and
read-only CLI import/export. MR 2a/2b deliver persisted mock-only relative
groups through service, worker, local IPC, and touchscreen. Remaining MR 2
slices add adaptive adapter declarations, native/device-specific operations,
and synchronized sets. MR 2e persists validated disabled-by-default mirrors;
confirmed source commands and source events map through serial worker, confirm
their targets, and return per-target applied/skipped/failed results. This phase
does not add meters, LAN serving, browser UI, or unapproved hardware routing
changes.

## Goal

Let `focusrited` own portable per-device dashboard configuration and safe
virtual groups. A configured group appears as an additional local touchscreen
track while individual members remain independently available. Users may manage
configuration through validated CLI import/export before Phase 5 web editing.

## Scope and guardrails

- Configuration is daemon-owned, versioned, atomic, per-device metadata.
  Import/export never writes hardware.
- Dashboard has at most 12 visible items. Daemon validates this limit for API,
  CLI import, and all future clients; UI limit indicators are advisory only.
- Dashboard config controls visibility, order, label, and selected virtual
  tracks. It is separate from profiles and never applied as device state.
- Per-control cut mode is persisted configuration: `hardware` requires a
  discovered mute companion; `panic_cut` writes the declared level minimum and
  is labelled Cut; `none` renders no cut button. Panic-cut Restore is deferred
  until persisted last-confirmed-level semantics are specified.
- Group operations require declared operation compatibility. Compatible level
  controls may span input and output sides when user intent warrants it; mute
  and future operations validate members independently. Do not group level with
  phantom power, gain, clock, routing, or another dangerous domain.
- Control groups and synchronized level sets contain discovered leaf controls
  only. They never contain another group or set; dashboard collections may
  later nest for organization but have no control semantics.
- Native hardware group/link support wins. Non-native group writes are
  validated, ordered compound commands: no atomicity claim, per-member
  confirmation and applied/skipped/failed report required.
- A mirror binding is separate from a virtual group: one confirmed source-level
  change writes its mapped targets directly. One-way mirrors are explicit,
  off by default, capability-declared fan-out bindings; targets cannot become
  mirror sources. They may cross input/output only where adapter declares
  compatible safe mapping.
- A synchronized level set is separate explicit binding type, not a collection
  of mirrors. Any confirmed member change drives mapped every other member
  through serial worker. It supports physical main-monitor knob to optical
  speaker-fader synchronization, then may expand to arbitrary compatible level
  controls. Set rejects overlap with other synchronization sets, suppresses
  write echoes, and serializes simultaneous changes as last confirmed change
  wins.
- Relative level balance is preserved for compatible non-native level groups.
- Solo 4th Gen Direct Monitor source-level and A/B-balance mapping is deferred.
  It is not needed for current personal workflows. Its eight raw ALSA gain
  cells remain adapter-private and are never dashboard tracks.
- Solo's four Direct Monitor sources feed one shared monitor/headphone mix.
  The physical line-output and headphone paths are duplicated post-mix, with
  their own hardware knobs; they are not two software output tracks and must
  never appear as duplicate dashboard controls.
- Group, mirror, and synchronized-set level mapping uses a canonical normalized
  position. Map through declared dB ranges when both sides provide them;
  otherwise map through each integer minimum/maximum. Raw values remain the
  confirmed device/profile representation.
  Individual members stay independently controllable and may be shown beside
  their virtual group.
- Local touchscreen displays configured groups and may change visibility/order
  only if that interaction remains keyboard-free. It does not create, rename,
  or edit group membership.
- CLI is first configuration editor: validated export/import and inspect. Phase
  5 web UI adds group creation/membership editing. No live config file watcher.
- No hardware write during Pi setup. Mock tests cover group commands; any live
  group write needs explicit approval and reviewed restoration plan.

## Merge-request plan

### MR 1: Versioned dashboard configuration

**MR 1a complete**

- `dashboard.json` schema version 1 binds a control list and optional custom
  labels to device identity plus capability schema. It accepts only unique
  adapter-presented leaf controls and caps list length at 12.
- Missing file uses adapter defaults in memory without creating a file. A
  present malformed, mismatched, or unavailable-control file rejects daemon
  startup before IPC begins; it never replaces a valid in-memory config.
- Save validates before same-directory atomic replacement. CLI import/export
  operates on metadata only and exits before daemon/socket startup; it has no
  hardware command path.
- Daemon state and existing IPC snapshot/event/command-result messages carry
  additive `dashboard` metadata. Touch UI renders this configured order and
  custom strip labels, while retaining its existing safety checks.

**MR 1b complete**

- `focusrited --card CARD --dashboard-inspect` prints current stored dashboard
  or read-only adapter defaults as JSON. `--dashboard-export PATH` writes that
  same validated JSON to a chosen path. `--dashboard-import PATH` reads a
  candidate, validates it against current read-only discovery, then atomically
  replaces only daemon dashboard metadata. Actions are mutually exclusive and
  exit before daemon/socket startup. No file watcher or touchscreen text editor.

**Scope**

- Define versioned configuration schema for dashboard items, optional labels,
  visibility/order, and stable per-device binding.
- Load/save atomically through daemon-owned store; invalid files fail closed
  without replacing active valid config.
- Add read-only configuration inspect/export and validated import CLI commands.
  Import validates schema, device binding, at-most-12 visible dashboard items,
  duplicate names, leaf control IDs,
  and rejects group/set references as members,
  then atomically replaces configuration only on success.
- CLI import is initial custom-label editor. Phase 5 web UI adds text input for
  labels; touchscreen renders configured labels and does not require keyboard.
- Extend local IPC snapshots with configured dashboard metadata. Do not add
  group commands or touchscreen edit controls in this MR.

**Verification**

- Full Rust checks plus mock tests for round trip, corrupt/unknown schema,
  device mismatch, invalid IDs, atomic failed import, and no hardware writes.
- Test fixtures redact serials and tokens.

### MR 2: Virtual group service semantics

**MR 2a/2b complete — mock-only relative groups**

- Core mock-only validation accepts two-or-more unique discovered writable
  integer leaf controls with declared bounds. It maps canonical `0..=1000`
  positions independently into unequal ranges and executes ordered,
  per-member-confirmed service commands. It has no native adapter declaration
  or live hardware path yet.
- MR 2b defines relative-level behavior: each group names one member as its
  anchor; a command target is anchor's canonical `0..=1000` position. Daemon
  snapshots all members once, applies target-anchor delta to each normalized
  member position, clamps only at `0`/`1000`, maps to native integer bounds,
  then confirms writes in configured order. No-op members are reported skipped;
  first failed write stops operation with no rollback. Clipping is explicit and
  may change relative balance. Worker and local IPC return authoritative state
  plus applied/skipped/failed member result.
- Dashboard config schema v2 adds named level-group metadata while accepting
  v1 files with no groups. Group import/export validates discovery and writes
  no hardware; groups count toward the twelve visible dashboard-item limit.
- Group membership now fails closed unless every member's adapter capability
  explicitly declares `relative_level`. Current ALSA discovery declares none;
  mock-only groups remain available for service, IPC, and UI coverage until an
  adapter has reviewed level-control declarations.
- Each group command revalidates persisted dashboard device/schema binding
  against the current authoritative snapshot. Capability drift rejects the
  command before any member write.
- Adapter mappings are structural and adaptive: use discovered IDs, bounds,
  steps, and unique control-relationship shape at runtime. Fingerprints aid
  diagnostics but do not reject benign driver/firmware drift. Withhold a
  logical writable capability only for missing, contradictory, or ambiguous
  shape; never guess between candidates.

**Scope**

- Add capability-declared eligible member role/type and virtual group model.
- Extend `MockDevice` fixture metadata so arbitrary compatible level controls
  can declare normalized mapping; validate group/synchronization theory without
  requiring Solo or 16i16 hardware support.
- Validate input/output separation, operation compatibility, membership
  uniqueness, leaf-only membership, and dangerous-domain exclusion.
- Implement native delegation where available; otherwise validated ordered level
  and mute group commands through sole serial `DeviceWorker`.
- Confirm each member canonical value, preserve relative balance for level
  groups, and return per-member result. Reconcile external changes normally.
- Add validated one-way mirror bindings: source/target eligibility, declared
  mapping, explicit enablement, cycle rejection, target confirmation, and safe
  partial-write error. Mirror writes enter same serial worker path; no direct
  adapter/UI write or bidirectional propagation.
- Add validated synchronized level sets for capability-declared level mappings.
  Each member maps to/from one normalized canonical level, avoiding inconsistent
  pairwise transforms. Track set generation/write origin/expected confirmation
  to prevent echo loops. Target failure leaves source canonical state intact and
  reports set degraded rather than rolling source back. Do not retry
  automatically until next user or external confirmed member change.
- Extend local IPC with explicit group command/result messages only after mock
  service behavior is complete.

### MR 2c: Adaptive adapter declarations

**First slice complete — read-only integer metadata**

- Scarlett2 discovery now retains ALSA-declared minimum, maximum, and positive
  step for single integer controls, and delivers its bounds in existing
  capabilities. It remains read-only and declares no `relative_level` support.
- This uses direct `alsa-sys 0.6.0` (MIT) only for three missing safe-wrapper
  `ElemInfo` accessors; it is already transitive through `alsa 0.12.0`.

**Plan**

1. Extend read-only ALSA/FCP discovery to retain integer bounds and real step
   metadata; no write enablement in this slice.
2. Define adapter-local structural rules from control type/count/access,
   companions, and complete relationship shape. Resolve current IDs/ranges at
   runtime; do not bind rules to card index, numid, firmware version, or exact
   fingerprint.
3. Require exactly one complete match before emitting `relative_level` or a
   future compound operation. Missing, contradictory, or ambiguous matches
   remain read-only/unavailable with sanitized diagnostic reason.
4. Add sanitized fixtures proving one recognized shape plus benign numid/range
   drift. Add negative fixtures for missing and ambiguous shapes.
5. Keep Solo Direct Monitor raw cells adapter-private. Its logical source
   operation is deferred until an actual personal workflow needs it.

**Verification**

- Mock fixture tests show ID/order/range drift preserves a unique mapping.
- Missing or ambiguous shape emits no writable logical capability and performs
  no write.
- Any hardware session remains read-only unless separately approved with a
  restore plan.

### MR 2d: Native and device-specific group operations

**Plan**

1. Add adapter-owned native group delegation only where discovery proves an
   explicit hardware link/control. Never emulate a declared native operation.
2. Do not add Solo Direct Monitor logical source operations in this phase;
   keep all raw matrix cells hidden and adapter-private.
3. Return the same ordered applied/skipped/failed result shape for native and
   compound operations. Do not claim atomicity unless hardware proves it.

**Verification**

- Mock native and compound paths cover success, partial failure, external
  change, reconnect, and individual member command after group command.
- Any live operation requires explicit approval, recorded starting values, and
  a reviewed restore plan.

### MR 2e: One-way mirror bindings

**Complete — mock-only runtime semantics**

- Dashboard schema v3 persists source, target, and disabled-by-default state.
  It accepts only adapter-declared relative-level controls, rejects self maps,
  repeated sources, and all cycles.
- Confirmed source commands and external source events flow through serial
  worker. Each target mapping uses normalized integer bounds, confirms state,
  and returns applied/skipped/failed result without rolling back source.
- Target writes refresh authoritative state before later event reconciliation,
  so expected target echoes do not re-trigger a mapping. Touchscreen shows a
  target-failure toast.

**Verification**

- Mock source event, disabled binding, target failure, cycle rejection, and
  reconnect coverage.

### MR 2f: Synchronized level sets

**Plan**

1. Persist explicit compatible members and one canonical normalized mapping.
2. Propagate any confirmed member change through serial worker; prevent echo
   loops with generation/origin/expected-confirmation tracking.
3. Reject overlapping sets; serialize concurrent updates as last confirmed
   change wins; report degraded target failure without automatic retry.

**Verification**

- Mock main-knob event, UI event from any member, three-or-more members,
  unequal ranges, echo suppression, concurrent changes, and degraded failure.

**Verification**

- Mock coverage for native path, invalid/mixed membership, ordered success,
  partial failure, external member change, reconnect, concurrent clients,
  individual member command after group command, mirror source event, mirror
  target failure, disabled binding, and cycle rejection; synchronized-set
  main-knob event, UI event from any member, three-or-more members, unequal-
  range mapping, echo suppression, concurrent changes, and degraded target
  failure.
- No Pi group write. Any later live test uses pre-recorded values, explicit
  approval, and manual restore confirmation.

### MR 3: Configured touchscreen dashboard

**Scope**

- Render configured virtual tracks and optional individual tracks in compact
  grid; preserve existing Focus interaction and snapshot-resync behavior.
- Support keyboard-free visibility/order controls only if screen-fit review
  accepts them. Group definition remains CLI/web only.
- Hide unconfigured controls. Empty dashboard leads to Settings/configuration
  guidance, never guessed controls.

**Verification**

- Mock API/UI tests for group appearance, individual/group coexistence,
  command result/error, reconnect, changed instance, and max 12 strips.
- Pi display/touch check is read-only unless separately approved.

## Exit checks

- [ ] Validated CLI export/import round-trips portable dashboard metadata with
  no hardware write.
- [ ] Dashboard config survives restart and device reconnect without applying
  profiles or device state.
- [ ] Virtual input/output groups are validated, capability-limited, and report
  per-member non-atomic results.
- [ ] Explicit one-way mirror bindings use declared mappings, cannot form loops,
  and report target failure without corrupting source state.
- [ ] Synchronized level sets converge mapped member values without echo loops;
  a physical monitor-knob change can drive declared optical fader.
- [ ] Individual and virtual tracks remain independently usable.
- [ ] Touchscreen renders configured dashboard without exposing membership
  editor or unrelated discovered controls.

## Deferred

- Browser group editor and LAN configuration API: Phase 5.
- Custom keyboard/text entry on touchscreen.
- Routing, clock, phantom-power, and cross-domain group operations.
- Automatic file watching/reloading of manually edited config.
