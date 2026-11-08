# Best-practice research

Collected 2026-11-08. Re-check versions and limits before relying on them.

## How HiRO works (the reference product)

- A screen-free kids' speaker controlled with physical cards, a "cassette player" where cassettes became plastic cards plus digital files.
- The parents' app connects the speaker to Wi-Fi to download content and updates. After download it works fully offline.
- Cards are sold separately and traded second-hand (Finn.no), which implies the right to play content is tied to the card's ID, not to an account.
- Takeaway: the card is a **key**, not storage. NTAG215 holds 504 bytes; one minute of 128 kbps audio is ~1 MB.

## Compute

- **Raspberry Pi Zero 2 W is the floor.** shairport-sync recommends at least a Pi 2 or Zero 2 W for AirPlay 2.
- A Pi 4 gives headroom if the web app, Spotify and AirPlay run together.

## NFC reader

- **PN532, not RC522.** The RC522 is limited to MIFARE Classic; the PN532 also reads MIFARE Ultralight and NTAG stickers.
- Use **I²C** (four wires, DIP switch set to I²C) with Adafruit's CircuitPython PN532 library on Blinka, in a venv. Many older Pi guides use libraries that no longer run on current Pi OS.
- Typical read range is 3–5 cm, less through thick plastic. Keep metal (battery, speaker magnet) away from the antenna.

## Audio output

- Use an **I²S amplifier** (MAX98357A) instead of the Pi's analog out, which is prone to noise.
- If using USB audio instead, Phoniebox recommends a ground-loop isolator against crackling.

## Bluetooth speaker mode (A2DP sink)

- Modern stack: **BlueZ → PipeWire/PipeWire-Pulse → ALSA**, works headless on Pi OS Lite.
- Enable sink role in WirePlumber: `bluez5.roles = [a2dp_sink]`.
- Headless pairing: a small BlueZ agent with `NoInputNoOutput` capability; `JustWorksRepairing = always` in `/etc/bluetooth/main.conf` allows re-pairing without interaction (Collabora's reference setup).
- For a kids' device, open pairing only on a button gesture with a timeout rather than staying discoverable.
- BlueALSA is the lighter alternative if PipeWire proves too heavy, but PipeWire gives one mixer for all sources.
- **Radio caveat:** Wi-Fi and Bluetooth share one chip and antenna on the Pi; A2DP plus Wi-Fi traffic can stutter. Leave room for a USB Bluetooth dongle and disable Wi-Fi power management.

## AirPlay speaker mode

- **shairport-sync** built with `--with-airplay-2`, plus **NQPTP** for timing. Most distro packages lag behind, so build from source or verify the packaged version supports AirPlay 2.
- NQPTP runs as root and needs exclusive UDP ports 319 and 320.
- Only one AirPlay 2 instance per IP.
- **Remote control is not implemented in AirPlay 2 mode**, so the box cannot pause the sender. Arbitration must take over the output locally.
- Session hooks (`run_this_before_play_begins`, `run_this_after_play_ends`) let the daemon know when AirPlay starts and stops.

## Spotify

- **Avoid the Web API for this.** Since the February 2026 changes, Development Mode needs the app owner to have Premium and allows at most five authorized users per Client ID.
- **go-librespot** makes the box a Spotify Connect device and exposes a local HTTP API; `POST /player/play` takes a Spotify URI, which is what a card needs.
- Headless login via device authorization: the daemon prints a code to enter at spotify.com/pair.
- Unofficial client: needs Premium in practice and may break. Keep it a plugin.

## Apple Music

- No official playback API for third-party hardware.
- Music Assistant has an Apple Music provider (paid subscription), but playback is not officially supported by Apple: use at own risk.
- Practical route: AirPlay from the phone, or an iPhone Shortcuts NFC automation.

## Headless setup

- **comitup**: if no known Wi-Fi is reachable, the device starts a hotspot with a captive portal that opens automatically on iOS/Android. Requires NetworkManager and systemd.
- Alternatives: balena WiFi Connect, pi-wifi-setup.

## Prior art

- **Phoniebox v3**: Python core, plugin system, web client; supports USB, RC522, PN532 and PC/SC readers, multiple readers, GPIO buttons and dials, RFID control cards (e.g. max volume), read-only filesystem guides.
- **ESPuino** (ESP32, Bluetooth out), **TonUINO** (Arduino, offline MP3).

## Sources

- [shairport-sync AirPlay 2 notes](https://github.com/mikebrady/shairport-sync/blob/master/AIRPLAY2.md)
- [shairport-sync README](https://github.com/mikebrady/shairport-sync/blob/master/README.md)
- [Collabora: Raspberry Pi as a Bluetooth speaker with PipeWire](https://collabora.com/news-and-blog/blog/2022/09/02/using-a-raspberry-pi-as-a-bluetooth-speaker-with-pipewire-wireplumber)
- [peppe8o: Bluetooth audio on Raspberry Pi OS Lite](https://peppe8o.com/fixed-connect-bluetooth-headphones-with-your-raspberry-pi/)
- [Phoniebox / RPi-Jukebox-RFID](https://github.com/MiczFlor/RPi-Jukebox-RFID)
- [PN532 on the Raspberry Pi 5](https://raspberry.tips/en/raspberrypi-tutorials/pn532-on-the-raspberry-pi-5)
- [go-librespot](https://github.com/devgianlu/go-librespot) and its [API spec](https://github.com/devgianlu/go-librespot/blob/master/api-spec.yml)
- [Spotify: Update on developer access (Feb 2026)](https://developer.spotify.com/blog/2026-02-06-update-on-developer-access-and-platform-security)
- [Music Assistant: Apple Music provider](https://www.music-assistant.io/music-providers/apple-music/)
- [comitup](https://github.com/davesteele/comitup)
- [HiRO Speaker app (App Store)](https://apps.apple.com/pg/app/hiro-speaker/id6745472618)
- [Tek.no: first look at HiRO](https://www.tek.no/nyheter/sniktitt/i/kwaq19/slik-er-norske-hiro)
