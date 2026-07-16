# Contributing

## Prerequisites

- Linux/WSL with a C toolchain (`cc`) for Rust crates that compile native code.
- [rustup](https://rustup.rs/). The committed `rust-toolchain.toml` installs the
  selected compiler, Clippy, and rustfmt.

On Debian/Ubuntu/WSL, install the C toolchain with:

```sh
sudo apt-get update
sudo apt-get install -y build-essential
```

On Fedora, install `gcc` and `make`.

## Setup

```sh
rustup show
```

## Checks

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Web setup is deferred to Phase 5 while upstream Fict packages are repaired.
Later `focusrited` will serve static web output; Vite will not be a production
server dependency.

## Deferred tooling

Pi-native validation and any cross-build toolchain are Phase 3 work. Do not
install one until that phase identifies a real deployment need.
