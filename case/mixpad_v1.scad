// Pico Mixpad v1 - printed parts (hand-wired version)
// OpenSCAD 2021.01 or newer. Every part comes from this one file.
//
// Export one part at a time, either in the OpenSCAD GUI (set `part` below, F6, then File > Export > STL)
// or from the command line:
//   openscad -D 'part="plate"' -o plate.stl mixpad_v1.scad
//
// Parts: "test", "oled_test", "plate", "tray", "oled_clamp", "knob", "knob_fit_test", "assembly"
//
// Coordinates: x = left/right (0 = centre), y = front (0) to back, z = up (0 = underside of the tray).
//
// MEASURE BEFORE PRINTING THE BIG PARTS: print "test" and "oled_test" first (they take ~20 min each) and
// adjust the tolerances below until the switch, encoder, LEDs, screws and OLED fit.

part = "assembly";

/* [Fit tolerances - tune with the test prints] */
switch_cutout   = 14.0;   // Cherry MX plate cutout; 14.0 nominal, raise to 14.1 if switches won't seat
encoder_hole    = 7.4;    // EC11 M7 bushing
led5_hole       = 5.1;    // 5 mm LED body
led3_hole       = 3.1;    // 3 mm LED body
pilot_hole      = 1.6;    // M2 self-tapping screw into plastic
clearance_hole  = 2.4;    // M2 screw passing through
knob_bore       = 5.8;    // ribbed 6 mm shaft press fit (tune with "knob_fit_test")

/* [Case] */
W        = 110;   // outer width
D        = 135;   // outer depth
wall     = 2.5;
floor_t  = 2.4;
inner_h  = 30;    // floor top to plate underside (room for the Pico + jumper plugs)
plate_t  = 4.0;
corner_r = 4;

/* [Layout] */
pitch    = 19.05;                          // standard key spacing
key_rows = [18, 18 + 19.05];               // y of the two key rows (front row first)
knob_y   = 60;
knob_xs  = [-34.5, -11.5, 11.5, 34.5];     // the four app knobs
master   = [34.5, 86];                     // master knob
oled_c   = [-22, 86];                      // centre of the OLED module
led_size = 5;                              // 5 or 3 (mm) - which LEDs you fit
leds     = [[6, 86], [14, 86]];            // red, green

/* [OLED module: 0.96" 128x64 I2C, pins along one edge] */
oled_w        = 27.8;   // module board size (measure yours); 27.8 fits the common 27.3 x 27.8 boards
oled_h        = 27.8;
oled_t        = 4.1;    // glass face to back of the board
oled_window   = [25, 14];
oled_window_dy = -1.5;  // window offset from module centre, + = toward the pin edge (pins face the back)

/* [Pico 2 H, mounted upside down so its header pins point up] */
pico_len   = 51;
pico_wid   = 21;
pico_holes = [[-23.5, -5.7], [23.5, -5.7], [-23.5, 5.7], [23.5, 5.7]];   // 47 x 11.4 mm hole pattern
pico_post_h = 7;      // lifts the board so the USB plug clears the floor
usb_side    = -1;     // -1 = cable exits the left wall, 1 = right wall

$fn = 64;
eps = 0.01;

inner_w = W - 2 * wall;
pico_c  = [usb_side * (inner_w / 2 - 0.6 - pico_len / 2), 114.5];
bosses  = [[-(inner_w / 2 - 3.2), wall + 3.2], [inner_w / 2 - 3.2, wall + 3.2],
           [-(inner_w / 2 - 3.2), D - wall - 3.2], [inner_w / 2 - 3.2, D - wall - 3.2]];

// ---------------------------------------------------------------- helpers

module rounded_rect(w, d, h, r) {
    // w x d footprint centred on x, starting at y = 0
    translate([-w / 2 + r, r, 0]) minkowski() {
        cube([w - 2 * r, d - 2 * r, h / 2]);
        cylinder(r = r, h = h / 2);
    }
}

module switch_cut() {
    // through-cutout + underside pocket that leaves a 1.5 mm skin for the MX clips
    translate([-switch_cutout / 2, -switch_cutout / 2, -eps]) cube([switch_cutout, switch_cutout, plate_t + 2 * eps]);
    translate([-7.8, -7.8, -eps]) cube([15.6, 15.6, plate_t - 1.5 + eps]);
}

module encoder_cut() {
    // M7 hole + 13.5 mm pocket so the plate is 2 mm thick where the nut clamps
    translate([0, 0, -eps]) cylinder(d = encoder_hole, h = plate_t + 2 * eps);
    translate([-6.75, -6.75, -eps]) cube([13.5, 13.5, plate_t - 2 + eps]);
}

