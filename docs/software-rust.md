# Software: Rust daemon — best-practice research

Collected 2026-11-08. Re-check crate versions before pinning. The component split is in [`architecture.md`](architecture.md); how to run it without hardware is in [`testbench.md`](testbench.md).

## Verdict

Rust is a good fit: one static binary, low RAM on the Zero 2 W (512 MB), no interpreter to break on OS upgrades, and the type system makes the untrusted-card-input path easy to get right. Two things differ from a typical Python Pi project:

1. **No rppal.** The usual Pi peripheral crate was archived on 1 July 2025 (no further updates; works on boards up to Pi 5). Use the kernel's generic interfaces instead: `gpiocdev` (GPIO char device, async via tokio) and `linux-embedded-hal` / `i2cdev` for I²C. These are board-agnostic.
2. **No Adafruit PN532 library.** Use the `pn532` crate (no_std, embedded-hal, I²C interface, NTAG read requests) behind our own trait, or a small in-house driver if it fights us. Last released March 2024: "works, not actively maintained".

## Hardware notes that affect software

- **LED PWM:** only GPIO12 of the LED pins (12/16/26) is a hardware PWM pin. Mixed colours need the kernel `pwm-gpio` software-PWM driver (hrtimer based, Pi kernel 6.6.62+, in Trixie); every pin becomes a `/sys/class/pwm` channel.
- **PN532 on Pi I²C:** clock stretching is handled badly by the Pi's controller. Lower the bus speed (`dtparam=i2c_arm_baudrate=100000` or less); budget for retries in the driver. Use the IRQ line (GPIO4) to wait for cards instead of polling.
- **Battery:** the LiFePO4wered/Pi+ (I²C 0x43) is owned by its own daemon. tapboxd reads state through it (`lifepo4wered-cli`), never from the bus, so there's no contention with the PN532.

## Target and build

| Decision | Choice | Why |
|---|---|---|
| OS | Raspberry Pi OS **Lite 64-bit, Trixie** | Current release, arm64 on Zero 2 W |
| Target | `aarch64-unknown-linux-musl` (static) | No glibc-version coupling to the OS; copy one file and it runs |
| Cross build | **cargo-zigbuild** (dev and CI) | No Docker, no sysroot |
| Pure-Rust deps only | `zbus` (not `bluer`), `rustls` + ring (not OpenSSL) | `bluer` links libdbus and needs its headers, which breaks the easy static build |
| Toolchain | `rust-toolchain.toml` pinned; `Cargo.lock` committed; `--locked` | Reproducible builds for contributors |
| Packaging | **cargo-deb** with systemd units | One `.deb` installs binary + unit + config |

## Booting and staying up

PipeWire runs as a user service, so the audio stack runs as a dedicated `tapbox` user with lingering enabled and `tapboxd` is a **user unit** of that user (`deploy/tapboxd.service`).

- **`Type=notify` + `sd-notify`** (pure Rust): `READY=1` once the reader, player and web server are up; the watchdog starts counting after that.
- **Watchdog ping from the main event loop**, not a separate timer task, so a deadlocked core stops the pings and gets restarted.
- **Hardware watchdog** for kernel hangs: `RuntimeWatchdogSec=15s` in `/etc/systemd/system.conf`.
- `DynamicUser=` doesn't fit (needs fixed groups `gpio`, `i2c`, a runtime dir for PipeWire). Run `systemd-analyze security --user tapboxd` and tighten from there.
- **Power-loss safety:** the master switch cuts power without warning. Read-only root + overlayfs; the only state tapboxd writes is the parents' max volume, atomically (temp → fsync → rename → fsync dir) and only when it changes. Playback state lives in memory: switching off is a fresh start. Wi-Fi and Bluetooth state (NetworkManager, BlueZ) goes on the writable data partition.

## Architecture: functional core, imperative shell

```
reader ──┐                          ┌─> player (sim / mpv)
buttons ─┼─> Event ─> Core ─> Command ┼─> LED
bt/airplay┤   (pure, no clock)       ├─> Bluetooth / AirPlay
battery ─┘                          └─> state store
```

- `Core::handle(now, Event) -> Vec<Command>`: no tokio, no I/O, no clock. Every rule is a unit test, and the testbench can drive it on a virtual clock.
- The shell (`App` in `tapboxd`) owns the drivers and carries out commands. The same `App` runs the daemon, the interactive testbench and the scenario runner; only the clock and the drivers differ.
- Hardware sits behind small interfaces with simulated versions, so everything runs on a laptop.
- `tokio` current-thread runtime.

## Crate picks

| Need | Crate | Status |
|---|---|---|
| Card codec | own (`tapbox-card`) | done |
| State machine | own (`tapbox-core`) | done |
| Async runtime | `tokio` (current-thread) | in use |
| HTTP client (downloads) | `reqwest` + `rustls` (ring) | in use |
| Hashing / `ni:` | `sha2`, `base64` (URL-safe, no pad) | in use |
| Config / state | `serde`, `serde_json`, `toml` | in use |
| systemd | `sd-notify` | in use |
| Logging | `tracing`, `tracing-subscriber` (journald later) | in use |
| Player | mpv as child process, own typed JSON-IPC client | in use |
| GPIO buttons | `gpiocdev` (feature `async_tokio`), kernel debounce | next |
| I²C / NFC | `linux-embedded-hal` 0.4 + `pn532` 0.4 | next |
| LED | sysfs PWM (`pwm-gpio` overlay) | next |
| D-Bus / BlueZ | `zbus` | later |
| Web app | `axum` | later |

## Quality bar

- `#![forbid(unsafe_code)]` workspace-wide.
- Clippy pedantic; `unwrap_used`, `expect_used`, `panic` denied outside tests.
- Newtypes for confusable values: `Uid`, `ContentHash`, `Percent`, `Millis`.
- Parse, don't validate: `tapbox-card` returns typed `Card`/`Sources`; invalid states can't reach the core.
- Tests: unit tests, card-spec vectors, `proptest` (round-trips, never-panic on arbitrary bytes), testbench scenarios. `cargo-fuzz` on the NDEF parser is a next step.
- CI: `fmt --check`, `clippy`, `test --locked`, scenarios, `cargo deny`, aarch64-musl build.
- Licence: **MIT OR Apache-2.0**, matching the Rust ecosystem. go-librespot (GPL-3.0) stays a separate process.

## Sources

- [rppal (archived)](https://github.com/golemparts/rppal)
- [gpiocdev](https://docs.rs/gpiocdev) · [gpiocdev-rs repo](https://github.com/warthog618/gpiocdev-rs)
- [linux-embedded-hal releases](https://github.com/rust-embedded/linux-embedded-hal/releases)
- [pn532 crate](https://docs.rs/pn532)
- [BlueR](https://docs.rs/bluer)
- [sd-notify](https://docs.rs/sd-notify/)
- [Kernel software PWM on Pi GPIOs (Hackster)](https://www.hackster.io/news/linux-s-kernel-level-software-pwm-support-arrives-on-the-raspberry-pi-s-gpio-pins-0c1d71598647)
- [cargo-zigbuild on Pi Zero 2 W (TU Dresden)](https://robolab.inf.tu-dresden.de/en/knowledge/how-to/cross-compilation-zig)
- [cargo-deb](https://github.com/kornelski/cargo-deb)
- [Raspberry Pi OS (Trixie)](https://www.raspberrypi.com/documentation/usage/getting-started/getting-started-with-raspberry-pi)
- [Rust supply-chain tooling (cargo-deny / cargo-vet)](https://microsoft-rust-training.vercel.app/engineering-book/ch06-dependency-management-and-supply-chain-s)
