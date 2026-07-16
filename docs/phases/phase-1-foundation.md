# Phase 1: Foundation Execution Plan

## Goal

Create reproducible Rust build/test foundations without implementing hardware
control or LAN APIs.

## Decisions

- Rust 1.97.0 via `rustup`, with Clippy, rustfmt, and the ARM64 Rust target.
- Project code is MIT-licensed; third-party license inventory and notices are a
  Phase 7 packaging requirement.

## Completed

- [x] Rust workspace and `focusrited` foundation crate.
- [x] Pinned Rust toolchain and dependency lockfile.
- [x] Rust-only GitHub Actions checks.
- [x] Contributor setup and verification instructions.
- [x] Root MIT license, Rust SPDX metadata, and dependency-license policy.
- [x] Rust format and Clippy checks.

## Blockers

### Local C toolchain

`cargo test --workspace` cannot link because this development machine lacks
`cc`. Install the platform C build toolchain (for Debian/Ubuntu,
`build-essential`) before retrying Rust tests.

## Deferred

- ARM64 linking: choose and validate a sysroot or Zig route at the first real
  cross-link test; the Rust target is already installed.
- Web setup: defer Fict, Vite, Node, pnpm, Biome, and Vitest to Phase 5. Fict
  0.28.0 published packages omit declared `dist/` files; retry after an
  upstream fixed release or accepted upstream patch.
- API schema tooling: Phase 5 decision, after capability/state discovery.

## Exit checks

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Native and ARM64 artifacts build reliably in WSL.

## Update rule

Record each decision, blocker, resolution, and completed exit check here while
Phase 1 is active. At phase completion, retain this file as its implementation
record and update the roadmap status.
