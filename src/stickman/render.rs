//! Floor line and pose strokes (embedded-graphics).

use crate::stickman::eval;
use crate::stickman::ir::{Actor, BoneKind, PoseScratch};
use crate::DISPLAY_WIDTH;
use crate::stickman::geometry::floor_y;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle};

const WHITE: Rgb565 = Rgb565::WHITE;
const HEAD_STROKE: u32 = 2;
const BODY_THICKNESS: i32 = 2;

/// Draw the full floor line (layer 0). Prefer a dirty span after erasing a pose.
pub fn draw_floor<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_floor_span(display, 0, DISPLAY_WIDTH as i32 - 1)
}

/// Redraw only `[x0, x1]` of the floor line (inclusive), clipped to the display.
pub fn draw_floor_span<D>(display: &mut D, x0: i32, x1: i32) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let y = floor_y();
    let max_x = DISPLAY_WIDTH as i32 - 1;
    let left = x0.clamp(0, max_x);
    let right = x1.clamp(0, max_x);
    if left > right {
        return Ok(());
    }
    let style = PrimitiveStyle::with_stroke(WHITE, 1);
    Line::new(Point::new(left, y), Point::new(right, y))
        .into_styled(style)
        .draw(display)
}

/// Inclusive X range of floor pixels a pose may cover (for dirty restore).
pub fn pose_floor_dirty_x(pose: &PoseScratch) -> (i32, i32) {
    let mut lo = i32::MAX;
    let mut hi = i32::MIN;
    for i in 0..pose.n {
        if !pose.visible[i] {
            continue;
        }
        lo = lo.min(pose.origin[i].x).min(pose.tip[i].x);
        hi = hi.max(pose.origin[i].x).max(pose.tip[i].x);
    }
    if lo > hi {
        return (0, 0);
    }
    let margin = BODY_THICKNESS + 2;
    (lo - margin, hi + margin)
}

/// Draw the sampled pose in white.
pub fn draw_actor<D>(
    display: &mut D,
    actor: &Actor,
    pose: &PoseScratch,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_actor_colored(display, actor, pose, WHITE)
}

/// Draw the sampled pose in an arbitrary color (erase with black).
pub fn draw_actor_colored<D>(
    display: &mut D,
    actor: &Actor,
    pose: &PoseScratch,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let species = eval::species_of(actor);
    for i in 0..pose.n {
        if !pose.visible[i] {
            continue;
        }
        match species.bones[i].kind {
            BoneKind::Joint => {}
            BoneKind::Line => draw_body_line(display, pose.origin[i], pose.tip[i], color)?,
            BoneKind::Circle { diameter } => {
                let stroke = if diameter >= 8 { HEAD_STROKE } else { 1 };
                Circle::with_center(pose.tip[i], diameter)
                    .into_styled(PrimitiveStyle::with_stroke(color, stroke))
                    .draw(display)?;
            }
        }
    }
    Ok(())
}

/// Draw a 2px-thick segment (two parallel 1px strokes).
fn draw_body_line<D>(
    display: &mut D,
    a: Point,
    b: Point,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let style = PrimitiveStyle::with_stroke(color, 1);
    Line::new(a, b).into_styled(style).draw(display)?;
    if BODY_THICKNESS < 2 {
        return Ok(());
    }
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
