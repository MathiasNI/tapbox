# Hardware

Parts chosen to work together: power budget, amp-to-speaker match, pin map and enclosure fit are checked below. Check stock and prices before ordering.

## Parts list

### Electronics

| Part | Pick | Qty | Why |
|---|---|---|---|
| Computer | **Raspberry Pi Zero 2 WH** (pre-soldered header) | 1 | Minimum for AirPlay 2; header saves soldering |
| Battery / UPS | **LiFePO4wered/Pi+, 18650 version with stackable header** (Silicognition), rev 8 | 1 | LiFePO4 cell (2000 mAh high-capacity option, 3.2 V): the safest lithium chemistry for a kids' toy. Up to 2 A continuous, micro-USB charge input (check yours: the maker's brief says rev 8 has USB-C), safe shutdown, can stay plugged in. I²C 0x43 |
| NFC reader | **PN532 NFC module V3** (Elechouse or equivalent) | 1 | Reads NTAG; set DIP switches to I²C (address 0x24). Prefer a board that breaks out the **IRQ** pin |
| Amplifier | **Adafruit MAX98357A I²S amp** (product 3006) | 1 | 3.2 W into 4 Ω at 5 V, no extra DAC needed |
| Speaker | **Tang Band W2-2243S**, 2", 4 Ω | 1 | Al/Mg cone, 86.7 dB sensitivity, 10 W handling, flat flange, 51.6 mm cutout, 36 mm deep |
| Speaker (local alternative) | Visaton FRS 5 XTS, 8 Ω | — | Easy to get from RS Norway; quieter, as the amp only gives 1.8 W into 8 Ω |
| Play button | **Sanwa OBSF-30** | 1 | Snap-in for 2–3.9 mm panels (our wall is 2.4 mm), 2.8 mm spade terminals |
| Volume buttons | **Sanwa OBSF-24** | 2 | Same family, 24 mm |
| Status LED | **5 mm diffused RGB LED, common cathode** + 3 × 220 Ω resistors | 1 | Runs directly from 3.3 V GPIO (see LED note) |
| Power switch | **TE Connectivity PRASA1-16F-BB0BW** (RS 718-2247), snap-in rocker, SPST ON–OFF, 16 A, 13 × 19.2 mm cutout, 4.75 mm quick-connect tabs | 1 | Master switch on the back lid: cuts the battery. Snaps into a 0.75–1.25 mm panel, so the lid is thinned locally (see `enclosure.md`). See power note |
| Battery holder | **Single 18650 holder with wire leads** (or PCB holder + wires) | 1 | The cell moves out of the board's own holder so the switch can sit in its line |
| Fuse | **Inline mini blade fuse holder + 5 A mini blade fuse** | 1 | The board's BAT pads have no protection; the fuse guards the battery wiring |
| Battery plug | **2-pin connector rated ≥ 3 A** (JST-XH or XT30), not JST-PH (2 A) | 1 pair | So the lid with the board still comes off; carries the full battery current |
| NFC cards | **NTAG215 PVC cards**, white, printable | 10–25 | 504 bytes, the card-spec reference tag |
| microSD | **32 GB high-endurance** (SanDisk High Endurance or Samsung PRO Endurance) | 1 | Survives constant writes and power cuts better |
| Charge port | Panel-mount cable, **USB-C female → micro-USB male**, screw holes 17 mm apart | 1 | Gives the box a USB-C charge port on the back lid, wired to the battery board's micro-USB input |

AliExpress item numbers and exact variants: `parts-aliexpress.md`.

### Mechanical and small parts

| Part | Spec | Qty |
|---|---|---|
| Heat-set inserts | M3 × 5, OD 4.2 (Ø4.0 hole); M2.5 × 4, OD 3.5 (Ø3.3 hole) | 4 + 4 |
| Screws | M3 × 8 (lid), M2.5 × 6 (speaker) | 4 + 4 |
| Standoffs | M2.5 × 6 mm + screws (Pi + battery board stack on the lid) | 4 |
| Acoustic fabric | to cover the Ø64 grille from inside | 1 piece |
| Foam gasket tape | 3 mm, for speaker flange and lid | 1 roll |
| Damping | a small tuft of polyester wadding inside the box | — |
| Feet | silicone bumpers Ø10 | 4 |
| Wiring | JST-PH 2-pin pigtails (speaker), female Dupont jumpers (incl. one extra for PN532 IRQ), button cables with 2.8 mm spade → 2-pin Dupont plug, **22 AWG** silicone wire for the battery loop, 2 × **4.8 mm insulated female spade connectors** for the rocker switch | — |
| Battery fixing | VHB tape or two cable ties for the 18650 holder on the floor | — |
| Filament | PETG ~300 g; clear PETG for the light pipe (optional) | — |

## Compatibility checks

