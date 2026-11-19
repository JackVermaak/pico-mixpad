# Pico Mixpad v1 – printed parts

All parts come from one parametric OpenSCAD file, `mixpad_v1.scad`. Ready-to-print STLs are in `stl/`,
already in print orientation (no supports needed).

| File | What it is | Size (mm) | Notes |
|---|---|---|---|
| `test.stl` | Fit test: one switch cutout, one encoder hole, both LED holes, an M2 screw boss | 64 × 34 × 12 | **Print first** |
| `oled_test.stl` | Fit test: the OLED window and pocket | 51 × 35 × 8 | **Print first** |
| `knob_fit_test.stl` | Three rings with 5.6 / 5.8 / 6.0 mm bores (1, 2, 3 notches) | 37 × 11 × 8 | Only if you print the big knob |
| `plate.stl` | Top plate: 8 keys, 5 encoders, OLED window and pocket, 2 LED holes, 4 corner screws | 110 × 135 × 8 | Printed top face down |
| `tray.stl` | Bottom tray: Pico supports, USB opening (left wall), 4 screw bosses, feet recesses | 110 × 135 × 32.4 | Open side up |
| `oled_clamp.stl` | Bar that holds the OLED in its pocket | 43 × 6 × 2.5 | |
| `knob.stl` | Optional 30 mm master knob (the encoders come with 15 mm knobs) | 29 × 29 × 16 | |

Assembled case: 110 × 135 × 36.4 mm without keycaps and knobs.

## Print settings

PLA or PETG, 0.2 mm layers, 3 walls, 4 top/bottom layers, 20% infill, no supports.
About 135 g of filament for the fit tests, plate, tray and clamp; on a Bambu Lab P1S the tray takes about 3 h and the plate about 1 h 45 min. The plate's top face is printed against the bed, so it
takes on the bed's finish: a smooth sheet gives a smooth top, textured PEI a textured one.

## Fit testing

1. Print `test.stl` and `oled_test.stl`.
2. Check against the real parts:
   - A switch clicks in and holds firmly → otherwise change `switch_cutout` (14.0 default; 14.1 if too tight, 13.95 if loose).
   - The encoder bushing goes through and the nut threads on → `encoder_hole` (7.4).
   - Both LEDs push in from below and stop at their rim → `led5_hole` / `led3_hole`.
   - An M2 screw bites into the boss without splitting it → `pilot_hole` (1.6).
   - The OLED drops into the pocket glass-first and the lit area sits inside the window →
     `oled_w`, `oled_h`, `oled_window_dy`. Measure your module with calipers; boards vary.
3. Change the values at the top of `mixpad_v1.scad`, then export the big parts:
   ```
   openscad -D 'part="plate"' -o plate.stl mixpad_v1.scad
   openscad -D 'part="tray"'  -o tray.stl  mixpad_v1.scad
   ```
   Or open the file in OpenSCAD, set `part = "plate";`, press F6 and export.

## Other options in the file

- `led_size = 5;` → `3` for the 3 mm LEDs.
- `usb_side = -1;` → `1` to move the USB cable to the right wall.
- `inner_h` (30 mm) is set by the Pico mounted upside down with jumper plugs on its header pins. Don't reduce it.

## Screws (M2 self-tapping)

| Where | Screw | Qty |
|---|---|---|
| Plate to tray (corners) | M2 × 10 | 4 |
| Pico to tray (far-end posts) | M2 × 6 | 2 |
| OLED clamp | M2 × 6 | 2 |

## How the parts go together

- **Pico:** upside down (chips facing the floor, header pins pointing up) on the four supports near the
  back. The two thin pegs at the USB end go into its mounting holes; two M2 × 6 screws hold the far end.
  The USB socket lines up with the opening in the left wall.
- **OLED:** glass first into the pocket on the underside of the plate, pins toward the back.
  Screw the clamp bar across its back.
- **Encoders:** from below, through the 7.4 mm holes; washer and nut on top. Snip off the small
  anti-rotation tab if the body won't sit flat.
- **Switches** clip in from the top; **LEDs** push in from below.
- **Feet:** stick the 12 mm feet into the four shallow recesses under the tray.
