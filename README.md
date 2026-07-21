# Focusrite Controller

Local touchscreen and LAN controller for Focusrite Scarlett hardware mixers.

For delivery status and phase execution plans, see the
[roadmap](docs/roadmap.md).

## Development setup

Install Rust through `rustup`, then run checks documented in
[CONTRIBUTING.md](CONTRIBUTING.md). The committed Rust toolchain and lockfile
reproduce the setup on another machine.

## License

Licensed under [MIT](LICENSE). See [licensing policy](docs/licensing.md) for
third-party dependency and distribution requirements.

## Goal

A Raspberry Pi 5 connects to a Focusrite interface by USB and runs:

- `focusrited`: Rust daemon, sole product policy/API authority and web API.
- `focusrite-ui`: Rust fullscreen touchscreen client.
- Fict TypeScript web UI for phones and other LAN devices.

Audio stays on external devices connected to Focusrite inputs. This project
controls hardware mixer/routing settings; it does not carry, process, record,
or play host audio in v1.

This is a personal convenience controller for a small set of declared
multi-device and multi-output workflows, not a Linux replacement for Focusrite
Control 2. Unsupported mixer, routing, input-gain, balance, and advanced
device features stay hidden rather than being pursued for feature parity.

## External reference

`externals/alsa-scarlett-gui` is an ignored checkout of the upstream Linux
control-panel project. Use it as design and device-behavior reference, then
prove the attached device's capability shape through this project's bounded
runtime discovery. Never copy, link, or derive product code from it: the
reference project is GPL-3.0-or-later while this project is MIT. Refresh the
checkout with `git -C externals/alsa-scarlett-gui pull --ff-only` before new
device-support research.

## Planned support

| Device family | Intended role | Linux path |
| --- | --- | --- |
| Scarlett Solo 4th Gen | early test hardware | upstream `scarlett2` ALSA controls |
| Scarlett 16i16 4th Gen | primary target | FCP kernel driver + `fcp-server` |
| Scarlett 18i16/18i20 4th Gen | later validation target | FCP kernel driver + `fcp-server` |

Exact settings are discovered at runtime. Solo is useful for control-flow
testing, but does not represent 16i16 routing or monitor-group coverage.

## Non-affiliation

Focusrite and Scarlett are trademarks of Focusrite Audio Engineering Limited.
This project is independent and not endorsed by Focusrite.
