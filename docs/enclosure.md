# Enclosure

Parametric model: [`hardware/enclosure/enclosure.scad`](../hardware/enclosure/enclosure.scad). Drawings: [design canvas](https://claude.ai/artifact/Rz8SQfL9gafV2fqdRtXBRE).

All dimensions in mm. They are based on typical parts. **Measure your actual parts and adjust the parameters before printing.** Where the drawings and the `.scad` file differ, the `.scad` file is the source of truth.

Render STLs (OpenSCAD 2021+):

```sh
openscad -D 'part="shell"' -o shell.stl hardware/enclosure/enclosure.scad
openscad -D 'part="lid"'   -o lid.stl   hardware/enclosure/enclosure.scad
```

Parts come out in print orientation. The model has not been test-rendered yet; check it in OpenSCAD's preview first.

## Key dimensions

| Item | Value |
|---|---|
| Outer box (W × H × D) | 130 × 124 × 76 |
| Walls | 2.4 (top 3.2) |
| Corner radius, front view | R14 |
| Edge radius around the front face | R1.5 (prints on the bed) |
| Edge radius around the back face | R4 |
| Card tray | 90 × 58, 2 deep, 1.2 floor, Ø16 finger notch at the front |
| Grille | Ø64 at (65, 50) from top-left, Ø3 holes on 5 mm hex |
| LED light pipe | Ø5 at (65, 9) |
| Buttons | Ø24 at x = 30 and 100, Ø30 at x = 65; all at y = 102 from top |
| Back lid | 3 thick, inset 4 from the edge, R10 corners, rests on a 2 mm ledge |
| Lid screws | 4 × M3 × 8 at x = 12 / 118, 14 from bottom and 12 from top (106 × 98 pattern), into heat-set inserts (Ø4.0 holes, Ø8 bosses) |
| Speaker mount | 4 × M2.5 inserts on a 50 mm square around the grille (`speaker_screw_spacing`, match your driver) |
| Pi Zero mount | M2.5 holes in the lid, 23 × 58, centred at x = 65, 56 from the bottom |
| USB-C cutout | 10 × 4.5, centred, 12 from the bottom |
| Feet | 4 × Ø10 recesses, 1 deep |

Coordinates "from top-left" are as seen from the front.

## Internal layout

1. Card tray on top, PN532 glued under the 1.2 mm floor, antenna up.
2. LED behind the light pipe at the top of the front.
3. 50 mm speaker behind the grille on four M2.5 inserts, foam gasket, acoustic fabric.
4. Arcade buttons below the speaker.
5. Pi Zero 2 W + PiSugar 3 + MAX98357A on the inside of the back lid. The whole electronics stack comes out with the lid.
6. Panel-mount USB-C in the lid.

About 0.9 L of sealed volume, enough for a 50 mm driver. Keep it airtight with gasket tape under the lid.

## Print settings

- PETG, 0.2 mm layers, 0.4 mm nozzle, 4 perimeters, 20 % gyroid infill.
- Shell: **front face on the bed**. Grille and button holes print clean without supports. The front edge radius is kept small (`r_front`) so the first layers don't overhang.
- Lid: flat, outside face down.
- No metal-filled, carbon or conductive filament: it blocks NFC.

## Assembly order

1. Press in heat-set inserts: 4 in the lid bosses, 4 at the speaker.
2. Glue fabric behind the grille, mount the speaker with gasket tape.
3. Snap in the buttons, fit the LED and light pipe.
4. Fix the PN532 under the tray floor; **test read range with a card before closing**.
5. Build the lid stack: standoffs, PiSugar + Pi, amp, USB-C.
6. Wire with plugs so the lid comes off without soldering.
7. Gasket tape on the lid, close with 4 × M3 × 8.
