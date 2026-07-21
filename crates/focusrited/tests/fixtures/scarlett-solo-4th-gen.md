# Scarlett Solo 4th Gen ALSA control fixture

Captured through USB/IP on WSL2 on 2026-07-15 with bounded read-only
`amixer -c 0 contents`. Card index is runtime-assigned and must not be assumed
by production code. Serial numbers and mutable current values are omitted.

Firmware control reports version `2417`; minimum version reports `2115`.
`USB Internal Validity` and `Sync Status` are read-only status controls.

## Discovered writable controls

| Family | ALSA control pattern | Type | Domain |
| --- | --- | --- | --- |
| PCM capture route | `PCM 0[1-4] Capture Enum` | enum | Off, Analogue 1/2, Mix A-F, DSP 1/2, PCM 1/2 |
| PCM source | `PCM Input Capture Switch` | enum | Direct, Mixer |
| DSP input route | `DSP Input [1-2] Capture Enum` | enum | same route domain |
| Input 1 mode | `Line In 1 Level Capture Enum` | enum | Line, Inst |
| Input 2 Air | `Line In 2 Air Capture Enum` | enum | Off, Presence, Presence + Drive |
| Input 2 phantom | `Line In 2 Phantom Power Capture Switch` | boolean | off/on |
| Analogue output route | `Analogue Output 0[1-2] Playback Enum` | enum | same route domain |
| Direct monitor | `Direct Monitor Playback Switch` | boolean | off/on |
| Mixer input level | `Mix [A-F] Input 0[1-4] Playback Volume` | integer | 0–184, step 1, -80.00 to +12.00 dB |
| Mixer input route | `Mixer Input 0[1-4] Capture Enum` | enum | same route domain |
| Monitor-mix level | `Monitor Mix [A-B] Input 0[1-4] Playback Volume` | integer | 0–184, step 1, -80.00 to +12.00 dB |

`Mix` levels (24 controls) and `Monitor Mix` levels (8 controls) report ALSA
readback events. Production discovery must retain the actual ALSA identifier,
type, range, enum items, access mode, and availability; display labels are not
stable IDs.

## Discovered read-only controls

| ALSA control | Type | Domain |
| --- | --- | --- |
| `Firmware Version` | integer | driver-reported version |
| `Minimum Firmware Version` | integer | driver-reported version |
| `USB Internal Validity` | boolean | validity status |
| `Sync Status` | enum | Unlocked, Locked |
| `Capture Channel Map` | integer array | four fixed channels |
| `Playback Channel Map` | integer array | two fixed channels |
| `Level Meter` | integer array | 12 values, 0–4095 |

No control was changed for this capture. This proves Linux-exposed controls on
this Solo only; it does not establish Pi or FCP-device support.
