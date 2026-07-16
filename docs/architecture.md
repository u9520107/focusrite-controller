# Architecture

## Scope

V1 controls one attached Focusrite interface. It preserves hardware state at
startup, serves a local touch UI and authenticated home-LAN web UI, and stores
optional named profiles. It configures hardware mixer/routing settings, but
does not carry, process, record, or play host audio. Live audio meters are
deferred.

## Ownership

```text
Focusrite USB
  ├─ scarlett2 ALSA controls ──────────┐
  └─ FCP kernel driver ─ fcp-server ─ ALSA controls ─┤
                                                    │
                                             focusrited
                                              ├─ policy, validation, command queue
                                              ├─ state reconciliation and persistence
                                              ├─ Unix socket: focusrite-ui
                                              └─ later optional HTTP/WebSocket:
                                                 Fict UI and LAN clients
```

`focusrited` is sole product policy and API authority: it owns capability
discovery, validation, command ordering, persistence, reconciliation, and APIs.
No UI may open USB or ALSA controls. For FCP hardware, `fcp-server` owns FCP
device protocol and creates ALSA controls; `focusrited` must never claim
exclusive USB ownership. A UI failure must never change or interrupt device
state. Foundation, native touchscreen, and macro-pad operation use only the
Unix socket; LAN API design is deferred. See
[Network Security](network-security.md).

## Runtime model

- One serial device worker performs all blocking hardware operations.
- FCP adapter monitors `fcp-server` readiness plus ALSA control availability.
  It is offline until both are ready, rejects writes while either fails, and
  resnapshots capabilities/state after recovery.
- Device adapter reports capabilities and current canonical state.
- Command validation happens before each hardware write.
- ALSA events, front-panel mutations, FCP lifecycle changes, and post-write
  reads reconcile canonical state. External changes create new revisions.
- Daemon broadcasts confirmed state revisions and an `instance_id` to every
  client. Restart changes `instance_id`; clients then fetch a snapshot.
- USB loss or FCP loss broadcasts offline state. Recovery discovers capabilities
  again, then broadcasts a new snapshot.
- Queue has bounded per-client/global limits. Successive fader commands for one
  control coalesce before write; discrete and dangerous commands never coalesce.

## Device abstraction

Adapters normalize Linux-exposed controls, not vendor USB protocols:

- `Scarlett2Alsa`: Solo/other supported small Gen 4 devices.
- `FcpAlsa`: 16i16, 18i16, 18i20 after FCP userspace support is active.
- `MockDevice`: deterministic tests and API/UI development.

Model controls by opaque, stable capability/control IDs; labels are display
metadata only. Each control declares typed value, unit, range/enumeration,
step, writability, availability plus reason, danger level, dependency
expression, and capability-schema version. Compound operations declare one
validation and ordering boundary. Hardware labels and control sets vary; shared
UI must never assume Solo names or channel counts.

## Surfaces

Default touchscreen groups: monitor/output, selected mixer controls, mute, and
device status. It uses large controls and fullscreen layout after screen details
arrive. Touchscreen and web UI must expose designated main volume and mute when
the active device capability provides them; web may expose more groups.

A local USB macro-pad is an optional third local control surface. Its adapter
uses the same Unix-socket commands as the touchscreen; it never opens ALSA or
USB audio controls. Initial intended mapping is capability-dependent:

- large encoder: designated main-output or mix volume, plus mute;
- small encoder 1: level plus mute for first user-defined linked input pair;
- small encoder 2: level plus mute for second user-defined linked input pair.

For example, linked pairs may be labelled as two stereo input devices. “Level”
means mixer/output level, not preamp gain. The macro-pad’s press/button behavior
and USB protocol (HID or MIDI) are discovered when its model is selected. If
the 16i16 exposes optical-S/PDIF mix gain but not native monitor-knob assignment,
the large encoder may target that logical output directly.
Remaining buttons may later map to capability-discovered, user-configured
actions through the same command API; no fixed mapping is assumed yet.

Preferences control visibility, not device availability:

- `main`: monitor/output volume, mute, source.
- `mixer`: faders, mute/solo, routing where supported.
- `inputs`: gain, phantom, Air, Auto Gain, Clip Safe where supported.
- `advanced`: clock, digital I/O, loopback where supported.
- `profiles`: save/select/apply named states.

Controls that are physical-only or otherwise not writable through the daemon are
hidden by default. If a user tries to enable their display, the UI must warn
that the control cannot be changed remotely and must never present it as an
actionable slider/button.

Dangerous controls need explicit confirmation. Firmware update and factory reset
are outside v1.

## User metadata and linked groups

User metadata is separate from device state and profiles. The daemon persists
per-device custom display labels keyed by stable capability/control ID. For
example, two physical inputs may be labelled `Gaming laptop L` and `Gaming
laptop R`. Labels survive daemon or Pi restart, never write hardware, and are
unavailable if their referenced control is unavailable.

Users may define a named linked group over compatible controls, such as a
`Gaming laptop` stereo pair. If hardware exposes a native stereo/link control,
the adapter uses it. Otherwise a group command is a validated, ordered compound
operation: fader changes preserve each member's relative balance; mute and solo
may apply to all members. Routing or source changes group only where
capabilities declare compatible operations. Gain, phantom power, clock, and
other dangerous controls are never grouped by default and remain individually
confirmed. Hardware cannot guarantee atomicity for a non-native group write.
External ALSA/front-panel mutations still reconcile each member's canonical
state and create a new revision.

Where discovery proves a readable front-panel monitor level and writable
optical-mix master gain, users may enable a one-way mirror binding: a confirmed
front-panel level change writes the mapped optical gain. It is off by default,
available only for a dedicated optical mix, and reports partial-write failure.
This is not a native monitor group and must not feed optical changes back into
the front-panel control.

## Persistence

Persist only daemon preferences, per-device user metadata (labels and linked
groups), and user-named profiles. Preserve physical device state at startup.
Applying a profile is explicit; no startup profile write occurs in v1. Atomic
writes occur on profile or metadata changes, never every fader movement.
Profile apply is a non-rollback transaction: bind profile to device identity
and capability-schema version; dry-run diff first; require explicit
confirmation for dangerous values; apply deterministic adapter-declared order;
then return per-operation applied/skipped/failed results. Never auto-apply or
auto-rollback.

## Future Pebble client

Keep `pebble/` in monorepo once API is stable. Pebble is a third client, never
a hardware controller. Its connection path and SDK details remain discovery
work; it must authenticate and use same versioned command/state API through a
phone/companion or LAN bridge.
