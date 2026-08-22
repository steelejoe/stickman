//! Floor placement and integer trig for FK / spin.

/// Pixels above the bottom of the display for the floor line.
const FLOOR_MARGIN: i32 = 18;

/// Standing pose: head-center distance above the feet (`y`).
pub const HEAD_CENTER_ABOVE_FEET: i32 = 58;
/// Standing pose: top-of-head distance above the feet (head diameter 12).
pub const STANDING_HEIGHT: i32 = HEAD_CENTER_ABOVE_FEET + 6;

/// Y coordinate of the floor (feet contact line).
pub fn floor_y() -> i32 {
    crate::DISPLAY_HEIGHT as i32 - FLOOR_MARGIN
}

/// Feet `y` so the standing head center sits just above the screen midline.
pub fn jump_apex_foot_y(display_height: i32) -> i32 {
    let head_target = display_height / 2 - 6;
    head_target + HEAD_CENTER_ABOVE_FEET
}

/// sin/cos of an angle in degrees, milli-units [-1000, 1000].
/// Angle 0 = straight down (+Y); positive rotates toward +X before facing is applied.
pub fn sin_cos_deg_milli(deg: i32) -> (i32, i32) {
    let s = sin_deg_milli(deg);
    let c = sin_deg_milli(deg + 90);
    (s, c)
}

/// Rotate `p` around `origin` by `deg` degrees clockwise (screen y-down).
pub fn rotate_point_cw(p: (i32, i32), origin: (i32, i32), deg: i32) -> (i32, i32) {
    let (s, c) = sin_cos_deg_milli(deg);
    let dx = p.0 - origin.0;
    let dy = p.1 - origin.1;
    // y-down clockwise: (0,-1) at +90° → (+1, 0).
    (
        origin.0 + (dx * c - dy * s) / 1000,
        origin.1 + (dx * s + dy * c) / 1000,
    )
}

fn sin_deg_milli(deg: i32) -> i32 {
    // sin every 5° from 0..90
    const T: [i32; 19] = [
        0, 87, 174, 259, 342, 423, 500, 574, 643, 707, 766, 819, 866, 906, 940, 966, 985, 996, 1000,
    ];
    let mut a = deg % 360;
    if a < 0 {
        a += 360;
    }
    let (sign, r) = match a {
        0..=90 => (1, a),
        91..=180 => (1, 180 - a),
        181..=270 => (-1, a - 180),
        _ => (-1, 360 - a),
    };
    let i = (r / 5) as usize;
    let frac = r % 5;
    let v = if i >= 18 {
        1000
    } else {
        let lo = T[i];
        let hi = T[i + 1];
        lo + (hi - lo) * frac / 5
    };
    sign * v
}
