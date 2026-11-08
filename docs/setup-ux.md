# Setup and management without a screen

The box has buttons and one status light. Everything else happens in a web app on the phone, served by the box itself.

## 1. Getting onto Wi-Fi

The box sets itself up from its own web app; there's no extra portal software.

1. With no known network in range (or after holding both volume buttons for 5 s, or "Change Wi-Fi" in Settings) the box opens an open hotspot, `tapbox-setup`. The light blinks orange.
2. The phone joins it. NetworkManager's shared mode hands out an address and answers every DNS name with the box, so iOS and Android open the captive-portal sheet by themselves. `tapboxd` answers the probe URLs (`/generate_204`, `/hotspot-detect.html`, `/connecttest.txt`, …) with a redirect to `/`.
3. The page lists nearby networks (signal, lock). Pick one, type the password, Connect.
4. The box joins the network with NetworkManager. On a wrong password it reopens the hotspot and the page shows the reason. On success the page says "All set: switch back to *Home* and open `http://<name>.local`".
5. From then on it's reachable on the home network at `http://<name>.local` (Avahi/mDNS). NetworkManager remembers the network; setup is only needed once.

## 2. The web app (`http://<name>.local`)

One HTML file served by `tapboxd` (`crates/tapboxd/src/web/ui.html`), no build step. The page follows the box live over server-sent events (`/api/events`), sends changes to `/api/action`, and streams uploads to `/api/upload`.

- **Now:** what's playing, play/pause, volume (up to the max), and the **problem log**: every red flash, newest first, with the card and the reason ("blank card that isn't linked to anything yet", "download failed: hash mismatch"). Unknown cards get a "Link" button.
- **Cards:** "Link a card" → tap a card on the box (it doesn't play; the light flashes white) → pick a file from the library, name it, Save. A link on the box overrides what's written on the card. Change or remove links.
- **Library:** upload music and stories (MP3, M4A/M4B, AAC, OGG, Opus, FLAC, WAV, up to 2 GiB). Files are hashed while streaming in; the same file twice is stored once. Delete warns when cards use the file.
- **Settings:** box name, max volume, lift behaviour (pause or keep playing), "Pair a phone" for Bluetooth, Wi-Fi status and "Change".
- **AirPlay and Bluetooth need no setup.** AirPlay (shairport-sync) shows up under the box name. Phones paired once reconnect by themselves; a new phone needs "Pair a phone" (or holding play for 3 s) once.
- **Later:** add URLs, podcast feeds and Spotify links; write portable cards (`card-spec.md`) from the same flow; updates.

The page has no login: anyone on the home network can manage the box, as with most speakers. Upload names are sanitised and only audio extensions are accepted.

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
| Red flash | Card problem: unknown, broken, or nothing on it can be played. The reason is in the web app's problem list |
| Blue blinking | Bluetooth pairing open |
| Purple | AirPlay playing |
| Orange blinking | Wi-Fi setup mode |
| Dim green | Idle, ready |
| Red slow blink | Battery low |
| White fading out | Shutting down (low battery) |

## 5. Gestures (no phone needed)

Power: the **switch on the back** turns it on and off. ON boots the box (white pulse); OFF cuts power at once. Switching off is a fresh start: next time, put a card on and it plays from the beginning at the default volume (a parent's max-volume card is remembered). Wi-Fi and Bluetooth pairings are kept. It only charges with the switch ON.

| Gesture | Action |
|---|---|
| Hold play 3 s | Open Bluetooth pairing for 2 minutes |
| Hold vol − and vol + together 5 s | Wi-Fi setup mode |
| Power switch (back) | On / off |

## 6. Admin cards

Special cards (see `card-spec.md`, command records) for parents:

- Set max volume
- Open Bluetooth pairing
- Wi-Fi setup mode
- Read out the box's IP address / name
