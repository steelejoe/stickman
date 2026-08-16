//! Floor line and pose strokes (embedded-graphics).

use crate::stickman::geometry::floor_y;
use crate::stickman::ir::{BoneKind, PoseScratch};
use crate::DISPLAY_WIDTH;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle};

const WHITE: Rgb565 = Rgb565::WHITE;
const HEAD_STROKE: u32 = 2;

/// Draw the full floor line (layer 0).
pub fn draw_floor<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let y = floor_y();
    let max_x = DISPLAY_WIDTH as i32 - 1;
    let style = PrimitiveStyle::with_stroke(WHITE, 1);
    Line::new(Point::new(0, y), Point::new(max_x, y))
        .into_styled(style)
        .draw(display)
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
        }
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
