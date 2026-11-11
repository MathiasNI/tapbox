# Hardware

Parts chosen to work together: power budget, amp-to-speaker match, pin map and enclosure fit are checked below. Check stock and prices before ordering.

## Parts list

### Electronics

| Part | Pick | Qty | Why |
|---|---|---|---|
| Computer | **Raspberry Pi Zero 2 WH** (pre-soldered header) | 1 | Minimum for AirPlay 2; header saves soldering |
| Power supply | **Raspberry Pi 15 W USB-C power supply** (5.1 V, 3 A) | 1 | Mains powered, no battery. The 5.1 V gives margin for the drop over the switch and wiring |
| Power socket | **USB-C female panel-mount breakout, 6-pin, with 5.1 kΩ on CC1 and CC2**, screw-hole board | 1 | The box's power input on the back lid. The CC resistors are what make a USB-C supply switch on its 5 V; a plain 2-wire socket only works with USB-A-to-C cables. No resistors on the board → solder two 5.1 kΩ from CC1 and CC2 to GND |
| Pi power lead | **Micro-USB male to 2 bare wires, 22 AWG**, ~20 cm | 1 | From the switch into the Pi's PWR IN, so its input protection stays in the path |
| NFC reader | **PN532 NFC module V3** (Elechouse or equivalent) | 1 | Reads NTAG; set DIP switches to I²C (address 0x24). Prefer a board that breaks out the **IRQ** pin |
| Amplifier | **Adafruit MAX98357A I²S amp** (product 3006) | 1 | 3.2 W into 4 Ω at 5 V, no extra DAC needed |
| Speaker | **Tang Band W2-2243S**, 2", 4 Ω | 1 | Al/Mg cone, 86.7 dB sensitivity, 10 W handling, flat flange, 51.6 mm cutout, 36 mm deep |
| Speaker (local alternative) | Visaton FRS 5 XTS, 8 Ω | — | Easy to get from RS Norway; quieter, as the amp only gives 1.8 W into 8 Ω |
| Play button | **Sanwa OBSF-30** | 1 | Snap-in for 2–3.9 mm panels (our wall is 2.4 mm), 2.8 mm spade terminals |
| Volume buttons | **Sanwa OBSF-24** | 2 | Same family, 24 mm |
| Status LED | **5 mm diffused RGB LED, common cathode** + 3 × 220 Ω resistors | 1 | Runs directly from 3.3 V GPIO (see LED note) |
| Power switch | **TE Connectivity PRASA1-16F-BB0BW** (RS 718-2247), snap-in rocker, SPST ON–OFF, 16 A, 13 × 19.2 mm cutout, 4.75 mm quick-connect tabs | 1 | On/off on the back lid: breaks the 5 V line from the socket. Snaps into a 0.75–1.25 mm panel, so the lid is thinned locally (see `enclosure.md`). See power note |
| NFC cards | **NTAG215 PVC cards**, white, printable | 10–25 | 504 bytes, the card-spec reference tag |
| microSD | **32 GB high-endurance** (SanDisk High Endurance or Samsung PRO Endurance) | 1 | Survives constant writes and power cuts better |

AliExpress item numbers and exact variants: `parts-aliexpress.md`.

### Mechanical and small parts

| Part | Spec | Qty |
|---|---|---|
| Heat-set inserts | M3 × 5, OD 4.2 (Ø4.0 hole); M2.5 × 4, OD 3.5 (Ø3.3 hole) | 4 + 4 |
| Screws | M3 × 8 (lid), M2.5 × 6 (speaker) | 4 + 4 |
| Standoffs | M2.5 × 6 mm + screws (Pi on the lid) | 4 |
| Screws for the USB-C board | M2 × 6 + nuts, if your board has screw holes (else glue it) | 2 |
| Acoustic fabric | to cover the Ø64 grille from inside | 1 piece |
| Foam gasket tape | 3 mm, for speaker flange and lid | 1 roll |
| Damping | a small tuft of polyester wadding inside the box | — |
| Feet | silicone bumpers Ø10 | 4 |
| Wiring | JST-PH 2-pin pigtails (speaker), female Dupont jumpers (incl. one extra for PN532 IRQ), button cables with 2.8 mm spade → 2-pin Dupont plug, **22 AWG** silicone wire for the 5 V line (socket → switch → Pi lead), 2 × **4.8 mm insulated female spade connectors** for the rocker switch | — |
| Filament | PETG ~300 g; clear PETG for the light pipe (optional) | — |

## Compatibility checks

- **Power:** Pi Zero 2 W (≤ ~0.6 A busy, with Wi-Fi), amplifier peaks (~0.8 A at full 3.2 W), PN532 (~0.1 A) and LED come to ~1.5 A worst case, well inside the 3 A supply, the 16 A switch and 22 AWG wire. The amp takes its 5 V from the Pi's header, after the Pi's input.
- **Amp ↔ speaker:** a 4 Ω driver gets the MAX98357A's full 3.2 W; the W2-2243S handles 10 W, so there's headroom. Hearing protection comes from the software volume cap.
- **Box:** ~1.6 L sealed volume (124 mm cube) is plenty for this driver (Vas 0.23 L). A little damping material inside smooths the response.
- **Fit:** speaker 36 mm deep in a ~116 mm deep interior; arcade buttons ~25 mm deep; the rocker (body ~12.8 × 18, ~21 mm deep) sits upright on the lid at x = 96, 26 mm up, beside the Pi stack (x ≈ 47–77) and clear of the lid screw boss at x = 112; wall 2.4 mm is inside Sanwa's 2–3.9 mm snap-in range. With no battery the floor is free and the lid stack is just the Pi and the amp.
- **I²C:** only the PN532 (0x24) is on the bus.
- **GPIO:** I²S uses 18/19/21, I²C 2/3, buttons 5/27/13, LED 12/16/26, PN532 IRQ 4. No overlaps. The power switch uses no GPIO.
- **Speaker mount:** the enclosure uses a square insert pattern (`speaker_screw_spacing`). Measure your driver's four flange holes and set the parameter (holes on a circle of diameter d → square side d / √2).