- **Power:** Pi Zero 2 W + amplifier peaks + PN532 together stay under the LiFePO4wered/Pi+'s 2 A continuous output. Power the amp from 5 V. On the battery side that is up to ~3.5 A at 3.2 V, which sets the switch, fuse, plug and wire sizes above.
- **Amp ↔ speaker:** a 4 Ω driver gets the MAX98357A's full 3.2 W; the W2-2243S handles 10 W, so there's headroom. Hearing protection comes from the software volume cap.
- **Box:** ~1.6 L sealed volume (124 mm cube) is plenty for this driver (Vas 0.23 L). A little damping material inside smooths the response.
- **Fit:** speaker 36 mm deep in a ~116 mm deep interior; arcade buttons ~25 mm deep; the 18650 holder (~77 × 21 × 20 mm) lies on the floor at x = 23–101, 28–49 mm from the front, below the lid stack (which starts ~23 mm up) and behind the arcade buttons (to 27 mm deep), and ~95 mm from the NFC antenna; the rocker (body ~12.8 × 18, ~21 mm deep) sits upright on the lid at x = 96, 26 mm up, beside the Pi stack (x ≈ 47–77) and clear of the lid screw boss at x = 112; wall 2.4 mm is inside Sanwa's 2–3.9 mm snap-in range. The battery board is 77.5 mm long; the deeper cube leaves plenty of room between the lid stack and the speaker.
- **I²C:** PN532 (0x24) and LiFePO4wered/Pi+ (0x43) don't clash. Only the LiFePO4wered daemon talks to the battery board; the tapbox daemon reads battery state from it, not from the bus.
- **GPIO:** I²S uses 18/19/21, I²C 2/3, buttons 5/27/13, LED 12/16/26, PN532 IRQ 4, battery board shutdown detect 14 (pin 8). No overlaps. The power switch uses no GPIO.
- **Speaker mount:** the enclosure uses a square insert pattern (`speaker_screw_spacing`). Measure your driver's four flange holes and set the parameter (holes on a circle of diameter d → square side d / √2).

## Wiring (BCM GPIO / physical pin)

| From | To | Pin |
|---|---|---|
| LiFePO4wered/Pi+ | sits on the header (stackable): 5 V, GND, I²C, GPIO14 shutdown detect | 2, 4 / GND / 3, 5 / 8 |
| 18650 cell + | 5 A fuse → rocker switch → battery plug → LiFePO4wered/Pi+ **BAT +** pad | — |
| 18650 cell − | battery plug → LiFePO4wered/Pi+ **BAT −** pad | — |
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
- **Power on/off (master switch):** a latching rocker on the back lid sits in the battery line. **OFF disconnects the cell: truly off, nothing drains.** ON reconnects it, and the board boots the Pi by itself: set `AUTO_BOOT` to 1 (`AUTO_BOOT_VBAT`, "boot whenever the battery is above the boot threshold") with `lifepo4wered-cli` and save it to the board's flash. The board still shuts the Pi down cleanly on low battery and boots again once charged.
- **What OFF costs:** it is a hard power cut, with no shutdown, and that's fine: switching off is a fresh start. Playback state (resume positions, volume) is never written to disk; only a parent's max-volume setting is saved (atomically, and only when it changes). Wi-Fi and Bluetooth pairings are kept by NetworkManager and BlueZ. The SD card still needs protecting from the cuts: a **read-only root filesystem is required**, with `/etc/NetworkManager/system-connections` and `/var/lib/bluetooth` on the writable data partition. The testbench checks the behaviour (`power cut`, scenario 08).
- **Charging:** with the switch OFF the cell is disconnected and **cannot charge**. Charge with the switch ON; the box runs while charging, as a UPS should. With the switch OFF and a charger plugged in, the board may still start the Pi from USB power alone: test this on the bench.
- **Battery wiring safety:** the board's BAT pads go straight to the cell with no protection, so the fuse goes right at the cell's + lead. Leave the board's own 18650 holder **empty** and tape over it, so nobody fits a second cell in parallel. Don't use the board's on-board button to shut down: with `AUTO_BOOT_VBAT` it would boot straight back up.
- **Check on arrival:** the BAT pad names and the `AUTO_BOOT` setting in the board's product brief; the rocker's voltage drop at ~3 A (its listed contact resistance is up to 100 mΩ, i.e. up to ~0.35 V at 3.5 A on a 3.2 V cell; a good one is far lower).
- **Battery board:** with the stackable header, the Dupont plugs go on the extended pins above it. Its installer enables I²C and the UART on GPIO14/15; leave pin 8 free.
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

- Raspberry Pi, Adafruit and Pimoroni parts: Kjell & Company, Elfa Distrelec, RS Norway, or The Pi Hut / Pimoroni in the UK (import VAT applies).
- LiFePO4wered/Pi+: Silicognition on Tindie (https://www.tindie.com/products/silicognition/lifepo4weredpi-2/), options: High-capacity 18650 2000 mAh, Stackable header rev 8, Include mounting kit, Normal compact mount.
- Power switch: RS Norway, stock no. 718-2247 (TE PRASA1-16F-BB0BW).
- Visaton: RS Norway / Elfa Distrelec.
- Tang Band: European speaker shops.
- Small parts and tools: AliExpress, see `parts-aliexpress.md`.

## Caveats

- **Radio:** onboard Wi-Fi and Bluetooth share one chip and antenna. Disable Wi-Fi power management; a USB Bluetooth dongle is plan B.
- **NFC range:** 3–5 cm in free air, less through plastic. Keep the tray floor ≈ 1.2 mm and test range before closing the box.
- **Metal near the antenna** (battery, speaker magnet) detunes it. Keep ≥ 20 mm away.
