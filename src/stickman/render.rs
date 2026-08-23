//! Floor line and pose strokes (embedded-graphics).

use crate::stickman::geometry::{self, rotate_point_cw, Segment, MAX_FLOOR_SEGS};
use crate::stickman::ir::{BoneKind, PoseScratch};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle};

const WHITE: Rgb565 = Rgb565::WHITE;
const BLACK: Rgb565 = Rgb565::BLACK;
const HEAD_STROKE: u32 = 2;
/// Pixels between successive arms of the crate's Archimedean fill.
const CRATE_SPIRAL_SPACING: i32 = 6;

/// Draw the full floor line (layer 0).
pub fn draw_floor<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let mut segs = [Segment::ZERO; MAX_FLOOR_SEGS];
    let n = geometry::fill_floor_segments(&mut segs);
    let style = PrimitiveStyle::with_stroke(WHITE, 1);
    for i in 0..n {
        let s = segs[i];
        if s.x0 == s.x1 && s.y0 == s.y1 {
            continue;
        }
        Line::new(Point::new(s.x0, s.y0), Point::new(s.x1, s.y1))
            .into_styled(style)
            .draw(display)?;
    }
    Ok(())
}

/// Draw the sampled pose in white.
pub fn draw_actor<D>(display: &mut D, pose: &PoseScratch) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let Some(species) = pose.species else {
        return Ok(());
    };
    for i in 0..pose.n {
        if !pose.visible[i] {
            continue;
        }
        match species.bones[i].kind {
            BoneKind::Joint => {}
            BoneKind::Line => draw_body_line(display, pose.origin[i], pose.tip[i])?,
            BoneKind::Circle { diameter } => {
                let stroke = if diameter >= 8 { HEAD_STROKE } else { 1 };
                Circle::with_center(pose.tip[i], diameter)
                    .into_styled(PrimitiveStyle::with_stroke(WHITE, stroke))
                    .draw(display)?;
            }
            BoneKind::Rect { width, height } => {
                draw_crate(display, pose.origin[i], width, height, pose.spin_deg)?;
            }
        }
    }
    if let Some(bubble) = pose.bubble.as_ref() {
        crate::stickman::bubble::draw(display, bubble)?;
    }
    Ok(())
}

/// Draw a 2px-thick segment (two parallel 1px strokes).
fn draw_body_line<D>(display: &mut D, a: Point, b: Point) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let style = PrimitiveStyle::with_stroke(WHITE, 1);
    Line::new(a, b).into_styled(style).draw(display)?;
    let (ox, oy) = if (b.x - a.x).abs() >= (b.y - a.y).abs() {
        (0, 1)
    } else {
        (1, 0)
    };
    Line::new(
        Point::new(a.x + ox, a.y + oy),
        Point::new(b.x + ox, b.y + oy),
    )
    .into_styled(style)
    .draw(display)
}

/// Filled crate: black body, white Archimedean spiral, white rim.
fn draw_crate<D>(
    display: &mut D,
    origin: Point,
    width: u32,
    height: u32,
    spin_deg: i32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let w = width as i32;
    let h = height as i32;
    let hw = w / 2;
    let left = origin.x - hw;
    let top = origin.y - h;
    if spin_deg == 0 {
        for ly in 0..h {
            for lx in 0..w {
                let color = if crate_cell_white(lx, ly, w, h) {
                    WHITE
                } else {
                    BLACK
                };
                Pixel(Point::new(left + lx, top + ly), color).draw(display)?;
            }
        }
        return Ok(());
    }

    let corners = geometry::rect_corners(origin, width, height, spin_deg);
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for p in corners {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let pivot = (origin.x, origin.y - h / 2);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let (lxw, lyw) = rotate_point_cw((x, y), pivot, -spin_deg);
            let lx = lxw - left;
            let ly = lyw - top;
            if lx < 0 || ly < 0 || lx >= w || ly >= h {
                continue;
            }
            let color = if crate_cell_white(lx, ly, w, h) {
                WHITE
            } else {
                BLACK
            };
            Pixel(Point::new(x, y), color).draw(display)?;
        }
    }
    Ok(())
}

/// White rim and a two-arm Archimedean swirl; everything else is black.
fn crate_cell_white(lx: i32, ly: i32, w: i32, h: i32) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    if lx <= 0 || ly <= 0 || lx >= w - 1 || ly >= h - 1 {
        return true;
    }
    let dx = lx - (w - 1) / 2;
    let dy = ly - (h - 1) / 2;
    if dx == 0 && dy == 0 {
        return true;
    }
    let deg = geometry::atan2_deg(dy, dx);
    let r = isqrt(dx * dx + dy * dy);
    let mut phase = deg - r * 360 / CRATE_SPIRAL_SPACING;
    phase %= 360;
    if phase < 0 {
        phase += 360;
    }
    phase < 180
}

fn isqrt(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stickman::library;

    #[test]
    fn crate_fill_has_white_spiral_and_black_gaps() {
        let w = library::BOX_WIDTH as i32;
        let h = library::BOX_HEIGHT as i32;
        let mut white = 0u32;
        let mut black = 0u32;
        for ly in 1..h - 1 {
            for lx in 1..w - 1 {
                if crate_cell_white(lx, ly, w, h) {
                    white += 1;
                } else {
                    black += 1;
                }
            }
        }
        assert!(white > 0, "spiral should paint some interior white");
        assert!(black > 0, "gaps between spiral arms should be black");
        assert!(
            white < (w * h) as u32,
            "crate should not be a solid white fill"
        );
        for x in 0..w {
            assert!(crate_cell_white(x, 0, w, h), "top rim {x}");
            assert!(crate_cell_white(x, h - 1, w, h), "bottom rim {x}");
        }
        for y in 0..h {
            assert!(crate_cell_white(0, y, w, h), "left rim {y}");
            assert!(crate_cell_white(w - 1, y, w, h), "right rim {y}");
        }
    }
}