module led_cut() {
    translate([0, 0, -eps]) cylinder(d = led_size == 5 ? led5_hole : led3_hole, h = plate_t + 2 * eps);
}

module oled_window_cut() {
    translate([-oled_window[0] / 2, -oled_window[1] / 2 + oled_window_dy, -eps])
        cube([oled_window[0], oled_window[1], plate_t + 2 * eps]);
    // 45-degree chamfer on top so the screen edges aren't shadowed
    translate([0, oled_window_dy, plate_t - 1.2])
        linear_extrude(height = 1.2 + eps, scale = [(oled_window[0] + 2.4) / oled_window[0], (oled_window[1] + 2.4) / oled_window[1]])
            square(oled_window, center = true);
}

module oled_pocket_walls() {
    // hangs below the plate (plate underside = z 0 here); module drops in glass-first
    difference() {
        translate([-oled_w / 2 - 1.8, -oled_h / 2 - 1.8, -oled_t]) cube([oled_w + 3.6, oled_h + 3.6, oled_t]);
        translate([-oled_w / 2 - 0.3, -oled_h / 2 - 0.3, -oled_t - eps]) cube([oled_w + 0.6, oled_h + 0.6, oled_t + 2 * eps]);
        // gap for the pin header on the back edge
        translate([-6, oled_h / 2 - 1, -oled_t - eps]) cube([12, 4, oled_t + 2 * eps]);
    }
    // two posts for the clamp bar, on the front half (away from the pins)
    for (sx = [-1, 1]) translate([sx * (oled_w / 2 + 4.8), -5, -oled_t]) difference() {
        cylinder(d = 6, h = oled_t);
        translate([0, 0, -eps]) cylinder(d = pilot_hole, h = oled_t + 2 * eps);
    }
}

// ---------------------------------------------------------------- parts

module plate() {
    difference() {
        union() {
            rounded_rect(W, D, plate_t, corner_r);
            // alignment lip that sits just inside the tray walls
            difference() {
                translate([0, wall + 0.3, -2]) rounded_rect(inner_w - 0.6, D - 2 * wall - 0.6, 2, 1.2);
                translate([0, wall + 1.5, -2 - eps]) rounded_rect(inner_w - 3.0, D - 2 * wall - 3.0, 2 + 2 * eps, 0.5);
            }
        }
        for (r = key_rows, i = [-1.5, -0.5, 0.5, 1.5]) translate([i * pitch, r, 0]) switch_cut();
        for (x = knob_xs) translate([x, knob_y, 0]) encoder_cut();
        translate([master[0], master[1], 0]) encoder_cut();
        for (p = leds) translate([p[0], p[1], 0]) led_cut();
        translate([oled_c[0], oled_c[1], 0]) oled_window_cut();
        translate([oled_c[0], oled_c[1], -eps]) translate([-oled_w / 2 - 0.3, -oled_h / 2 - 0.3, 0]) cube([oled_w + 0.6, oled_h + 0.6, 0.6]);
        // corner screws: clearance hole + recess for the pan head
        for (b = bosses) translate([b[0], b[1], -2 - eps]) {
            cylinder(d = clearance_hole, h = plate_t + 2 + 2 * eps);
            translate([0, 0, 2 + plate_t - 1.6]) cylinder(d = 4.6, h = 1.6 + 2 * eps);
        }
        // keep the lip clear of the screw bosses
        for (b = bosses) translate([b[0], b[1], -2 - eps]) cylinder(r = 4.3, h = 2 + eps);
        // and leave the space above the Pico free for the jumper wires to bend
        translate([pico_c[0] - pico_len / 2 - 4, pico_c[1] - pico_wid / 2 - 4, -2 - eps]) cube([pico_len + 8, pico_wid + 8, 2 + eps]);
    }
    translate([oled_c[0], oled_c[1], 0]) oled_pocket_walls();
}

module tray() {
    difference() {
        union() {
            difference() {
                rounded_rect(W, D, floor_t + inner_h, corner_r);
                translate([0, wall, floor_t]) rounded_rect(inner_w, D - 2 * wall, inner_h + eps, max(corner_r - wall, 0.5));
            }
            // corner bosses for the plate screws
            for (b = bosses) translate([b[0], b[1], 0]) cylinder(d = 7, h = floor_t + inner_h);
            // Pico supports (board sits upside down on them). The USB socket hangs below the board
            // between the two USB-end holes, so those get thin pegs; the far end gets screw posts.
            for (h = pico_holes) translate([pico_c[0] + h[0], pico_c[1] + h[1], 0]) {
                if (h[0] * usb_side > 0) {
                    cylinder(d = 3.0, h = floor_t + pico_post_h);
                    cylinder(d = 1.8, h = floor_t + pico_post_h + 1.2, $fn = 24);
                } else {
                    cylinder(d = 5, h = floor_t + pico_post_h);
                }
            }
        }
        for (b = bosses) translate([b[0], b[1], floor_t + inner_h - 12]) cylinder(d = pilot_hole, h = 12 + eps);
        for (h = pico_holes) if (h[0] * usb_side < 0)
            translate([pico_c[0] + h[0], pico_c[1] + h[1], floor_t + 1]) cylinder(d = pilot_hole, h = pico_post_h + eps);
        // USB plug opening, lined up with the Pico's connector
        translate([usb_side * (W / 2 - wall / 2), pico_c[1], floor_t + pico_post_h - 1.5])
            cube([wall + 2, 12, 10], center = true);
        // shallow recesses that locate the 12 mm rubber feet
        for (sx = [-1, 1], y = [14, D - 14]) translate([sx * (W / 2 - 14), y, -eps]) cylinder(d = 12.6, h = 0.6);
    }
}

