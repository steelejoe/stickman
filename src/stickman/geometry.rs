//! Floor polyline (flat + ramps) and integer trig for FK / spin.

use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::geometry::Point;

/// Pixels above the bottom of the display for the default floor line.
const FLOOR_MARGIN: i32 = 18;
/// Peak of the middle-third bump, pixels above [`floor_y`].
pub const FLOOR_BUMP: i32 = 20;
/// Cosine bump is four ramps; plus two flat wings.
pub const MAX_FLOOR_SEGS: usize = 8;

/// Standing pose: head-center distance above the feet (`y`).
pub const HEAD_CENTER_ABOVE_FEET: i32 = 58;
/// Standing pose: top-of-head distance above the feet (head diameter 12).
pub const STANDING_HEIGHT: i32 = HEAD_CENTER_ABOVE_FEET + 6;
/// Peak foot lift for a forward hop (~half standing height).
pub const JUMP_FORWARD_RISE: i32 = STANDING_HEIGHT / 2;

/// One floor (or ramp) edge. `x0 <= x1`. Y is the contact line (screen +Y down).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Segment {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Segment {
    pub const ZERO: Self = Self {
        x0: 0,
        y0: 0,
        x1: 0,
        y1: 0,
    };

    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Inclusive start, exclusive end, except a vertical or last-pixel cap.
    pub fn covers_x(self, x: i32, include_end: bool) -> bool {
        if self.x0 == self.x1 {
            return x == self.x0;
        }
        let last = include_end && x == self.x1;
        x >= self.x0 && (x < self.x1 || last)
    }

    pub fn y_at(self, x: i32) -> i32 {
        let dx = self.x1 - self.x0;
        if dx == 0 {
            return self.y0;
        }
        self.y0 + (self.y1 - self.y0) * (x - self.x0) / dx
    }

    /// Clockwise degrees from +X (screen +Y down) along this segment.
    pub fn slope_deg(self) -> i32 {
        atan2_deg(self.y1 - self.y0, self.x1 - self.x0)
    }
}

/// Default (flat) floor y — the wings of the polyline, not the bump peak.
pub fn floor_y() -> i32 {
    DISPLAY_HEIGHT as i32 - FLOOR_MARGIN
}

/// Contact y of the floor polyline at `x`.
pub fn floor_y_at(x: i32) -> i32 {
    let mut segs = [Segment::ZERO; MAX_FLOOR_SEGS];
    let n = fill_floor_segments(&mut segs);
    floor_y_at_in(&segs[..n], x)
}

pub fn floor_y_at_in(segs: &[Segment], x: i32) -> i32 {
    let n = segs.len();
    if n == 0 {
        return floor_y();
    }
    if x <= segs[0].x0 {
        return segs[0].y0;
    }
    for (i, s) in segs.iter().enumerate() {
        if s.covers_x(x, i + 1 == n) {
            return s.y_at(x);
        }
    }
    segs[n - 1].y1
}

pub fn floor_segment_in(segs: &[Segment], x: i32) -> Option<Segment> {
    let n = segs.len();
    segs.iter()
        .enumerate()
        .find(|(i, s)| s.covers_x(x, *i + 1 == n))
        .map(|(_, s)| *s)
}

/// Surface angle at `x` on the display floor (0 on a flat wing).
pub fn floor_slope_deg_at(x: i32) -> i32 {
    let mut segs = [Segment::ZERO; MAX_FLOOR_SEGS];
    let n = fill_floor_segments(&mut segs);
    floor_segment_in(&segs[..n], x)
        .map(Segment::slope_deg)
        .unwrap_or(0)
}

/// Integer `atan2(y, x)` in degrees, screen +Y down, range (-180, 180].
pub fn atan2_deg(y: i32, x: i32) -> i32 {
    if x == 0 && y == 0 {
        return 0;
    }
    let ax = x.abs();
    let ay = y.abs();
    let steep = ax < ay;
    let (num, den) = if steep { (ax, ay) } else { (ay, ax) };
    let mut best = 0i32;
    let mut best_e = i32::MAX;
    for d in 0..=90 {
        let s = sin_deg_milli(d);
        let c = sin_deg_milli(d + 90);
        let e = (num * c - den * s).abs();
        if e < best_e {
            best_e = e;
            best = d;
        }
    }
    let mut a = if steep { 90 - best } else { best };
    if x < 0 {
        a = 180 - a;
    }
    if y < 0 {
        a = -a;
    }
    a
}