## Wiring (BCM GPIO / physical pin)

| From | To | Pin |
|---|---|---|
| USB-C socket VBUS | rocker switch (spade) → micro-USB lead red → Pi **PWR IN** | — |
| USB-C socket GND | micro-USB lead black → Pi **PWR IN** | — |
| PN532 VCC / GND | 3.3 V / GND | 1 / 9 |
| PN532 SDA / SCL | GPIO2 / GPIO3 (I²C) | 3 / 5 |
| PN532 IRQ (optional, recommended) | GPIO4 | 7 |
| MAX98357A VIN / GND | 5 V / GND | 2 / 6 |
| MAX98357A BCLK | GPIO18 | 12 |
| MAX98357A LRC | GPIO19 | 35 |
| MAX98357A DIN | GPIO21 | 40 |
| LED red / green / blue (each via 220 Ω) | GPIO12 / GPIO16 / GPIO26 | 32 / 36 / 37 |
| LED common cathode | GND | 25 |
| Button vol − | GPIO5 → GND | 29 / 30 |
| Button play | GPIO27 → GND | 13 / 14 |
| Button vol + | GPIO13 → GND | 33 / 34 |

Notes:

- **Buttons:** each button's signal and GND are neighbouring pins, so one 2-pin Dupont plug per button fits straight onto the header.
- **LED change:** the earlier plan used a WS2812B on SPI. A WS2812B powered from 5 V wants a data signal of about 3.5 V or more, and the Pi only gives 3.3 V, so it can be flaky without a level shifter. A plain RGB LED on three GPIOs with PWM works reliably and is just as good for a status light.
- **LED PWM:** of GPIO12/16/26 only GPIO12 has hardware PWM. Mixed colours (orange, purple, dim green) need PWM on all three, so use the kernel's `pwm-gpio` software-PWM driver (Pi kernel 6.6.62+, included in Trixie) on all three pins, one overlay line per pin in `config.txt`. Check the exact overlay name and parameters in `/boot/firmware/overlays/README`.
- **PN532 IRQ:** lets the daemon wait for a card instead of polling the bus, and makes I²C reads more reliable. If your module has no IRQ pin, the daemon falls back to polling.
- **PN532 I²C speed:** the PN532 stretches the I²C clock, which the Pi's controller handles badly. Start with `dtparam=i2c_arm_baudrate=100000` (lower if reads fail) and test on the bench.
- **Power on/off:** the rocker on the back lid breaks the 5 V line between the USB-C socket and the Pi. **OFF is truly off**; ON boots the Pi (it starts whenever it gets power, no setting needed). Plug the micro-USB lead into **PWR IN**, the port nearest the end of the board, not the USB data port.
- **What OFF costs:** it is a hard power cut, with no shutdown, and that's fine: switching off is a fresh start. Playback state (resume positions, volume) is never written to disk. What the parents set up (max volume, card links, the library) is written only when they change it, atomically. Wi-Fi and Bluetooth pairings are kept by NetworkManager and BlueZ. The SD card still needs protecting from the cuts: a **read-only root filesystem is required**, with `/etc/NetworkManager/system-connections` and `/var/lib/bluetooth` on the writable data partition. Pulling the plug is the same as switching off. The testbench checks the behaviour (`power cut`, scenario 08).
- **Why no battery:** a lithium cell in a kids' toy needs a fuse, a protected holder and care when charging. On mains power there is nothing in the box above 5 V and 3 A, and the supply is a certified Raspberry Pi one. The box only plays while plugged in.
- **Check on arrival:** that the USB-C board has the two 5.1 kΩ CC resistors (two tiny "512" parts next to the socket; test with a USB-C-to-C cable); that the Pi doesn't report undervoltage at full volume (`vcgencmd get_throttled` should print `0x0`). If it does, shorten the 5 V wiring or add a 470 µF capacitor across the amp's VIN and GND.
- Buttons use the Pi's internal pull-ups; debounce is done in the kernel (GPIO character device).
- Use plugs (JST/Dupont/spade) everywhere so the lid with all electronics comes off without soldering.

## `config.txt` summary

```ini
dtparam=i2c_arm=on
dtparam=i2c_arm_baudrate=100000
dtoverlay=max98357a          # I²S amp
# software PWM for the RGB LED on GPIO12/16/26: see overlays/README for pwm-gpio syntax
```

## Where to buy (Norway)

- Raspberry Pi (including the 15 W USB-C supply), Adafruit and Pimoroni parts: Kjell & Company, Elfa Distrelec, RS Norway, or The Pi Hut / Pimoroni in the UK (import VAT applies).
- Power switch: RS Norway, stock no. 718-2247 (TE PRASA1-16F-BB0BW).
- Visaton: RS Norway / Elfa Distrelec.
- Tang Band: European speaker shops.
- Small parts and tools: AliExpress, see `parts-aliexpress.md`.

## Caveats

- **Radio:** onboard Wi-Fi and Bluetooth share one chip and antenna. Disable Wi-Fi power management; a USB Bluetooth dongle is plan B.
- **NFC range:** 3–5 cm in free air, less through plastic. Keep the tray floor ≈ 1.2 mm and test range before closing the box.
- **Metal near the antenna** (speaker magnet, screws) detunes it. Keep ≥ 20 mm away.
