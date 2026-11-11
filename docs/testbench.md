# Testbench

Try the box on a laptop before any hardware exists. The testbench runs the real daemon logic — card decoding, the state machine, the resolver, persistence — against simulated hardware: virtual cards, virtual buttons, a simulated LED, and a simulated (or real) audio player.

Everything lives in [`testbench/`](../testbench):

| Path | What |
|---|---|
| `tapbox.toml` | A normal daemon config: library, UID map for blank cards, core settings |
| `cards/*.json` | Virtual cards. Each is encoded to real NDEF bytes and decoded by the same code a PN532 reader will use |
| `library/` | Audio found by content hash (two short test tones) |
| `scenarios/*.tbx` | Scripted tests, run on a virtual clock |
| `state/` | Saved state of interactive sessions (git-ignored) |

## Interactive: `tapboxd sim`

```sh
cargo run -p tapboxd -- sim            # simulated audio
cargo run -p tapboxd -- sim --audio mpv  # real audio through mpv (install mpv first)
cargo run -p tapboxd -- sim --fresh    # forget saved settings, links and the library index
cargo run -p tapboxd -- sim --setup    # start in Wi-Fi setup, as a new box does
cargo run -p tapboxd -- sim --no-web   # console only
```

The sim also serves the real web app at **http://127.0.0.1:8080** (`--web <addr>` to change it). It's the same page the box serves, plus a **Testbench** panel to tap cards, press buttons and start AirPlay/Bluetooth; the console and the page drive the same app, so both stay in sync. Wi-Fi is simulated: `Home`, `Home-5G`, `Neighbour` (any password of 8+ characters works, except `wrongpassword`) and the open `Cafe guest`. Uploads land in `testbench/library/` (git-ignored except the two tones); links and settings in `testbench/state/`.

```text
tapbox> tap story-440
  → LED green flash (card recognised)
  → play card 04a1000001a0b0 from 0s
  → playing testbench/library/tone-440.wav from 0s
  → LED green (playing)
tapbox> hold play 3s
  → LED white flash (acknowledged)
  → Bluetooth pairing open for 120s
  → LED blue blinking (Bluetooth pairing)
tapbox> status
tapbox> problems
     0s ago  spotify-link     nothing on this card can be played here: spotify:album:4aawyAB9vmqN3uQ7FjRGTy
```

Every red flash also logs a problem with the reason; `problems` lists them, newest first. The web app shows the same list.

Commands run in real time: `hold play 10s` really takes ten seconds.

## Scenarios: `tapboxd test`

```sh
cargo run -p tapboxd -- test                       # all of testbench/scenarios
cargo run -p tapboxd -- test testbench/scenarios/05-bluetooth-airplay-priority.tbx
```

Scenarios run on a virtual clock (`wait 5m` is instant) and are deterministic. They also run in `cargo test` and in CI, so every behaviour rule in `architecture.md` and `setup-ux.md` has a scenario that would catch a regression.

## Command language

Same in both modes. One command per line, `#` starts a comment.

| Command | Meaning |
|---|---|
| `tap <card>` / `lift` | Place / remove a card from `cards/` |
| `press`, `release`, `click <button>` | Buttons: `play`, `vol-up`, `vol-down` |
| `hold <button> <time>` | Press, wait, release |
| `wait <time>` | `500ms`, `3s`, `2m` |
| `bluetooth start\|stop`, `airplay start\|stop` | Another source takes or leaves the speaker |
| `wifi done` | Wi-Fi setup finished (connected, or the hotspot timed out) |
| `power off` | Clean shutdown (`poweroff`, an update) |
| `power cut` | Master switch OFF then ON: a fresh boot. Only the max volume survives |
| `player end\|fail` | End or fail the current track now |
| `track-length <time>` | Length of simulated tracks (default 5 min) |
| `expect led <state>` | `idle`, `playing`, `airplay`, `bt-pairing`, `wifi-setup`, `shutting-down` |
| `expect flash <kind>` | `card`, `error`, `ack`, during the last action |
| `expect problem <kind>` | A problem was logged during the last action: `invalid-card`, `unknown-card`, `control-cards-disabled`, `nothing-playable`, `download-failed`, `playback-failed` |
| `expect cmd <name>` | A command ran during the last action: `play`, `pause`, `resume`, `pause-bluetooth`, `mute-airplay`, `open-bluetooth-pairing`, `enter-wifi-setup`, `persist`, … |
| `expect playing\|paused <card>`, `expect idle` | Playback state |
| `expect volume <n>`, `expect max-volume <n>` | Volume |
| `expect position <card> <time>` | Resume position (in memory, this session), ±1 s |
| `expect loaded <text>` | The player has a file/URL containing `<text>` |
| `expect airplay-muted\|airplay-unmuted` | AirPlay mute state |
| `expect not …` | Negate `cmd` or `flash` (or anything else) |

"The last action" is everything that happened since the most recent non-`expect` line, including commands that ran during a `wait` or `hold`.

## Card files

```json
{
  "uid": "04a1000001a0b0",
  "description": "shown by `cards`",
  "phone-uri": "https://example.org/about",
  "card": { "v": 1, "t": "Title", "s": ["ni:///sha-256;…", "https://…"] }
}
```

Instead of `card`, a file may have `"uri": "https://…"` (a phone-written card), `"raw-hex": "03 10 …"` (malformed bytes), or nothing (a blank tag, looked up in `[uid-map]`). An invalid `card` payload is written as-is, so you can test rejection.

To add your own audio: drop a file in `library/`, get its hash with `sha256sum` or by writing a card with `tapboxd card encode`, and reference it as `ni:///sha-256;<base64url>`.

## Card tools

```sh
cargo run -p tapboxd -- card encode '{"v":1,"t":"Story","s":["https://example.org/a.mp3"]}'
cargo run -p tapboxd -- card decode '03 9b d2 1b …'
```

`encode` prints the bytes to write from NTAG page 4 and the size against NTAG213/215/216 capacity.

## What is simulated, and what comes later

| Part | Today | On the Pi |
|---|---|---|
| NFC reader | `tap` / `lift` | PN532 over I²C, IRQ on GPIO4 |
| Buttons | `press` / `release` | GPIO5 / 27 / 13 via `gpiocdev` |
| LED | printed | RGB on GPIO12/16/26, `pwm-gpio` |
| Audio | simulated clock, or mpv | mpv → PipeWire → MAX98357A |
| Bluetooth / AirPlay | `bluetooth` / `airplay` commands | BlueZ over D-Bus, shairport-sync hooks |
| Power switch / plug | `power cut` | Rocker switch in the 5 V line from the USB-C socket |
| Clean shutdown | `power off` | `poweroff` → SIGTERM to tapboxd |
| Wi-Fi setup | simulated networks in the web app | NetworkManager hotspot + captive DNS, Avahi |
| Speech | recorded only | TTS |
