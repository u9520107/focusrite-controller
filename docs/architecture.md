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
                                              └─ HTTP/WebSocket: same-origin Fict UI
                                                                  and future LAN clients
```

`focusrited` is sole product policy and API authority: it owns capability
discovery, validation, command ordering, persistence, reconciliation, and APIs.
No UI may open USB or ALSA controls. For FCP hardware, `fcp-server` owns FCP
device protocol and creates ALSA controls; `focusrited` must never claim
exclusive USB ownership. A UI failure must never change or interrupt device
state.

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
arrive. Web may expose more groups.

Preferences control visibility, not device availability:

- `main`: monitor/output volume, mute, source.
- `mixer`: faders, mute/solo, routing where supported.
- `inputs`: gain, phantom, Air, Auto Gain, Clip Safe where supported.
- `advanced`: clock, digital I/O, loopback where supported.
- `profiles`: save/select/apply named states.

Dangerous controls need explicit confirmation. Firmware update and factory reset
are outside v1.

## Persistence

Persist only daemon preferences and user-named profiles. Preserve physical
device state at startup. Applying a profile is explicit; no startup profile
write occurs in v1. Atomic writes occur on profile changes, never every fader
movement. Profile apply is a non-rollback transaction: bind profile to device
identity and capability-schema version; dry-run diff first; require explicit
confirmation for dangerous values; apply deterministic adapter-declared order;
then return per-operation applied/skipped/failed results. Never auto-apply or
auto-rollback.

## Future Pebble client

Keep `pebble/` in monorepo once API is stable. Pebble is a third client, never
a hardware controller. Its connection path and SDK details remain discovery
work; it must authenticate and use same versioned command/state API through a
phone/companion or LAN bridge.