module oled_clamp() {
    difference() {
        hull() for (sx = [-1, 1]) translate([sx * (oled_w / 2 + 4.8), 0, 0]) cylinder(d = 6, h = 2.5);
        for (sx = [-1, 1]) translate([sx * (oled_w / 2 + 4.8), 0, -eps]) cylinder(d = clearance_hole, h = 2.5 + 2 * eps);
    }
}

module knob() {
    // optional larger master knob (the encoders ship with 15 mm knobs)
    h = 16; d = 30;
    difference() {
        union() {
            cylinder(d = d - 2, h = h);
            for (a = [0 : 15 : 359]) rotate(a) translate([d / 2 - 1.6, 0, 0]) cylinder(d = 2.4, h = h - 1.5, $fn = 16);
            translate([0, 0, h - 1.5]) cylinder(d1 = d - 2, d2 = d - 5, h = 1.5);
        }
        translate([0, 0, -eps]) cylinder(d = knob_bore, h = 12 + eps);
        translate([0, 0, -eps]) cylinder(d1 = knob_bore + 1, d2 = knob_bore, h = 0.6);   // lead-in chamfer
        translate([d / 2 - 5, 0, h - 0.8]) cylinder(d = 2, h = 1);                       // position dot
    }
}

module knob_fit_test() {
    bores = [5.6, 5.8, 6.0];
    for (i = [0 : 2]) translate([i * 13, 0, 0]) difference() {
        cylinder(d = 11, h = 8);
        translate([0, 0, -eps]) cylinder(d = bores[i], h = 8 + 2 * eps);
        // notches on the rim count the variant: 1 = 5.6, 2 = 5.8, 3 = 6.0
        for (n = [0 : i]) rotate(90 + n * 25) translate([5.5, 0, 6.5]) cube([1.6, 1.2, 3.1], center = true);
    }
}

module test_plate() {
    difference() {
        union() {
            translate([-32, -17, 0]) cube([64, 34, plate_t]);
        }
        translate([-16, 0, 0]) switch_cut();
        translate([12, 4, 0]) encoder_cut();
        translate([6, -11, 0]) cylinder(d = led5_hole, h = 3 * plate_t, center = true);
        translate([14, -11, 0]) cylinder(d = led3_hole, h = 3 * plate_t, center = true);
        translate([26, -11, 0]) cylinder(d = clearance_hole, h = 3 * plate_t, center = true);
    }
    // boss under the plate to test the M2 pilot hole (screw in from the top through the 2.4 mm hole next to it)
    translate([26, 6, -8]) difference() {
        cylinder(d = 7, h = 8);
        translate([0, 0, -eps]) cylinder(d = pilot_hole, h = 7);
    }
}

module oled_test() {
    // just the plate area around the OLED, to check the window position and pocket fit
    difference() {
        translate([-oled_w / 2 - 12, -oled_h / 2 - 4, 0]) cube([oled_w + 24, oled_h + 8, plate_t]);
        oled_window_cut();
        translate([-oled_w / 2 - 0.3, -oled_h / 2 - 0.3, -eps]) cube([oled_w + 0.6, oled_h + 0.6, 0.6]);
    }
    oled_pocket_walls();
}

module assembly() {
    color("lightgrey") tray();
    color("dimgrey") translate([0, 0, floor_t + inner_h]) plate();
}

// Parts are output in print orientation: the plate and test pieces are flipped so their
// visible top face lies on the bed.
module flip() { translate([0, 0, plate_t]) rotate([180, 0, 0]) children(); }

if (part == "plate") flip() plate();
else if (part == "tray") tray();
else if (part == "oled_clamp") oled_clamp();
else if (part == "knob") knob();
else if (part == "knob_fit_test") knob_fit_test();
else if (part == "test") flip() test_plate();
else if (part == "oled_test") flip() oled_test();
else if (part == "none") { }
else assembly();
