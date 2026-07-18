# Phase 4c: Dashboard Configuration and Virtual Groups

## Status

In progress. MR 1a implements versioned, device-bound leaf-control metadata,
atomic persistence, daemon validation, and additive local-IPC delivery. Import
CLI and group definitions remain separate reviewable work. This phase is
required for configured dashboard tracks and linked input/output controls; it
does not add meters, LAN serving, browser UI, or hardware routing changes.

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
- A mirror binding is separate from a virtual group: a confirmed source-level
  change writes mapped target level automatically. One-way mirrors are explicit,
  off by default, capability-declared, and reject cycles. They may cross
  input/output only where adapter declares compatible safe mapping.
- A synchronized level set is separate explicit binding type, not a collection
  of mirrors. Any confirmed member change drives mapped every other member
  through serial worker. It supports physical main-monitor knob to optical
  speaker-fader synchronization, then may expand to arbitrary compatible level
  controls. Set rejects overlap with other synchronization sets, suppresses
  write echoes, and serializes simultaneous changes as last confirmed change
  wins.
- Relative level balance is preserved for compatible non-native level groups.
- Solo 4th Gen Direct Monitor is a special mapped group: eight raw ALSA gain
  cells form two output sides by four documented sources. Raw cells are never
  dashboard tracks. Adapter declares the source-to-cell operation and its
  balance/pan preservation rule; `USB Playback 1/2` can then appear as initial
  tracks, while Analogue 1/2 remain separate soft-disabled candidates.
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

**MR 2a in progress**

- Core mock-only validation accepts two-or-more unique discovered writable
  integer leaf controls with declared bounds. It maps canonical `0..=1000`
  positions independently into unequal ranges and executes ordered,
  per-member-confirmed service commands. It has no worker/IPC, persistence,
  adapter declaration, or live hardware path yet.
- Current `position` semantics are an absolute normalized target for every
  member. Relative-balance preservation needs an explicit anchor/baseline rule;
  do not persist or expose this operation until that contract exists.

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
