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

Phase 2’s direct ALSA adapter also needs development headers and `pkg-config`.
On Debian/Ubuntu/WSL:

```sh
sudo apt-get install -y pkg-config libasound2-dev
```

On Fedora, install `pkgconf-pkg-config` and `alsa-lib-devel`.

Phase 2 read-only hardware discovery also needs command-line tools. On
Debian/Ubuntu/WSL:

```sh
sudo apt-get install -y usbutils alsa-utils
```

`usbutils` provides `lsusb`; `alsa-utils` provides `amixer`. `libasound2-dev`
is for compiling the Rust ALSA binding, not for the `amixer` command. On Fedora,
install `usbutils` and `alsa-utils`.

## Setup

```sh
rustup show
```

## Route a Scarlett Solo into WSL2

Do this only when ready for Phase 2 read-only discovery. While attached, the
Solo is unavailable to Windows applications. This grants WSL direct USB access;
it does not write device state.

1. In an elevated PowerShell window, install/update WSL and `usbipd-win`:

   ```powershell
   wsl --update
   winget install usbipd
   ```

2. Plug in the Solo, then list USB devices from elevated PowerShell. Identify
   its `BUSID` by its Focusrite description; do not copy serial numbers into
   issues, fixtures, or commits.

   ```powershell
   usbipd list
   usbipd bind --busid <BUSID>
   ```

   `bind` shares that USB port persistently. It needs elevation; attaching does
   not. If the device is already shared, skip `bind`.

3. Start the intended WSL2 distro, then attach from a normal PowerShell window:

   ```powershell
   usbipd attach --wsl --busid <BUSID>
   ```

4. In WSL, verify Linux sees the Solo, then run only bounded read-only probes
   from [hardware support](docs/hardware-support.md#discovery-procedure):

   ```sh
   lsusb
   cat /proc/asound/cards
   amixer -c <card> controls
   ```

   If `amixer -c <card> controls` fails but its `sudo` form works, grant the
   current WSL user read/write access to ALSA controls, then open a new shell:

   ```sh
   sudo usermod -aG audio "$USER"
   exit
   ```

   Reopen the WSL distro and confirm `id` lists `audio`. This only changes
   local device-node permission; it does not change Solo state. Do not use
   `sudo` for later discovery once normal access works.

5. When finished, return it to Windows:

   ```powershell
   usbipd detach --busid <BUSID>
   ```

Attach is not persistent: repeat it after WSL restart or USB unplug/replug.
Do not run `alsactl store`, control writes, routing/clock changes, firmware
updates, or resets without explicit approval. See Microsoft’s current
[WSL USB guide](https://learn.microsoft.com/windows/wsl/connect-usb) and
[usbipd-win instructions](https://github.com/dorssel/usbipd-win).

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
