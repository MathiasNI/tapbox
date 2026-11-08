# Setup and management without a screen

The box has buttons and one status light. Everything else happens in a web app on the phone, served by the box itself.

## 1. Getting onto Wi-Fi

Use [comitup](https://github.com/davesteele/comitup). If no known network is in range, the box starts a hotspot (`tapbox-<nnn>`) with a captive portal that opens automatically on iOS and Android. Pick the home network, enter the password, done. The LED blinks orange while waiting.

## 2. The web app (`http://<name>.local`)

- **Library:** upload files, add URLs or podcast feeds, paste Spotify links.
- **Link a card:** press "Link card", tap a card on the box, choose what it plays. The app updates live over a WebSocket.
- **Write a portable card:** the same flow writes the open card format (`card-spec.md`) to the card so it works in any tapbox. Phones can also write cards with NFC Tools.
- **Settings:** box name, max volume, what happens when a card is lifted, Bluetooth and AirPlay on/off, updates.

## 3. Spotify

- go-librespot makes the box a Spotify Connect speaker.
- Login: device authorization flow; the box prints/shows a code to enter at spotify.com/pair. The web app displays the code.
- Cards store a `spotify:` URI; the daemon calls go-librespot's local `POST /player/play`.
- No Spotify Web API app is needed (Development Mode limits make that a dead end).

## 4. Status light

| Pattern | Meaning |
|---|---|
| White pulse | Booting |
| Green flash | Card recognised |
| Red flash | Unknown card or error |
| Blue blinking | Bluetooth pairing open |
| Purple | AirPlay playing |
| Orange blinking | Wi-Fi setup mode |
| Dim green | Idle, ready |
| Red slow blink | Battery low |
| White fading out | Shutting down |

## 5. Gestures (no phone needed)

Power: the button **under the box** turns it on and off. Hold it until the LED reacts. Off means fully off, with clean shutdown first; it charges while off.

| Gesture | Action |
|---|---|
| Hold play 3 s | Open Bluetooth pairing for 2 minutes |
| Hold vol − and vol + together 5 s | Wi-Fi setup mode |
| Hold play 10 s | Safe shutdown (same as the power button) |
| Hold the power button (bottom) | On / clean shutdown |
| Hold the power button 10 s | Force power off (if the Pi hangs) |

## 6. Admin cards

Special cards (see `card-spec.md`, command records) for parents:

- Set max volume
- Open Bluetooth pairing
- Wi-Fi setup mode
- Read out the box's IP address / name
