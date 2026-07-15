# Phase 1: Foundation Execution Plan

## Goal

Create reproducible Rust and web build/test foundations without implementing
hardware control or LAN APIs.

## Decisions

- Rust 1.97.0 via `rustup`, with Clippy, rustfmt, and the ARM64 Rust target.
- Node 24.18.0 via `web/.nvmrc`; pnpm 11.13.0 via Corepack.
- Fict with Vite; Vite produces static assets for later serving by
  `focusrited`, not a production web server.
- Biome provides web formatting/linting. Vitest provides web tests.
- TypeScript 6.0.3 is pinned; TypeScript 7 is deferred until stable ecosystem
  support is confirmed.
- Project code is MIT-licensed; third-party license inventory and notices are a
  Phase 7 packaging requirement.

## Completed

- [x] Rust workspace and `focusrited` foundation crate.
- [x] Pinned Rust, Node, pnpm, and dependency lockfiles.
- [x] Biome and standalone Vitest smoke-test baseline.
- [x] Contributor setup and verification instructions.
- [x] Root MIT license, Rust/web SPDX metadata, and dependency-license policy.
- [x] Rust format and Clippy checks.
- [x] Web lint, install-lock, and Vitest checks.

## Blockers

### Fict published package contents

`fict@0.28.0` and `@fictjs/vite-plugin@0.28.0` declare runtime, Vite-plugin,
and TypeScript entry points under `dist/`, but their published packages omit
those files. As a result, Fict-dependent `pnpm typecheck` and `pnpm build`
cannot run. The issue has been reported to the upstream maintainer; retain the
pinned Fict dependencies and retry after a fixed release.

### Local C toolchain

`cargo test --workspace` cannot link because this development machine lacks
`cc`. Install the platform C build toolchain (for Debian/Ubuntu,
`build-essential`) before retrying Rust tests.

## Deferred

- ARM64 linking: choose and validate a sysroot or Zig route at the first real
  cross-link test; the Rust target is already installed.
- CI workflow: add only after local Rust tests and Fict production build pass.
- API schema tooling: Phase 5 decision, after capability/state discovery.

## Exit checks

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [x] `pnpm lint`
- [ ] `pnpm typecheck`
- [x] `pnpm test`
- [ ] `pnpm build`
- [ ] Native and ARM64 artifacts build reliably in WSL.

## Update rule

Record each decision, blocker, resolution, and completed exit check here while
Phase 1 is active. At phase completion, retain this file as its implementation
record and update the roadmap status.
