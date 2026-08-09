//! Draw stick figure using embedded-graphics.

use crate::stickman::geometry::{
    arm_joint_angles, floor_y, leg_joint_angles, sin_cos_deg_milli, StickmanState, SHIN_LEN,
    THIGH_LEN,
};
use crate::DISPLAY_WIDTH;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle};

const WHITE: Rgb565 = Rgb565::WHITE;
const STROKE: u32 = 2;

const UPPER_ARM_LEN: i32 = 12;
const FOREARM_LEN: i32 = 11;

/// Draw the floor as a horizontal line near the bottom of the display.
pub fn draw_floor<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let y = floor_y();
    let style = PrimitiveStyle::with_stroke(WHITE, 1);
    Line::new(Point::new(0, y), Point::new(DISPLAY_WIDTH as i32 - 1, y))
        .into_styled(style)
        .draw(display)
}

/// Draw the stick figure at the given state in white.
pub fn draw_stickman<D>(display: &mut D, state: &StickmanState) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_stickman_colored(display, state, WHITE)
}

/// Draw the stick figure in an arbitrary color (used to erase with black).
pub fn draw_stickman_colored<D>(
    display: &mut D,
    state: &StickmanState,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let dir: i32 = if state.facing_left { -1 } else { 1 };
    let (x, y) = (state.x, state.y);
    let style = PrimitiveStyle::with_stroke(color, STROKE);

    // Body landmarks (y = floor / feet contact).
    let hip = Point::new(x, y - 28);
    let neck = Point::new(x, y - 52);
    let shoulder = Point::new(x, y - 46);
    let head_center = Point::new(x, y - 58);

    Circle::with_center(head_center, 12)
        .into_styled(style)
        .draw(display)?;
    Line::new(neck, hip).into_styled(style).draw(display)?;

    // Legs: opposite phase, each with hip + knee.
    let (l_hip, l_knee) = leg_joint_angles(state.leg_phase, 0);
    let (r_hip, r_knee) = leg_joint_angles(state.leg_phase, 50);
    draw_leg(display, hip, l_hip, l_knee, dir, style)?;
    draw_leg(display, hip, r_hip, r_knee, dir, style)?;

    // Arms: contralateral to legs (offset 50 / 0 vs leg 0 / 50).
    let arm_phase = state.arm_phase;
    let (l_sh, l_el) = arm_joint_angles(arm_phase, 50);
    let (r_sh, r_el) = arm_joint_angles(arm_phase, 0);
    draw_arm(display, shoulder, l_sh, l_el, dir, style)?;
    draw_arm(display, shoulder, r_sh, r_el, dir, style)?;

    Ok(())
}

fn joint_offset(origin: Point, angle_deg: i32, length: i32, dir: i32) -> Point {
    let (s, c) = sin_cos_deg_milli(angle_deg);
    Point::new(
        origin.x + dir * s * length / 1000,
        origin.y + c * length / 1000,
    )
}

fn draw_leg<D>(
    display: &mut D,
    hip: Point,
    hip_angle: i32,
    knee_bend: i32,
    dir: i32,
    style: PrimitiveStyle<Rgb565>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let knee = joint_offset(hip, hip_angle, THIGH_LEN, dir);
    // Knee flexion folds the shin back relative to the thigh.
    let shin_angle = hip_angle - knee_bend;
    let foot = joint_offset(knee, shin_angle, SHIN_LEN, dir);
    Line::new(hip, knee).into_styled(style).draw(display)?;
    Line::new(knee, foot).into_styled(style).draw(display)?;
    Ok(())
}

fn draw_arm<D>(
    display: &mut D,
    shoulder: Point,
    shoulder_angle: i32,
    elbow_bend: i32,
    dir: i32,
    style: PrimitiveStyle<Rgb565>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let elbow = joint_offset(shoulder, shoulder_angle, UPPER_ARM_LEN, dir);
    // Slight elbow crook so the forearm is not a stiff continuation.
    let forearm_angle = shoulder_angle + elbow_bend;
    let hand = joint_offset(elbow, forearm_angle, FOREARM_LEN, dir);
    Line::new(shoulder, elbow).into_styled(style).draw(display)?;
    Line::new(elbow, hand).into_styled(style).draw(display)?;
    Ok(())
}
