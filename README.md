# Focusrite Controller

Local touchscreen and LAN controller for Focusrite Scarlett hardware mixers.

Phase 1 foundation is in progress. Hardware-control implementation waits for
Phase 2 discovery.

## Development setup

Phase 1 contains only build/test foundations. Install Rust through `rustup`,
use `fnm use` in `web/`, enable Corepack pnpm, then run checks documented in
[CONTRIBUTING.md](CONTRIBUTING.md). The committed toolchain, Node, pnpm, and
dependency lockfiles reproduce the setup on another machine.

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

## Planned support

| Device family | Intended role | Linux path |
| --- | --- | --- |
| Scarlett Solo 4th Gen | early test hardware | upstream `scarlett2` ALSA controls |
| Scarlett 16i16 4th Gen | primary target | FCP kernel driver + `fcp-server` |
| Scarlett 18i16/18i20 4th Gen | later validation target | FCP kernel driver + `fcp-server` |

Exact settings are discovered at runtime. Solo is useful for control-flow
testing, but does not represent 16i16 routing or monitor-group coverage.

## Status

Architecture and delivery plan are documented in [docs/](docs/). Solo/mock
discovery precedes hardware-control implementation; 16i16-specific claims wait
for real FCP captures and target-Pi validation.

## Non-affiliation

Focusrite and Scarlett are trademarks of Focusrite Audio Engineering Limited.
This project is independent and not endorsed by Focusrite.
