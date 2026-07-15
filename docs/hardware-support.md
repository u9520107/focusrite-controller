# Hardware Support and Discovery

## Linux support facts

Focusrite devices are class-compliant for audio, but mixer/routing controls need
Linux driver support. Scarlett2 support includes Scarlett Solo 4th Gen from
Linux 6.8. Big 4th Gen Scarlett devices (16i16, 18i16, 18i20) use FCP support,
included in Linux 6.14+, plus `fcp-server` userspace support and firmware.

Source: [Linux ALSA Focusrite Control/Mixer Drivers](https://github.com/geoffreybennett/linux-fcp).

## 16i16 platform acceptance gate

Before claiming or packaging 16i16 support, prove on target Pi OS ARM64 that
chosen image provides:

- 64-bit ARM userspace;
- Linux kernel 6.14 or later, or a maintained matching FCP backport;
- installable FCP firmware and `fcp-server`;
- stable USB power and touchscreen-compatible session.

When 16i16 access is available, first run bounded read-only discovery on a
directly connected Linux development laptop, then repeat target-Pi validation.
Record sanitized proof that clean install, reboot, and unplug/replug return
`fcp-server` and required ALSA controls to ready state. This is a 16i16
acceptance gate, not a blocker for Phase 1 foundation or Solo/mock work.

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

## 16i16 routing questions to verify

Phase 2 must determine from FCP/ALSA capabilities whether the digitally
represented front-panel Output control can be assigned beyond analogue monitor
outputs, specifically to an optical-S/PDIF output or its routed mix gain. Record
the available targets, readback/events after physical knob movement, and the
safe configuration/restart behavior. Do not assume Focusrite Control 2 behavior
is exposed through Linux until capture proves it.

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
