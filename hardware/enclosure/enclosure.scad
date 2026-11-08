// tapbox enclosure — parametric shell and back lid
//
// Units: mm.
// Axes:  x = width, left to right seen from the front
//        y = depth, front face at y = 0, back at y = D
//        z = height, bottom at z = 0
// "From top" values in the docs are converted with z = H - value.
//
// Render a part:  openscad -D 'part="shell"' -o shell.stl enclosure.scad
//                 openscad -D 'part="lid"'   -o lid.stl   enclosure.scad
// Parts come out in print orientation; "assembly" shows them in place.
//
// Measure your actual parts and adjust the parameters before printing.

part = "assembly"; // [assembly, shell, lid]

/* [Box] */
W = 130;          // width
H = 124;          // height
D = 76;           // depth
R = 14;           // corner radius seen from the front
r_front = 1.5;    // edge radius around the front face (small: this face prints on the bed)
r_back = 4;       // edge radius around the back face
wall = 2.4;       // side, front and bottom walls
top_wall = 3.2;   // top wall: tray_depth + tray floor

/* [Card tray] */
tray_w = 90;
tray_d = 58;
tray_depth = 2;   // leaves top_wall - tray_depth = 1.2 mm floor over the reader
tray_front = 12;  // tray front edge, from the front face
tray_r = 4;
notch_d = 16;     // finger notch at the tray's front edge

/* [Front] */
grille_d = 64;
grille_cz_top = 50;   // grille centre, from top
hole_d = 3;
hole_pitch = 5;       // hex grid pitch
led_d = 5;
led_cz_top = 9;
btn_cz_top = 102;
btn_x = [30, 65, 100];
btn_d = [24, 30, 24]; // vol -, play, vol +

/* [Speaker mount] */
speaker_screw_spacing = 50; // square pattern; match your driver
speaker_boss_d = 7;
speaker_boss_h = 5;
speaker_insert_d = 3.6;     // M2.5 heat-set insert

/* [Back lid] */
lid_t = 3;
lid_inset = 4;        // lid seat inset from the outer edge
lid_clear = 0.15;     // per side
flange_t = 2;         // ledge the lid rests on
flange_inset = 16;    // opening through the ledge, inset from the outer edge
screw_x = [12, 118];
screw_z = [14, 112];
boss_d = 8;
insert_d = 4.0;       // M3 heat-set insert
insert_depth = 7;
screw_d = 3.4;
cbore_d = 6.5;
cbore_h = 1.5;
usbc_w = 10;
usbc_h = 4.5;
usbc_cz = 12;         // from bottom
pi_cx = 65;
pi_cz = 56;           // Pi Zero mounted upright on the lid
pi_hole_dx = 23;
pi_hole_dz = 58;
pi_hole_d = 2.7;      // M2.5 standoffs

/* [Feet] */
foot_d = 10;
foot_depth = 1;
foot_x = [12, 118];
foot_y = [15, 61];

/* [Quality] */
$fn = 64;

eps = 0.01;

// ---------- helpers ----------

// Torus whose axis runs along y.
module torus_y(cx, y, cz, major, minor) {
    translate([cx, y, cz]) rotate([90, 0, 0])
        rotate_extrude() translate([major - minor, 0]) circle(r = minor);
}

// Rounded rectangle in the x/z plane, extruded from y0 to y1.
module rrect_y(x0, z0, x1, z1, rad, y0, y1) {
    hull() for (cx = [x0 + rad, x1 - rad], cz = [z0 + rad, z1 - rad])
        translate([cx, y1, cz]) rotate([90, 0, 0]) cylinder(r = rad, h = y1 - y0);
}

// Rounded rectangle in the x/y plane, extruded from z0 to z1.
module rrect_z(x0, y0, x1, y1, rad, z0, z1) {
    hull() for (cx = [x0 + rad, x1 - rad], cy = [y0 + rad, y1 - rad])
        translate([cx, cy, z0]) cylinder(r = rad, h = z1 - z0);
}

// Hole through the front wall.
module front_hole(x, z, d) {
    translate([x, wall + 1, z]) rotate([90, 0, 0]) cylinder(d = d, h = wall + 2);
}

// ---------- shell ----------

module outer_body() {
    hull() for (cx = [R, W - R], cz = [R, H - R]) {
        torus_y(cx, r_front, cz, R, r_front);
        torus_y(cx, D - r_back, cz, R, r_back);
    }
}

module shell_hollow() {
    difference() {
        outer_body();
        // main cavity, closed at the back by the lid ledge
        rrect_y(wall, wall, W - wall, H - top_wall, R - wall,
                wall, D - lid_t - flange_t + eps);
        // opening through the ledge
        rrect_y(flange_inset, flange_inset, W - flange_inset, H - flange_inset, 4,
                D - lid_t - flange_t - eps, D - lid_t + eps);
        // lid seat
        rrect_y(lid_inset, lid_inset, W - lid_inset, H - lid_inset, R - lid_inset,
                D - lid_t, D + 1);
    }
}