/// Fill `out` with the display floor: flat, cosine bump in the middle third, flat.
/// Returns the segment count.
pub fn fill_floor_segments(out: &mut [Segment; MAX_FLOOR_SEGS]) -> usize {
    let base = floor_y();
    let w = DISPLAY_WIDTH as i32;
    let left = w / 3;
    let right = w * 2 / 3;
    let span = (right - left).max(1);
    const SAMPLES: i32 = 4;
    let mut n = 0usize;
    out[n] = Segment::new(0, base, left, base);
    n += 1;
    let mut prev_x = left;
    let mut prev_y = base;
    for i in 1..=SAMPLES {
        let t = i * 1000 / SAMPLES;
        let x = left + span * i / SAMPLES;
        let y = base - bump_height_px(t);
        out[n] = Segment::new(prev_x, prev_y, x, y);
        n += 1;
        prev_x = x;
        prev_y = y;
    }
    out[n] = Segment::new(right.max(prev_x), prev_y.min(base), w, base);
    n += 1;
    n
}

/// `FLOOR_BUMP * (1 - cos(2π t)) / 2` with `t` in milli (0..1000).
fn bump_height_px(t_milli: i32) -> i32 {
    let deg = t_milli * 360 / 1000;
    let cos = sin_deg_milli(deg + 90);
    FLOOR_BUMP * (1000 - cos) / 2000
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

/// Axis-aligned rect corners (bottom-center origin), then optional spin about
/// the geometric center. Order: top-left, top-right, bottom-right, bottom-left.
pub fn rect_corners(origin: Point, width: u32, height: u32, spin_deg: i32) -> [Point; 4] {
    let hw = width as i32 / 2;
    let h = height as i32;
    let pts = [
        (origin.x - hw, origin.y - h),
        (origin.x + hw, origin.y - h),
        (origin.x + hw, origin.y),
        (origin.x - hw, origin.y),
    ];
    if spin_deg == 0 {
        return [
            Point::new(pts[0].0, pts[0].1),
            Point::new(pts[1].0, pts[1].1),
            Point::new(pts[2].0, pts[2].1),
            Point::new(pts[3].0, pts[3].1),
        ];
    }
    let pivot = (origin.x, origin.y - h / 2);
    let rot = |p: (i32, i32)| {
        let (x, y) = rotate_point_cw(p, pivot, spin_deg);
        Point::new(x, y)
    };
    [rot(pts[0]), rot(pts[1]), rot(pts[2]), rot(pts[3])]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_wings_match_flat_baseline() {
        let base = floor_y();
        assert_eq!(floor_y_at(0), base);
        assert_eq!(floor_y_at(DISPLAY_WIDTH as i32 - 1), base);
        assert_eq!(floor_y_at(DISPLAY_WIDTH as i32 / 6), base);
        assert_eq!(floor_y_at(DISPLAY_WIDTH as i32 * 5 / 6), base);
    }

    #[test]
    fn floor_middle_third_peaks_near_20px() {
        let base = floor_y();
        let mid = DISPLAY_WIDTH as i32 / 2;
        let peak = floor_y_at(mid);
        assert_eq!(peak, base - FLOOR_BUMP);
        assert!(floor_y_at(DISPLAY_WIDTH as i32 / 3 + 8) < base);
        assert!(floor_y_at(DISPLAY_WIDTH as i32 * 2 / 3 - 8) < base);
    }

    #[test]
    fn floor_segments_are_left_to_right_and_cover_width() {
        let mut segs = [Segment::ZERO; MAX_FLOOR_SEGS];
        let n = fill_floor_segments(&mut segs);
        assert!(n >= 4 && n <= MAX_FLOOR_SEGS);
        assert_eq!(segs[0].x0, 0);
        assert_eq!(segs[n - 1].x1, DISPLAY_WIDTH as i32);
        for i in 0..n {
            assert!(segs[i].x1 >= segs[i].x0, "seg {i}");
        }
        for x in [0, 50, 178, 267, 400, 535] {
            let y = floor_y_at(x);
            assert!(y <= floor_y());
            assert!(y >= floor_y() - FLOOR_BUMP);
        }
    }

    #[test]
    fn atan2_deg_cardinals_and_diagonals() {
        assert_eq!(atan2_deg(0, 100), 0);
        assert_eq!(atan2_deg(100, 0), 90);
        assert_eq!(atan2_deg(0, -100), 180);
        assert_eq!(atan2_deg(-100, 0), -90);
        assert_eq!(atan2_deg(100, 100), 45);
        assert_eq!(atan2_deg(-100, 100), -45);
        assert_eq!(atan2_deg(-100, -100), -135);
    }

    #[test]
    fn floor_wings_are_flat_ramps_are_not() {
        assert_eq!(floor_slope_deg_at(40), 0);
        assert_eq!(floor_slope_deg_at(DISPLAY_WIDTH as i32 * 5 / 6), 0);
        let up = DISPLAY_WIDTH as i32 / 3 + 20;
        let down = DISPLAY_WIDTH as i32 * 2 / 3 - 20;
        assert!(floor_slope_deg_at(up) < 0, "left ramp climbs toward +X");
        assert!(
            floor_slope_deg_at(down) > 0,
            "right ramp descends toward +X"
        );
    }
}
