# Deployment and Build Plan

## Supported targets

- Primary appliance: Raspberry Pi 5, 64-bit Linux,
  `aarch64-unknown-linux-gnu`.
- Later desktop Linux: `x86_64-unknown-linux-gnu`.

WSL is development/build host. Cross-build is default to avoid Pi compilation.
Native Pi build is an allowed fallback for diagnosing toolchain or target-only
issues, not release workflow.

## Build policy

Implementation will pin Rust, Node, pnpm, and Fict versions. Rust cross-build
uses a pinned container/sysroot or `cargo-zigbuild`; choice is deferred until
first executable dependency needs are known. Web assets build on development/CI
host; Pi requires no Node, Rust compiler, or web build tools.

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

Default bind is loopback. LAN bind is explicit configuration to one configured
private interface, never all interfaces by default. LAN mode uses random bearer
tokens stored with owner-only permissions. Plain HTTP exposes theft/replay risk
on observable Wi-Fi: v1 supports only trusted private networks, no port
forwarding, and documented token rotation/revocation. Browser HTTP uses bearer
headers; authenticated HTTP mints short-lived one-use WebSocket tickets. UI is
same-origin, Origin checks are strict, and CORS never uses wildcard.

V1 does not use HTTPS or self-signed certificates. Home-LAN token auth avoids
certificate trust friction. Deploy a reverse proxy with trusted TLS if service
ever reaches untrusted/shared networks or remote access. Never expose service
through router port forwarding.

## Validation limits

CI can lint, test mock adapters, build arm64 artifacts, and verify packages.
Only deployed Linux hardware can prove USB, ALSA/FCP, touchscreen, reboot, and
unplug behavior. QEMU is not hardware confidence.
