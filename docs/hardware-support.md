# Hardware Support and Discovery

## Linux support facts

Focusrite devices are class-compliant for audio, but mixer/routing controls need
Linux driver support. Scarlett2 support includes Scarlett Solo 4th Gen from
Linux 6.8. Big 4th Gen Scarlett devices (16i16, 18i16, 18i20) use FCP support,
included in Linux 6.14+, plus `fcp-server` userspace support and firmware.

Source: [Linux ALSA Focusrite Control/Mixer Drivers](https://github.com/geoffreybennett/linux-fcp).

## Upstream control-panel reference review

[`alsa-scarlett-gui`](https://github.com/geoffreybennett/alsa-scarlett-gui)
is a broad Linux Focusrite Control/Control 2 replacement. Its current source
and documentation are useful reference for supported control families,
relationships, FCP lifecycle expectations, and known device behavior. It is
GPL-3.0-or-later; this MIT project must not copy, link, or derive product code
from it. See `AGENTS.md` for the required ignored-checkout refresh rule.

Reference findings are hypotheses, not runtime authority. Every supported
capability still needs bounded target discovery proving its current IDs,
bounds, access, availability, and unambiguous relationship shape before this
project exposes it or permits a write.

### Scarlett Solo 4th Gen

Upstream documents Solo through the kernel `scarlett2` ALSA driver and confirms
that enabling Direct Monitor changes internal Mix A/B output routing; customised
mixer values persist for the next enable. The physical input gain and output/
headphone level knobs remain separate controls.

This project's current personal-helper scope does not need Direct Monitor
source-level or A/B-balance control. Keep the raw Mix/Monitor Mix cells hidden
and non-writable. Bounded read-only discovery and Direct Monitor state
reconciliation remain useful; no Focusrite-Control-style mixer UI is planned.

### Scarlett 16i16 4th Gen

Upstream documents the 16i16 as an FCP device: Linux 6.14+ FCP support,
compatible firmware, and `fcp-server` are prerequisites before its ALSA
control surface is useful. Its control panel uses the FCP server's ready ALSA
state, including a usable locked `Firmware Version` control, as a readiness
signal. Treat that as an implementation hypothesis to verify on target Pi
hardware, not a fixed cross-device contract.

The project has no `FcpAlsa` implementation yet. Phase 7 begins with
read-only platform and lifecycle evidence: FCP installation, `fcp-server`
startup/restart, ALSA control appearance/removal, USB reconnect, and sanitized
capability fixture. Only then choose one proven personal output workflow for a
narrow adapter-declared capability. Do not import the upstream routing matrix,
monitor-group editor, meters, DSP controls, firmware updates, reset actions,
or feature-parity UI.

## 16i16 platform acceptance gate

Before claiming or packaging 16i16 support, prove on target Pi OS ARM64 that
chosen image provides:

- 64-bit ARM userspace;
- Linux kernel 6.14 or later, or a maintained matching FCP backport;
- installable FCP firmware and `fcp-server`;
- stable USB power and touchscreen-compatible session.

When 16i16 access is available, run bounded read-only discovery on the target
Pi where possible. Any initial validation on a separate native Linux host must
be repeated on the Pi.
Record sanitized proof that clean install, reboot, and unplug/replug return
`fcp-server` and required ALSA controls to ready state. This is a Phase 7
acceptance gate, not a blocker for Phase 1 foundation or Phase 2 Solo/mock work.

Cross-compiling daemon/UI does not package or replace kernel-module and FCP
dependencies. These install on target Pi as deployment prerequisites.

## Discovery procedure

Perform only bounded read-only probes first. Save sanitized output by
device/firmware in `tests/fixtures/` after implementation begins. Do not run
filesystem-writing or unbounded commands during this stage.

```text
uname -r
timeout 10s lsusb -v | sed -n '1,400p'
cat /proc/asound/cards
arecord -l
aplay -l
amixer -c <card> controls
amixer -c <card> contents | sed -n '1,400p'
dmesg --since '10 minutes ago' | tail -n 200
```

For FCP devices also capture bounded `fcp-server` status, for example
`systemctl --no-pager --full status fcp-server | sed -n '1,120p'`, and verify
available FCP tooling.
`alsactl store` is excluded: it writes state. Bound `lsusb -v`, `amixer
contents`, and journal output with relevant device/card filters and line caps.
Redact serial numbers, LAN addresses/tokens, usernames, and unrelated system
data before saving captures; retain only controls, versions, and lifecycle
evidence needed for fixture.

## Adaptive structural capability mapping

The daemon ships adapter rules, not firmware- or ALSA-version-specific builds.
At runtime an adapter identifies a logical capability from a reviewed,
device-local control shape: control type/count/access, required companions,
and the complete relationship graph. It uses discovered control IDs, bounds,
and steps at runtime rather than fixed numids or numeric ranges.

Routine drift such as reordered controls, shifted numids, or changed level
bounds remains supported when exactly one mapping satisfies the adapter rule.
Capability fingerprints are recorded for diagnostics and fixture comparison;
they are not compatibility gates by themselves. A logical writable capability
is withheld only when required controls are missing, relationships contradict
the rule, or more than one candidate mapping fits. The daemon must not guess
between ambiguous candidates.

New or changed shapes begin as bounded read-only discovery. Add a sanitized
fixture and reviewed adapter rule before exposing a writable logical capability.
Raw matrix cells remain adapter-private; an adapter may expose a documented
compound operation only after its complete mapping is unambiguous.

## 16i16 routing questions to verify

Phase 7 must determine from FCP/ALSA capabilities whether the digitally
represented front-panel Output control reports its level and whether an
optical-S/PDIF-routed mix exposes writable master gain. Record both controls,
their ranges, readback/events after physical knob movement, and safe
configuration/restart behavior. Do not assume Focusrite Control 2 behavior is
exposed through Linux until capture proves it.

## Hardware acceptance matrix

| Check | Solo 4th | 16i16 | 18i16/18i20 later |
| --- | --- | --- | --- |
| discovery/capabilities | required | required | required |
| state reads/writes | required | required | required |
| disconnect/reconnect | required | required | required |
| profiles | required | required | required |
| routing/monitor groups | limited | required | required |
| two-client conflict | required | required | required |
| FCP/service restart recovery | n/a | required | required |
| front-panel/external ALSA reconciliation | required | required | required |

No code should treat Solo success as proof that 16i16 routing support works.
