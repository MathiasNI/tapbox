# tapbox

> Working name. Rename freely.

An open-source, screen-free NFC audio player for kids, in the spirit of HiRO, Toniebox and Yoto, that you build yourself from a Raspberry Pi, an NFC reader and a 3D-printed case.

Tap a card on the box and it plays a song, an audiobook, a podcast or a playlist. Cards use an **open format**, so a card made for one tapbox plays on any other tapbox, including a friend's.

## Three ways to use it

| Mode | What it does | Built on |
|---|---|---|
| **NFC cards** | A card triggers local files, URLs/podcasts or Spotify | PN532 + `tapboxd` (Rust) + mpv |
| **Bluetooth speaker** | Pair a phone and use the box as a normal BT speaker | BlueZ + PipeWire (A2DP sink) |
| **AirPlay speaker** | Shows up in the AirPlay picker on iPhone/Mac | shairport-sync + NQPTP |

All three play through one PipeWire mixer. The rule is **last source wins**: a card tap takes over the speaker even while AirPlay or Bluetooth is playing.

## Try it now, no hardware needed

```sh
cargo run -p tapboxd -- sim     # interactive: tap virtual cards, press virtual buttons;
                                #   the web app runs at http://127.0.0.1:8080
cargo run -p tapboxd -- test    # run the scripted scenarios
cargo test --workspace          # everything, including the scenarios
```

See [`docs/testbench.md`](docs/testbench.md).

## Status

- **Done:** card codec (`tapbox-card`), state machine (`tapbox-core`), daemon shell with resolver, hash-verified downloads, power-safe state and mpv playback; web app (Wi-Fi setup, library uploads, linking cards, problem log, settings); testbench with virtual cards and 8 scenarios.
- **Next:** hardware drivers (PN532, buttons, LED, battery), NetworkManager/Avahi behind the Wi-Fi setup, then Bluetooth/AirPlay integration and Spotify. See [Roadmap](#roadmap).

## Repo layout

```
crates/
  tapbox-card/   Card codec: NDEF TLV + application/vnd.tapbox+json. No I/O.
  tapbox-core/   State machine: events in, commands out. No I/O, no clock.
  tapboxd/       Daemon: resolver, player (sim/mpv), state store, testbench, CLI.
testbench/       Bench config, virtual cards, test audio, scenarios.
deploy/          systemd user unit, example /etc/tapbox/tapbox.toml.
card-spec/
  vectors/       Encoded test cards every implementation should decode.
docs/
  card-spec.md       Open NFC card format (draft v0.1)
  architecture.md    Components, audio stack, source priority
  software-rust.md   Rust research: crates, build, boot, quality bar
  testbench.md       Running and scripting the testbench
  hardware.md        Parts list, wiring, caveats
  setup-ux.md        Headless setup: Wi-Fi, web app, LED, gestures, Spotify
  enclosure.md       Enclosure dimensions, print settings, assembly
  research.md        Best-practice research, with sources
hardware/enclosure/  Parametric OpenSCAD model
```

## Building for the Pi

```sh
rustup target add aarch64-unknown-linux-musl
cargo install cargo-zigbuild
cargo zigbuild --release --target aarch64-unknown-linux-musl -p tapboxd
```

One static binary, no system libraries needed. CI builds it on every push.

## Design canvas

Dimensioned drawings, internal layout, wiring and print notes:
https://claude.ai/artifact/Rz8SQfL9gafV2fqdRtXBRE (private until you share it)

## Roadmap

1. ~~Card spec codec and test vectors~~
2. ~~Workspace, CI, lints, `cargo deny`, aarch64-musl build~~
3. ~~Core state machine + testbench on a laptop~~
4. **Hardware bring-up**: PN532 over I²C (+ IRQ), MAX98357A over I²S, buttons, RGB LED on `pwm-gpio`, LiFePO4wered battery status.
5. **Audio stack**: PipeWire, Bluetooth A2DP sink, shairport-sync AirPlay 2.
6. **Daemon on the Pi**: `tapboxd run`, systemd watchdog, `.deb`.
7. **Web app**: ~~library, upload, link cards, settings, Wi-Fi setup page~~; write portable cards, URLs and podcasts.
8. **Onboarding**: NetworkManager hotspot + captive DNS behind the setup page, admin cards.
9. **Plugins**: Spotify via go-librespot, podcasts and manifests, Home Assistant / Music Assistant.
10. **Hardening**: read-only root (overlayfs), safe shutdown, OTA updates.
11. **Enclosure v1**: print, test NFC range and acoustics, iterate.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Prior art worth studying

- [Phoniebox / RPi-Jukebox-RFID](https://github.com/MiczFlor/RPi-Jukebox-RFID): mature Pi RFID jukebox with a Python core, plugins and a web app in v3.
- [ESPuino](https://github.com/biologist79/ESPuino): ESP32-based, can output to Bluetooth speakers.
- [TonUINO](https://www.voss.earth/tonuino/): Arduino + DFPlayer, fully offline MP3s from SD.

## License

Code and card spec: dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Hardware files: CERN-OHL-P or CC BY 4.0 (not chosen yet). go-librespot is GPL-3.0; running it as a separate process does not affect this project's license.
