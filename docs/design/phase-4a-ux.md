# Phase 4a UX Design Brief

## Status

UX review is accepted. Preference API and hardware-control behavior remain
unimplemented. A read-only native fullscreen spike was verified on the target
Pi display on 2026-07-17; see the Phase 4a plan for sanitized runtime evidence.

## Product decision

The local touchscreen is a useful controller without SSH. A user selects which
discovered controls or daemon-declared linked groups appear in one dashboard,
their labels, and their order. Those choices are daemon-owned per-device
preferences and take effect immediately; a service reload is never required.

## Safety boundary

"Hidden" means hidden from this UI only. It never disables a hardware control,
changes its current value, removes a capability, or prevents another client
from using it. Device availability and writability remain daemon-discovered
facts. A hidden control remains available in Settings for restoration.

Dashboard order/labels are organization only. They never create a compound
command or change fader, mute, routing, gain, or phantom-power behavior.

## Initial information architecture

```text
Main
  status: connected / offline / reconnecting
  configurable dashboard controls
  Settings entry

Settings
  Control visibility
    each discovered control: show / hide
  Dashboard order and label
  Linked pair selection, where daemon capability permits it
  Reset display organization
```

There are no category-filter buttons in initial UI. The dashboard is enough for
the expected four displayed controls. It supports up to twelve compact strips;
more than eight use normal touchscreen scrolling rather than narrower targets.
Settings shows `n of 12` visible controls. The daemon rejects a thirteenth
dashboard item; when full, touchscreen disables further Show actions with an
explanation rather than silently hiding another item.

## Intended 16i16 dashboard

Expected user configuration is deliberately small:

1. `Gaming laptop`: linked input tracks 1/2.
2. `Daily driver`: linked input tracks 3/4.
3. `KEF level`: optical-output fader; practical listening level.
4. Optional hardware main monitor level, if user chooses to show it.

USB input, unused source/output tracks, and headphone-output digital levels
remain hidden unless user explicitly enables them. Headphone hardware knobs
remain the normal control path.

"Main output" is not a universal listening path. On a 16i16, a user may listen
through an optical-output mix fader while hardware main monitor output is
irrelevant. Dashboard therefore controls selected paths, not guessed global
main output.

- Settings offers eligible available writable controls and linked groups.
  Adapter-provided presentation labels identify choices; client never infers
  routing from control IDs.
- User may rename a dashboard item, for example `KEF level`.
- Custom labels arrive through configuration import first, then Phase 5 web
  text input. Touchscreen displays labels but does not edit text.
- Dashboard selection is display preference only. It never changes routing,
  hardware monitor assignment, or output values.
- Per-control dashboard configuration later selects one cut action: `hardware`
  only when a compatible discovered mute exists; `panic_cut` writes the level
  minimum; or `none`. Hardware mute preserves level. Panic cut is visibly a
  level change, is labelled `Cut` rather than `Mute`, and never claims to be a
  hardware mute. A later Restore action may use a persisted last confirmed
  level; it is not part of Phase 4a.
- A linked pair is not merely a UI label. One `Gaming laptop` slider needs a
  validated daemon compound command over compatible controls, preserving
  declared pair behavior and reporting partial failures. Until that service
  feature exists, UI must render separate 1 and 2 controls rather than pretend
  one slider controls both.
- A future bidirectional synchronized pair may bind hidden hardware main
  monitor level to visible `KEF level` optical fader. Physical knob and either
  UI control then converge through declared mapping; dashboard need not expose
  duplicate raw main monitor strip.

## Main-screen behavior

- Render only available dashboard items that are not hidden.
- Keep an empty dashboard useful: show a Settings action, not guessed disabled
  controls.
- Each displayed level control uses one compact strip: label, slider, and
  compatible mute. Do not add generic global-output level or mute controls in
  initial UI; users may explicitly show them later if hardware exposes them.
