# Enclosure

Parametric model: [`hardware/enclosure/enclosure.scad`](../hardware/enclosure/enclosure.scad). Drawings: [design canvas](https://claude.ai/artifact/Rz8SQfL9gafV2fqdRtXBRE).

All dimensions in mm. They are based on typical parts. **Measure your actual parts and adjust the parameters before printing.** Where the drawings and the `.scad` file differ, the `.scad` file is the source of truth.

Render STLs (OpenSCAD 2021+):

```sh
openscad -D 'part="shell"' -o shell.stl hardware/enclosure/enclosure.scad
openscad -D 'part="lid"'   -o lid.stl   hardware/enclosure/enclosure.scad
```

Parts come out in print orientation. Both parts render cleanly in OpenSCAD 2021.01.

## Key dimensions

| Item | Value |
|---|---|
| Outer box (W × H × D) | 124 × 124 × 124: a cube with evenly rounded edges |
| Walls | 2.4 (top 3.2) |
| Corner radius, front view | R6 |
| Edge radius around the front face | R2 (prints on the bed) |
| Edge radius around the back face | R6 |
| Card tray | 90 × 58, centred on top (33 from front and back), 2 deep, 1.2 floor, Ø16 finger notch at the front |
| Grille | Ø64 at (62, 50) from top-left, Ø3 holes on 5 mm hex |
| LED light pipe | Ø5 at (62, 9) |
| Buttons | Ø24 at x = 27 and 97, Ø30 at x = 62; all at y = 102 from top |
| Back lid | 3 thick, inset 4 from the edge, R2 corners, rests on a 2 mm ledge |
| Lid screws | 4 × M3 × 8 at x = 12 / 112, 14 from bottom and 12 from top (100 × 98 pattern), into heat-set inserts (Ø4.0 holes, Ø8 bosses) |
| Speaker mount | 4 × M2.5 inserts (Ø3.3 holes) on a 50 mm square around the grille (`speaker_screw_spacing`, match your driver) |
| Pi Zero mount | M2.5 holes in the lid, 23 × 58, centred at x = 62, 56 from the bottom |
| USB-C power socket | 10 × 4.5 cutout, centred, 12 from the bottom, for a 6-pin panel-mount breakout screwed to the inside of the lid. Set `usbc_screw_dx` to your board's hole spacing for two Ø2.4 holes (M2 + nuts); 0 = glue it. The ledge notch widens to match |
| Power switch | Snap-in rocker TE PRASA1-16F-BB0BW (RS 718-2247), upright: 13 × 19.2 cutout in the back lid centred at x = 96 (model x; left of the power socket seen from the back), 26 from the bottom (`switch_*`). It needs a 0.75–1.25 mm panel, so an 18 × 25 pocket on the lid's inside thins it to 1.0 mm round the cutout; matching 18 × 25 notch in the lid ledge for the body (~12.8 × 18, ~21 deep) |
| Feet | 4 × Ø10 recesses, 1 deep, at x = 12 / 112, 15 / 109 from the front |

Coordinates "from top-left" are as seen from the front.

## Internal layout

1. Card tray on top, PN532 glued under the 1.2 mm floor, antenna up.
2. LED behind the light pipe at the top of the front.
3. 50 mm speaker behind the grille on four M2.5 inserts, foam gasket, acoustic fabric.
4. Arcade buttons below the speaker.
5. Pi Zero 2 W + MAX98357A on the inside of the back lid. The whole electronics stack comes out with the lid.
6. USB-C power socket and the rocker switch in the lid, side by side low down. No battery: the floor is free.

About 1.6 L of sealed volume, plenty for a 50 mm driver. Keep it airtight with gasket tape under the lid.

## Print settings

- PETG, 0.2 mm layers, 0.4 mm nozzle, 4 perimeters, 20 % gyroid infill.
- Shell: **front face on the bed** (needs 124 mm of Z height). Grille and button holes print clean without supports. The front edge radius is kept small (`r_front`) so the first layers don't overhang.
- Lid: flat, outside face down. The switch pocket then faces up and needs no support.
- No metal-filled, carbon or conductive filament: it blocks NFC.

## Assembly order

1. Press in heat-set inserts: 4 in the lid bosses, 4 at the speaker.
2. Glue fabric behind the grille, mount the speaker with gasket tape.
3. Snap in the buttons, fit the LED and light pipe.
4. Fix the PN532 under the tray floor; **test read range with a card before closing**.
5. Build the lid stack: standoffs, Pi, amp; screw (or glue) the USB-C board behind its cutout; snap the rocker into the lid. Wire USB-C VBUS → rocker → micro-USB lead into the Pi's PWR IN.
6. Wire with plugs so the lid comes off without soldering.
7. Gasket tape on the lid, close with 4 × M3 × 8.
