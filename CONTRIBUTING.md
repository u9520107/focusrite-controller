# Contributing

## Prerequisites

- Native Linux with a C toolchain (`cc`) for Rust crates that compile native
  code. Raspberry Pi OS local sessions are the primary development target.
  SSH and Zed Remote Development are optional; when used, builds and hardware
  commands still run on the Pi.
- [rustup](https://rustup.rs/). The committed `rust-toolchain.toml` installs the
  selected compiler, Clippy, and rustfmt.

On Raspberry Pi OS, Debian, or Ubuntu, install the C toolchain with:

```sh
sudo apt-get update
sudo apt-get install -y build-essential
```

On Fedora, install `gcc` and `make`.

Phase 2’s direct ALSA adapter also needs development headers and `pkg-config`.
On Raspberry Pi OS, Debian, or Ubuntu:

```sh
sudo apt-get install -y pkg-config libasound2-dev
```

On Fedora, install `pkgconf-pkg-config` and `alsa-lib-devel`.

Hardware discovery also needs command-line tools. On Raspberry Pi OS, Debian,
or Ubuntu:

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

## Develop directly on the Pi

Work in the repository from a local Pi session. Editor, Cargo, and hardware
commands run natively on the Pi, validating target architecture and ALSA.
SSH/Zed Remote Development may access that same checkout when convenient, but
is not required.

Before connecting hardware, run the checks in [Checks](#checks). If ALSA access
is needed, add the current local user to the `audio` group, then start a new
login session:

```sh
sudo usermod -aG audio <user>
```

Before hardware probes, confirm the new session has both ALSA device nodes and
effective group membership:

```sh
test -d /dev/snd
id -nG
```

Do not use `sudo` to mask a missing `/dev/snd` directory or inactive `audio`
membership; fix local device-node/session access first.

Do not run control writes, routing/clock changes, firmware updates, resets, or
profile application without explicit approval.

## Optional: route a Scarlett Solo into WSL2

This was Phase 2 development infrastructure. Use it only when Pi-native access
is unavailable. While attached, the Solo is unavailable to Windows applications.
It grants WSL direct USB access; it does not write device state.

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

Cross-build tooling remains deferred. Native Pi development is the Phase 3
baseline; do not add a cross-build path until it solves a demonstrated need.