- This is a mixer-channel pattern, not a fixed RODE-style visual copy. For the
  4.3-inch landscape display, channels use horizontal sliders and compact grid
  rows. A future dense mixer layout may use vertical faders where screen size
  and dashboard use make that clearer; protocol and daemon behavior stay the
  same.
- Default grid is two columns. A dense dashboard may use three columns only when
  card width preserves slider and mute touch targets; otherwise use scrolling
  never shrink touch targets below usability.
- A compact grid card has tappable label above slider and compatible mute. It
  has no separate card header. Two columns by four rows target eight visible
  controls on a 4.3-inch landscape display.
- Initial UI omits numeric level display: raw adapter values and invented `%`
  are not meaningful mixer units. Add formatted units only after hardware and
  user testing proves they help.
- Slider rail shows ten visual divisions with a stronger midpoint marker for
  coarse touch positioning. Marks do not quantize commands; use real adapter-
  declared step size when a control is genuinely stepped.
- Label gets protected full-row width for recognizability. Slider uses bounded
  compact width; it does not consume label width.
- In compact touchscreen grid, tapping non-slider/non-mute card area opens
  Focus. Label remains its visible affordance but need not be sole touch target.
- Tapping a level-strip label opens one large local Focus panel with same
  confirmed value, slider, and compatible mute. It avoids a separate action
  whose position changes with label length. Focus changes presentation only;
  commands, snapshot resync, and validation remain identical.
- Use compact rectangular grid cards with modest corner radii. Preserve large
  touch targets, but do not spend screen area on decorative spacing.
- Value display changes only after snapshot, event, or command result confirms
  canonical state. During command delivery, show a short pending affordance;
  command error preserves prior confirmed state and shows short non-reflowing
  toast text.
- Offline/reconnecting state removes actionable controls and retains status.
- Revision gap or changed instance ID clears cached controls, then requests a
  snapshot before showing controls again.

## Settings behavior

- Settings lists controls using daemon-provided presentation labels. It never
  derives labels or categories from opaque control IDs.
- A visibility toggle writes only a preference. Changes apply when leaving
  Settings, or immediately if main screen remains live underneath.
- Dashboard ordering/label updates write only preference. They cannot make an
  unavailable or read-only control actionable.
- Reset removes only display preferences for current device and restores
  adapter-declared default presentation. It does not reset hardware or profiles.
- Settings state must remain reachable even when all controls are hidden.

## Planned preference model

Persist alongside daemon profile storage, keyed by stable device identity and
control ID:

```text
control_preferences[control_id] = {
  hidden: bool,
  label: optional string,
  dashboard_order: optional integer
}

dashboard_items = [control_id | linked_group_id]
```

Missing preference uses adapter-declared presentation. Stale preferences for
unavailable controls are retained safely but omitted from main UI; they may be
pruned only by an explicit, tested maintenance rule.

This is separate from profiles and device state. Profile save/apply never
reads, writes, or applies display preferences.

## Future virtual control groups

Virtual groups are a later daemon capability, not a touchscreen-only feature.
They let a user define a named logical track over compatible discovered
controls, for example `Gaming laptop` over input tracks 1/2 or `KEF` over
selected optical outputs.

- Membership is constrained by operation compatibility. A group level operation
  may include compatible input/output level controls; mute operation may include
  compatible mutes. It must not silently group unrelated domains such as level
  and phantom power. Dangerous controls remain individually confirmed unless
  later policy explicitly allows safe grouped operation.
- A group appears as an additional virtual dashboard track. It does not replace
  member controls: user may show both group and individuals, or hide either.
- Group level command validates all members and applies daemon-declared order.
  For non-native groups it preserves member relative balance where applicable,
  confirms each member, and reports per-member partial failure. It cannot claim
  hardware atomicity.
- Where hardware exposes native linking/monitor groups, adapter uses that
  capability rather than simulating it.
- A future synchronized level set is distinct from a group command: a confirmed
  change to any member converges all members through declared mapping. It is
  capability-limited, explicit, and reports degraded state on target failure.
