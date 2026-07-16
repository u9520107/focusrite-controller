# Phase 1: Foundation Execution Plan

## Goal

Create reproducible Rust build/test foundations in WSL without implementing
hardware control, LAN APIs, or cross-compilation.

## Decisions

- Rust 1.97.0 via `rustup`, with Clippy and rustfmt.
- Project code is MIT-licensed; third-party license inventory and notices are a
  Phase 8 packaging requirement.

## Completed

- [x] Rust workspace and `focusrited` foundation crate.
- [x] Pinned Rust toolchain and dependency lockfile.
- [x] Rust-only GitHub Actions checks.
- [x] First Rust GitHub Actions run.
- [x] Contributor setup and verification instructions.
- [x] Root MIT license, Rust SPDX metadata, and dependency-license policy.
- [x] Rust format and Clippy checks.
- [x] Rust tests after installing the local C toolchain.

## Deferred

- Pi-native validation and any required cross-compilation: Phase 3.
- Web setup: defer Fict, Vite, Node, pnpm, Biome, and Vitest to Phase 5. Fict
  0.28.0 published packages omit declared `dist/` files; retry after an
  upstream fixed release or accepted upstream patch.
- API schema tooling: Phase 5 decision, after capability/state discovery.

## Exit checks

- [x] `cargo fmt --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Update rule

Record each decision, blocker, resolution, and completed exit check here while
Phase 1 is complete. Retain this file as its implementation record. Create each
later phase execution plan when that phase starts.
