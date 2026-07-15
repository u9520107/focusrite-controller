# Contributing

## Prerequisites

- Linux/WSL with a C toolchain (`cc`) for Rust crates that compile native code.
- [rustup](https://rustup.rs/). The committed `rust-toolchain.toml` installs the
  selected compiler, Clippy, rustfmt, and ARM64 target.
- [fnm](https://github.com/Schniz/fnm) or compatible `.nvmrc` manager.
- Corepack, bundled with Node, to activate pinned pnpm.

## Setup

```sh
rustup show
cd web
fnm use
corepack enable pnpm
pnpm install --frozen-lockfile
```

## Checks

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd web
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

`web/dist/` is static production output. Later `focusrited` will serve it;
Vite is not a production server dependency.

## Current web limitation

The published Fict 0.28.0 runtime and Vite plugin omit their declared `dist/`
files. `pnpm lint` and `pnpm test` run, but Fict-dependent `pnpm typecheck`
and `pnpm build` remain blocked pending an upstream release with those files.

## Deferred tooling

ARM64 linking needs a chosen sysroot or Zig route. Do not install a cross-build
toolchain until its first real link test; `rust-toolchain.toml` already installs
the Rust ARM64 target.