- Control groups and synchronized sets list only discovered controls; they do
  not contain other groups/sets. Any future nested dashboard collection is
  visual organization only.
- Group creation/editing is deferred from touchscreen because it requires names,
  membership selection, validation explanation, and recovery from invalid
  membership. First editor is Phase 5 web UI; configuration import/export is
  equal first-class path.

## Canonical level mapping

Raw level values are adapter-specific: for example, Solo mixer controls use
`0..184` while other tracks may have different bounds or dB ranges. Profiles
keep confirmed raw values. Virtual level groups, mirror bindings, and
synchronized sets instead map through one canonical `0.0..1.0` position. When
both members expose dB ranges, normalize/map in dB; otherwise use each declared
integer minimum/maximum. This preserves endpoints across unequal ranges without
pretending raw values or invented percentages are comparable.

## Configuration import/export requirements

Display preferences and virtual-group definitions are daemon-owned per-device
configuration, separate from hardware profiles. They persist atomically in a
documented versioned file format and never write hardware during import/export.

- Web or touchscreen changes call daemon API; daemon validates and atomically
  saves configuration. UI does not write files directly.
- Provide CLI export and validated import commands so SSH/config-management
  workflows can reproduce a dashboard and group setup.
- Import rejects unknown schema versions, incompatible device binding, duplicate
  group names, invalid member IDs, overlapping synchronization sets, or
  incompatible operations without changing active configuration.
- Manual offline editing is supported only while daemon is stopped, followed by
  normal startup validation. Do not require service stop for ordinary changes;
  avoid a live file watcher until a real need proves it.
- Export redacts device serials/tokens and includes only portable preference and
  group metadata. Hardware profile values remain separately exported by explicit
  profile workflow.

## Responsive mockup states

Create HTML/CSS mockups before implementation for both 4.3-inch touchscreen
and web layout, using same visual vocabulary:

1. Dashboard, online, selected optical listening path (`KEF level`) plus compatible
   mute when capability provides one.
2. Dashboard, online, no visible controls.
3. Dashboard, muted.
4. Offline and reconnecting.
5. Settings: hide/show a control.
6. Settings: reorder a displayed control.
7. Command failure after a confirmed prior value.

## Native review harness

The native client has an explicit development-only demo mode for visual and
touch review. `FOCUSRITE_UI_DEMO=1` supplies synthetic capability data;
`FOCUSRITE_UI_REVIEW=1` additionally exposes review controls. They can
deterministically open/close Focus, select connection/error/Cut states, and
show calibration targets. It is never enabled in ordinary kiosk mode, never
connects to `focusrited`, and cannot send device commands. Each accepted native
state is screenshot-captured after layout/input changes; short local compositor
video is reserved for validating transition timing once static layout is
stable. Captured review artifacts must not commit device identities, raw
levels, or other sensitive runtime data.

`FOCUSRITE_UI_DEBUG=1` writes a temporary local
`/tmp/focusrite-ui-debug.log`. It records egui raw touch/pointer events and
the selected strip's hitbox and mapped pointer position. This locates errors
between Wayland and the UI. When no app event arrives, use a separate read-only
`libinput debug-events` capture to locate an earlier kernel/compositor mapping
failure; do not commit either log.

Touchscreen mockup optimizes fixed small-screen touch targets. Web mockup uses
same state and labels with responsive navigation; pairing/authentication and
LAN behavior remain Phase 5 scope.

## Design acceptance before implementation

- Review accepts navigation, empty/offline/error states, wording, touch target
  sizes, and preference semantics.
- 4.3-inch target resolution/orientation is recorded after DSI kiosk display
  is deliberately enabled for a read-only display check.
- MR 2a capability presentation proposal expands only as required by accepted
  mockup.
- Preference persistence/API is its own later slice after the display contract;
  it is not silently included in capability metadata work.
