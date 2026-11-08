# Hardware

Parts chosen to work together: power budget, amp-to-speaker match, pin map and enclosure fit are checked below. Check stock and prices before ordering.

## Parts list

### Electronics

| Part | Pick | Qty | Why |
|---|---|---|---|
| Computer | **Raspberry Pi Zero 2 WH** (pre-soldered header) | 1 | Minimum for AirPlay 2; header saves soldering |
| Battery / UPS | **PiSugar 3** (1200 mAh, for Pi Zero) | 1 | Pogo pins, keeps the GPIO free, up to 3 A output, USB-C charging, safe shutdown, I²C 0x57/0x68 |
| NFC reader | **PN532 NFC module V3** (Elechouse or equivalent) | 1 | Reads NTAG; set DIP switches to I²C (address 0x24) |
| Amplifier | **Adafruit MAX98357A I²S amp** (product 3006) | 1 | 3.2 W into 4 Ω at 5 V, no extra DAC needed |
| Speaker | **Tang Band W2-2243S**, 2", 4 Ω | 1 | Al/Mg cone, 86.7 dB sensitivity, 10 W handling, flat flange, 51.6 mm cutout, 36 mm deep |
| Speaker (local alternative) | Visaton FRS 5 XTS, 8 Ω | — | Easy to get from RS Norway; quieter, as the amp only gives 1.8 W into 8 Ω |
| Play button | **Sanwa OBSF-30** | 1 | Snap-in for 2–3.9 mm panels (our wall is 2.4 mm), 2.8 mm spade terminals |
| Volume buttons | **Sanwa OBSF-24** | 2 | Same family, 24 mm |
| Status LED | **5 mm diffused RGB LED, common cathode** + 3 × 220 Ω resistors | 1 | Runs directly from 3.3 V GPIO (see LED note) |
| NFC cards | **NTAG215 PVC cards**, white, printable | 10–25 | 504 bytes, the card-spec reference tag |
| microSD | **32 GB high-endurance** (SanDisk High Endurance or Samsung PRO Endurance) | 1 | Survives constant writes and power cuts better |
| USB-C | Short USB-C male → female **panel-mount** extension | 1 | Brings the PiSugar's charge port to the back lid |

### Mechanical and small parts

| Part | Spec | Qty |
|---|---|---|
| Heat-set inserts | M3 (Ø4.0 hole), M2.5 (Ø3.6 hole), e.g. ruthex | 4 + 4 |
| Screws | M3 × 8 (lid), M2.5 × 6 (speaker) | 4 + 4 |
| Standoffs | M2.5 × 6 mm + screws (Pi/PiSugar stack on the lid) | 4 |
| Acoustic fabric | to cover the Ø64 grille from inside | 1 piece |
| Foam gasket tape | 3 mm, for speaker flange and lid | 1 roll |
| Damping | a small tuft of polyester wadding inside the box | — |
| Feet | silicone bumpers Ø10 | 4 |
| Wiring | JST-PH 2-pin pigtails (speaker), female Dupont jumpers, 2.8 mm female spade connectors (buttons) | — |
| Filament | PETG ~250 g; clear PETG for the light pipe (optional) | — |

## Compatibility checks

- **Power:** Pi Zero 2 W + amplifier peaks + PN532 together stay well under the PiSugar 3's 3 A output. Power the amp from 5 V.
- **Amp ↔ speaker:** a 4 Ω driver gets the MAX98357A's full 3.2 W; the W2-2243S handles 10 W, so there's headroom. Hearing protection comes from the software volume cap.
- **Box:** ~0.9 L sealed volume is plenty for this driver (Vas 0.23 L). A little damping material inside smooths the response.
- **Fit:** speaker 36 mm deep in a 70 mm deep interior; arcade buttons ~25 mm deep; wall 2.4 mm is inside Sanwa's 2–3.9 mm snap-in range.
- **I²C:** PN532 (0x24) and PiSugar 3 (0x57/0x68) don't clash.
- **GPIO:** I²S uses 18/19/21, I²C 2/3, buttons 5/6/13, LED 12/16/26. No overlaps.
- **Speaker mount:** the enclosure uses a square insert pattern (`speaker_screw_spacing`). Measure your driver's four flange holes and set the parameter (holes on a circle of diameter d → square side d / √2).

## Wiring (BCM GPIO / physical pin)

| From | To | Pin |
|---|---|---|
| PN532 VCC / GND | 3.3 V / GND | 1 / 9 |
| PN532 SDA / SCL | GPIO2 / GPIO3 (I²C) | 3 / 5 |
| MAX98357A VIN / GND | 5 V / GND | 2 / 6 |
| MAX98357A BCLK | GPIO18 | 12 |
| MAX98357A LRC | GPIO19 | 35 |
| MAX98357A DIN | GPIO21 | 40 |
| LED red / green / blue (each via 220 Ω) | GPIO12 / GPIO16 / GPIO26 | 32 / 36 / 37 |
| LED common cathode | GND | 25 |
| Button vol − | GPIO5 → GND | 29 / 30 |
| Button play | GPIO6 → GND | 31 / 34 |
| Button vol + | GPIO13 → GND | 33 / 39 |
| PiSugar 3 | pogo pins under the Pi (5 V, GND, I²C) | — |

Notes:

- **LED change:** the earlier plan used a WS2812B on SPI. A WS2812B powered from 5 V wants a data signal of about 3.5 V or more, and the Pi only gives 3.3 V, so it can be flaky without a level shifter. A plain RGB LED on three GPIOs with PWM works reliably and is just as good for a status light.
- Buttons use the Pi's internal pull-ups.
- Use plugs (JST/Dupont/spade) everywhere so the lid with all electronics comes off without soldering.

## Where to buy (Norway)

- Raspberry Pi, Adafruit and Pimoroni parts: Kjell & Company, Elfa Distrelec, RS Norway, or The Pi Hut / Pimoroni in the UK (import VAT applies).
- Visaton: RS Norway / Elfa Distrelec.
- Tang Band: European speaker shops.
- Sanwa buttons: EU arcade parts shops.
- PiSugar: Tindie, Amazon or AliExpress.

## Caveats

- **Radio:** onboard Wi-Fi and Bluetooth share one chip and antenna. Disable Wi-Fi power management; a USB Bluetooth dongle is plan B.
- **NFC range:** 3–5 cm in free air, less through plastic. Keep the tray floor ≈ 1.2 mm and test range before closing the box.
- **Metal near the antenna** (battery, speaker magnet) detunes it. Keep ≥ 20 mm away.
