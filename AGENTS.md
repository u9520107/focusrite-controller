# Focusrite Controller Agent Guide

## Safety and privacy

- Never perform live hardware writes, firmware updates, factory resets, or
  clock/routing changes without explicit user approval.
- Preserve device state by default. Applying a stored profile must be explicit.
- Redact USB captures, service logs, device serials, LAN tokens, and home IP
  addresses before committing fixtures or documentation.

## Licensing

- Keep package license metadata aligned with the root [LICENSE](LICENSE).
- Review every new dependency's license; preserve required third-party notices.
- Follow [licensing policy](docs/licensing.md) before distributing artifacts or
  patching upstream dependencies.

## Architecture constraints

- `focusrited` is sole product policy and API authority. Clients never open or
  control USB/ALSA.
- On FCP devices, `fcp-server` owns its device protocol; `focusrited` consumes
  its ALSA controls and fails closed when FCP readiness is unavailable.
- Keep device support capability-discovered. Do not hardcode device-specific
  control names into shared models or UI.

## Engineering workflow

- Start every planned phase with a detailed execution plan in `docs/phases/`.
- Divide implementation into small, independently reviewable merge requests;
  each MR states scope, verification, and any hardware action needed.
- Never edit, commit, or push `main` directly. Work on a dedicated branch and
  merge through a reviewed MR. Do not push any branch unless explicitly asked.
- Run mock/unit tests before hardware tests.
- Hardware validation runs on target Linux hardware; QEMU cannot validate USB
  control behavior.
- Prefer small, standard-library solutions. Do not add abstractions before a
  second real caller needs them.
- Keep relevant documentation current when scope or decisions change.

## Verification

- Rust: format check, Clippy with warnings denied, tests.
- Web: lint, strict typecheck, tests, production build.
- API: mock-device integration coverage for ordering, reconnect, and concurrent
  client updates.

## Sources of truth

- [Roadmap](docs/roadmap.md): current phase and delivery status.
- [Architecture](docs/architecture.md): ownership and runtime design.
- [Protocol](docs/protocol.md): API and state semantics.
- [Network security](docs/network-security.md): LAN and browser boundary.
- [Hardware support](docs/hardware-support.md): discovery, fixtures, and target
  validation.
