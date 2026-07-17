# Deployment and Build Plan

## Supported targets

- Primary appliance: Raspberry Pi 5, 64-bit Linux,
  `aarch64-unknown-linux-gnu`.
- Primary development target: local Raspberry Pi 5 session.
- SSH and Zed Remote Development are optional remote access paths, not required
  development workflow.
- Laptop-local Linux or WSL are optional development environments; neither is a
  deployment target.

Pi-native development and validation come first. Choose a cross-build path only
if native Pi development demonstrates a real need.

## Build policy

Implementation will pin Rust, Node, pnpm, and Fict versions. Native Pi builds
are supported for development. Any Rust cross-build uses a pinned
container/sysroot or `cargo-zigbuild`; choice is deferred to Pi validation. Web
assets build on development/CI host; packaged Pi deployments require no Node,
Rust compiler, or web build tools.

## Packaging

Target package is a Debian `.deb` because Raspberry Pi OS is Debian-based. It
will install daemon, touchscreen client, static web assets, systemd units, udev
rules, data directories, and documented FCP prerequisites. A tarball may follow
for desktop Linux, but is not v1 release goal.

## Service model

- `focusrited` starts through systemd.
- Local Unix socket has dedicated group permissions for touchscreen client.
- Device access uses narrow udev/group permissions.
- Persistent preferences/profiles live in a service-owned data directory.
- Native UI starts through selected kiosk/session mechanism once display hardware
  and compositor are known.

## Network security

Foundation and native touchscreen operation use only Unix-socket IPC;
`focusrited` has no TCP listener. LAN access is a later, opt-in design decision.
See [Network Security](network-security.md).

## Validation limits

CI can lint, test mock adapters, and later verify packages. Only deployed Linux
hardware can prove USB, ALSA/FCP, touchscreen, reboot, and unplug behavior.
QEMU is not hardware confidence.