// Screw bosses behind the ledge, each tied to the nearest side wall by a rib.
module lid_bosses() {
    boss_len = insert_depth + 3;
    for (sx = screw_x, sz = screw_z) {
        x0 = sx < W / 2 ? wall - eps : sx;
        x1 = sx < W / 2 ? sx : W - wall + eps;
        hull() {
            translate([sx, D - lid_t, sz]) rotate([90, 0, 0]) cylinder(d = boss_d, h = boss_len);
            translate([x0, D - lid_t - boss_len, sz - boss_d / 2]) cube([x1 - x0, boss_len, boss_d]);
        }
    }
}

module speaker_bosses() {
    s = speaker_screw_spacing / 2;
    cz = H - grille_cz_top;
    for (dx = [-s, s], dz = [-s, s])
        translate([W / 2 + dx, wall - eps, cz + dz]) rotate([-90, 0, 0])
            cylinder(d = speaker_boss_d, h = speaker_boss_h + eps);
}

module grille_holes() {
    cz = H - grille_cz_top;
    row = hole_pitch * sqrt(3) / 2;
    lim = grille_d / 2 - hole_d / 2;
    n = ceil(grille_d / hole_pitch);
    for (j = [-n : n], i = [-n : n]) {
        x = i * hole_pitch + (j % 2 == 0 ? 0 : hole_pitch / 2);
        z = j * row;
        if (sqrt(x * x + z * z) <= lim) front_hole(W / 2 + x, cz + z, hole_d);
    }
}

module shell_cuts() {
    grille_holes();
    front_hole(W / 2, H - led_cz_top, led_d);
    for (k = [0 : len(btn_x) - 1]) front_hole(btn_x[k], H - btn_cz_top, btn_d[k]);

    // card tray and finger notch
    rrect_z((W - tray_w) / 2, tray_front, (W + tray_w) / 2, tray_front + tray_d, tray_r,
            H - tray_depth, H + 1);
    translate([W / 2, tray_front, H - tray_depth]) cylinder(d = notch_d, h = tray_depth + 1);

    // heat-set insert holes for the lid screws
    for (sx = screw_x, sz = screw_z)
        translate([sx, D - lid_t + eps, sz]) rotate([90, 0, 0])
            cylinder(d = insert_d, h = insert_depth);

    // speaker insert holes
    s = speaker_screw_spacing / 2;
    for (dx = [-s, s], dz = [-s, s])
        translate([W / 2 + dx, wall, H - grille_cz_top + dz]) rotate([-90, 0, 0])
            cylinder(d = speaker_insert_d, h = speaker_boss_h + 1);

    // notch in the ledge so the USB-C panel mount passes
    translate([W / 2 - usbc_w / 2 - 3, D - lid_t - flange_t - 1, usbc_cz - usbc_h / 2 - 3])
        cube([usbc_w + 6, flange_t + 2, usbc_h + 6]);

    // feet recesses
    for (fx = foot_x, fy = foot_y)
        translate([fx, fy, -eps]) cylinder(d = foot_d, h = foot_depth + eps);
}

module shell() {
    difference() {
        union() {
            shell_hollow();
            lid_bosses();
            speaker_bosses();
        }
        shell_cuts();
    }
}

// ---------- lid ----------

module lid() {
    c = lid_inset + lid_clear;
    difference() {
        rrect_y(c, c, W - c, H - c, R - c, D - lid_t, D);
        for (sx = screw_x, sz = screw_z) {
            translate([sx, D + 1, sz]) rotate([90, 0, 0]) cylinder(d = screw_d, h = lid_t + 2);
            translate([sx, D + eps, sz]) rotate([90, 0, 0]) cylinder(d = cbore_d, h = cbore_h + eps);
        }
        rrect_y(W / 2 - usbc_w / 2, usbc_cz - usbc_h / 2, W / 2 + usbc_w / 2, usbc_cz + usbc_h / 2,
                2, D - lid_t - 1, D + 1);
        for (dx = [-pi_hole_dx / 2, pi_hole_dx / 2], dz = [-pi_hole_dz / 2, pi_hole_dz / 2])
            translate([pi_cx + dx, D + 1, pi_cz + dz]) rotate([90, 0, 0])
                cylinder(d = pi_hole_d, h = lid_t + 2);
    }
}

// ---------- output ----------

if (part == "shell") {
    rotate([90, 0, 0]) shell();                    // front face on the bed
} else if (part == "lid") {
    translate([0, 0, D]) rotate([-90, 0, 0]) lid(); // outside face on the bed
} else {
    color("#F3D9C4") shell();
    color("#E8C2A5") lid();
}
