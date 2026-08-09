//! Draw stick figure using embedded-graphics.

use crate::stickman::geometry::{
    arm_joint_angles, floor_y, foot_x_rel, leg_joint_angles, rotate_point_cw, sin_cos_deg_milli,
    RollMode, StickmanState, HEAD_CENTER_ABOVE_FEET, SHIN_LEN, THIGH_LEN,
};
use crate::DISPLAY_WIDTH;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};

const WHITE: Rgb565 = Rgb565::WHITE;
/// Outline width for the head circle.
const HEAD_STROKE: u32 = 2;
/// Guaranteed on-screen thickness for torso and limbs (dual 1px strokes).
const BODY_THICKNESS: i32 = 2;

const UPPER_ARM_LEN: i32 = 12;
const FOREARM_LEN: i32 = 11;

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

/// Inclusive X range of floor pixels a stickman stroke may cover (for dirty restore).
pub fn stickman_floor_dirty_x(state: &StickmanState) -> (i32, i32) {
    let dir: i32 = if state.facing_left { -1 } else { 1 };
    let left_foot = state.x + dir * foot_x_rel(state.leg_phase, 0);
    let right_foot = state.x + dir * foot_x_rel(state.leg_phase, 50);
    let (lo, hi) = if left_foot <= right_foot {
        (left_foot, right_foot)
    } else {
        (right_foot, left_foot)
    };
    // Body thickness plus a little slack for joint rounding.
    let margin = BODY_THICKNESS + 2;
    (lo - margin, hi + margin)
}

/// Conservative display-space bounds covering the stickman stroke (dirty restore).
pub fn stickman_dirty_rect(state: &StickmanState) -> Rectangle {
    if state.roll_mode != RollMode::None {
        // Rolling poses spin about a point ~28px above the floor contact.
        const R: i32 = 48;
        let cy = state.y - 28;
        return Rectangle::new(
            Point::new(state.x - R, cy - R),
            Size::new((R * 2) as u32, (R * 2) as u32),
        );
    }
    // Sword poses: use a stable max extent (stance + full lunge/thrust) so
    // adjacent stab frames share one dirty tile and never trip the oversized
    // erase-then-draw path.
    let half_w = if state.sword_stance { 76 } else { 44 };
    // Standing head is ~64px above feet; jump/crouch stay within this pad.
    const ABOVE: i32 = HEAD_CENTER_ABOVE_FEET + 12;
    const BELOW: i32 = 4;
    Rectangle::new(
        Point::new(state.x - half_w, state.y - ABOVE),
        Size::new((half_w * 2) as u32, (ABOVE + BELOW) as u32),
    )
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
    let crouch = state.crouch.min(100) as i32;
    let head_style = PrimitiveStyle::with_stroke(color, HEAD_STROKE);

    match state.roll_mode {
        RollMode::Knockback => return draw_knockback(display, state, color),
        RollMode::Tumbling => return draw_side_tumble(display, state, color),
        RollMode::None => {}
    }

    // Crouch lowers the hips and shortens the visible torso a little.
    let hip_h = 28 - crouch * 16 / 100;
    let torso_h = 24 - crouch * 4 / 100;
    let stab = if state.sword_stance {
        state.sword_stab.min(100) as i32
    } else {
        0
    };
    // Stab lunges the hips forward with the leading foot.
    let hip = Point::new(x + dir * (stab * 10 / 100), y - hip_h);
    // Crouch (not begging) leans the torso ~30° toward the facing direction.
    // Stab adds a milder forward lean as the thrust extends.
    // Angle 0 = down; 180 = up; subtract lean so the spine tips forward.
    let lean = if crouch > 0 && !state.begging {
        crouch * 30 / 100 + if state.sword_stance { stab * 12 / 100 } else { 0 }
    } else {
        stab * 18 / 100
    };
    let torso_angle = 180 - lean;
    let neck = joint_offset(hip, torso_angle, torso_h, dir);
    let shoulder = joint_offset(hip, torso_angle, torso_h - 6, dir);
    let head_center = joint_offset(neck, torso_angle, 6, dir);

    Circle::with_center(head_center, 12)
        .into_styled(head_style)
        .draw(display)?;
    draw_body_line(display, neck, hip, color)?;

    if crouch > 0 {
        if state.sword_stance {
            draw_sword_crouch(display, hip, shoulder, dir, stab, color)?;
        } else {
            // Symmetric bent-knee crouch; feet stay on `y`.
            let hip_a = crouch * 22 / 100;
            let knee = 18 + crouch * 50 / 100;
            draw_leg(display, hip, hip_a, knee, dir, color)?;
            draw_leg(display, hip, -hip_a / 2, knee + 4, dir, color)?;

            if state.begging {
                // Arms reach forward/down toward the knees.
                let sh = 50 + crouch * 20 / 100;
                let el = 35 + crouch * 25 / 100;
                draw_arm(display, shoulder, sh, el, dir, color)?;
                draw_arm(display, shoulder, sh - 8, el + 6, dir, color)?;
            } else {
                // Arms hang by the sides (still roughly downward while torso leans).
                draw_arm(display, shoulder, 10, 18, dir, color)?;
                draw_arm(display, shoulder, -8, 22, dir, color)?;
            }
        }
    } else if state.sword_stance {
        draw_sword_stance(display, hip, shoulder, dir, stab, color)?;
    } else if state.y < floor_y() - 2 {
        // In-air jump: slight tuck in the legs, arms raised.
        draw_leg(display, hip, 18, 40, dir, color)?;
        draw_leg(display, hip, -14, 36, dir, color)?;
        draw_arm(display, shoulder, -150, 25, dir, color)?;
        draw_arm(display, shoulder, -135, 30, dir, color)?;
    } else {
        // Legs: opposite phase, each with hip + knee.
        let (l_hip, l_knee) = leg_joint_angles(state.leg_phase, 0);
        let (r_hip, r_knee) = leg_joint_angles(state.leg_phase, 50);
        draw_leg(display, hip, l_hip, l_knee, dir, color)?;
        draw_leg(display, hip, r_hip, r_knee, dir, color)?;

        // Arms: contralateral to legs (offset 50 / 0 vs leg 0 / 50).
        let arm_phase = state.arm_phase;
        let (l_sh, l_el) = arm_joint_angles(arm_phase, 50);
        let (r_sh, r_el) = arm_joint_angles(arm_phase, 0);
        draw_arm(display, shoulder, l_sh, l_el, dir, color)?;
        draw_arm(display, shoulder, r_sh, r_el, dir, color)?;
    }

    Ok(())
}

/// Standing one-handed sword. `stab` 0 = ready; 100 = horizontal blade + lunge.
fn draw_sword_stance<D>(
    display: &mut D,
    hip: Point,
    shoulder: Point,
    dir: i32,
    stab: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let t = stab.clamp(0, 100);

    // Front leg steps forward with the torso lunge; rear leg trails behind.
    draw_leg(display, hip, 10 + t * 28 / 100, 10 + t * 10 / 100, dir, color)?;
    draw_leg(display, hip, -8 - t * 18 / 100, 12 + t * 8 / 100, dir, color)?;

    draw_sword_arms_and_weapon(display, hip, shoulder, dir, t, color)
}

/// Crouched one-handed sword in a kneel: forward knee raised, rear knee down.
/// Stab slides the lead foot forward from that base.
fn draw_sword_crouch<D>(
    display: &mut D,
    hip: Point,
    shoulder: Point,
    dir: i32,
    stab: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let t = stab.clamp(0, 100);

    // Front: thigh forward with knee up, shin down to the planted foot.
    draw_leg(
        display,
        hip,
        58 + t * 22 / 100,
        72 + t * 6 / 100,
        dir,
        color,
    )?;
    // Rear: kneel on the trailing knee, shin folded under.
    draw_leg(
        display,
        hip,
        -28 - t * 10 / 100,
        98,
        dir,
        color,
    )?;

    draw_sword_arms_and_weapon(display, hip, shoulder, dir, t, color)
}

/// Shared sword-arm pose: bent ready → straight horizontal stab; free hand at waist front.
fn draw_sword_arms_and_weapon<D>(
    display: &mut D,
    hip: Point,
    shoulder: Point,
    dir: i32,
    t: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Ready: upper arm down-forward (elbow low), forearm up to the grip.
    // Stab: upper arm rises to match the forearm — arm straightens horizontal.
    let sh = 32 + t * 58 / 100;
    let el = 82 + t * 8 / 100;
    let sword_elbow = joint_offset(shoulder, sh, UPPER_ARM_LEN, dir);
    let fist = joint_offset(sword_elbow, el, FOREARM_LEN, dir);
    draw_body_line(display, shoulder, sword_elbow, color)?;
    draw_body_line(display, sword_elbow, fist, color)?;

    // Free hand held at waist, in front of the torso.
    let free_elbow = joint_offset(shoulder, 18, UPPER_ARM_LEN, dir);
    let free_hand = Point::new(hip.x + dir * 8, hip.y);
    draw_body_line(display, shoulder, free_elbow, color)?;
    draw_body_line(display, free_elbow, free_hand, color)?;

    draw_one_hand_sword(display, fist, dir, t, color)
}

/// Stick-style sword: fist ball, crossguard beside it, blade (angled up → horizontal as `t`→100).
fn draw_one_hand_sword<D>(
    display: &mut D,
    fist: Point,
    dir: i32,
    t: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Ready tip drifts up; full stab is horizontal through the fist.
    let tip = Point::new(fist.x + dir * 24, fist.y - 7 + t * 7 / 100);
    let pommel = Point::new(fist.x - dir * 4, fist.y + 3 - t * 3 / 100);

    draw_body_line(display, pommel, tip, color)?;

    // Crossguard just ahead of the fist, slightly below the blade line.
    let guard = Point::new(fist.x + dir * 3, fist.y + 2);
    let guard_a = Point::new(guard.x - dir * 1, guard.y - 5);
    let guard_b = Point::new(guard.x + dir * 1, guard.y + 5);
    draw_body_line(display, guard_a, guard_b, color)?;

    // Fist as a small ball on the grip.
    let style = PrimitiveStyle::with_stroke(color, 1);
    Circle::with_center(fist, 4)
        .into_styled(style)
        .draw(display)?;
    Ok(())
}

fn joint_offset(origin: Point, angle_deg: i32, length: i32, dir: i32) -> Point {
    let (s, c) = sin_cos_deg_milli(angle_deg);
    Point::new(
        origin.x + dir * s * length / 1000,
        origin.y + c * length / 1000,
    )
}

/// Front-facing tucked spin (knockback).
fn draw_knockback<D>(
    display: &mut D,
    state: &StickmanState,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let origin = (state.x, state.y - 28);
    // Spin matches reverse travel: facing right → move left → CCW.
    let roll = if state.facing_left {
        state.roll_deg
    } else {
        -state.roll_deg
    };
    let map = |lx: i32, ly: i32| {
        let (x, y) = rotate_point_cw((origin.0 + lx, origin.1 + ly), origin, roll);
        Point::new(x, y)
    };

    let head = map(0, -20);
    let hip = map(0, 10);
    let neck = map(0, -8);
    let knee_a = map(10, 18);
    let knee_b = map(-8, 16);
    let foot_a = map(6, 26);
    let foot_b = map(-4, 24);
    let hand_a = map(14, 0);
    let hand_b = map(-12, 2);
    let elbow_a = map(10, -4);
    let elbow_b = map(-8, -2);

    let head_style = PrimitiveStyle::with_stroke(color, HEAD_STROKE);
    Circle::with_center(head, 10)
        .into_styled(head_style)
        .draw(display)?;
    draw_body_line(display, neck, hip, color)?;
    draw_body_line(display, hip, knee_a, color)?;
    draw_body_line(display, knee_a, foot_a, color)?;
    draw_body_line(display, hip, knee_b, color)?;
    draw_body_line(display, knee_b, foot_b, color)?;
    draw_body_line(display, neck, elbow_a, color)?;
    draw_body_line(display, elbow_a, hand_a, color)?;
    draw_body_line(display, neck, elbow_b, color)?;
    draw_body_line(display, elbow_b, hand_b, color)?;
    Ok(())
}

/// Side-profile crouch-ball roll ("tumbleweed"): crouch silhouette spinning forward.
fn draw_side_tumble<D>(
    display: &mut D,
    state: &StickmanState,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Side-view crouch on a ~22px ball. Local +X = travel / chest.
    // Spin about the pose centroid so CoG height stays constant.
    //
    //        (head)
    //          |
    //   hand--neck
    //    |     |
    //   knee  hip
    //    |
    //   foot
    const LOCAL: &[(i32, i32)] = &[
        (0, -18), // head
        (0, -8),  // neck
        (0, 8),   // hip
        (11, 3),  // knee
        (8, 14),  // foot
        (8, -5),  // elbow
        (11, 3),  // hand
        (9, 5),   // back knee
        (5, 12),  // back foot
    ];
    // Head circle carries more visual mass than a limb joint — weight it so
    // the spin pivot matches the drawn CoG.
    const HEAD_WEIGHT: i32 = 3;
    let n = (LOCAL.len() as i32 - 1) + HEAD_WEIGHT;
    let (sx, sy) = LOCAL.iter().enumerate().fold((0i32, 0i32), |(ax, ay), (i, &(x, y))| {
        let w = if i == 0 { HEAD_WEIGHT } else { 1 };
        (ax + x * w, ay + y * w)
    });
    let cx = (sx + n / 2) / n;
    let cy = (sy + n / 2) / n;

    // Slightly lower than the ball midpoint so the roll sits closer to the floor.
    let origin = (state.x, state.y - 18);
    let facing = if state.facing_left { -1 } else { 1 };
    // Clockwise when facing right; counter-clockwise when facing left.
    let roll = if state.facing_left {
        -state.roll_deg
    } else {
        state.roll_deg
    };
    let map = |lx: i32, ly: i32| {
        let (x, y) = rotate_point_cw(
            (origin.0 + facing * (lx - cx), origin.1 + (ly - cy)),
            origin,
            roll,
        );
        Point::new(x, y)
    };

    let head = map(LOCAL[0].0, LOCAL[0].1);
    let neck = map(LOCAL[1].0, LOCAL[1].1);
    let hip = map(LOCAL[2].0, LOCAL[2].1);
    let knee = map(LOCAL[3].0, LOCAL[3].1);
    let foot = map(LOCAL[4].0, LOCAL[4].1);
    let elbow = map(LOCAL[5].0, LOCAL[5].1);
    let hand = map(LOCAL[6].0, LOCAL[6].1);

    let head_style = PrimitiveStyle::with_stroke(color, HEAD_STROKE);
    Circle::with_center(head, 10)
        .into_styled(head_style)
        .draw(display)?;
    draw_body_line(display, neck, hip, color)?;
    // One profile leg curled under — matches crouch silhouette.
    draw_body_line(display, hip, knee, color)?;
    draw_body_line(display, knee, foot, color)?;
    // One profile arm reaching the knee.
    draw_body_line(display, neck, elbow, color)?;
    draw_body_line(display, elbow, hand, color)?;
    // Light second limbs tucked behind (offset, shorter) for a bit of volume.
    draw_body_line(display, hip, map(LOCAL[7].0, LOCAL[7].1), color)?;
    draw_body_line(
        display,
        map(LOCAL[7].0, LOCAL[7].1),
        map(LOCAL[8].0, LOCAL[8].1),
        color,
    )?;
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
    // Offset perpendicular to the dominant axis so vertical/horizontal limbs
    // always read as two solid pixels (eg stroke width can look thinner).
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

fn draw_leg<D>(
    display: &mut D,
    hip: Point,
    hip_angle: i32,
    knee_bend: i32,
    dir: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let knee = joint_offset(hip, hip_angle, THIGH_LEN, dir);
    // Knee flexion folds the shin back relative to the thigh.
    let shin_angle = hip_angle - knee_bend;
    let foot = joint_offset(knee, shin_angle, SHIN_LEN, dir);
    draw_body_line(display, hip, knee, color)?;
    draw_body_line(display, knee, foot, color)?;
    Ok(())
}

fn draw_arm<D>(
    display: &mut D,
    shoulder: Point,
    shoulder_angle: i32,
    elbow_bend: i32,
    dir: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let elbow = joint_offset(shoulder, shoulder_angle, UPPER_ARM_LEN, dir);
    // Slight elbow crook so the forearm is not a stiff continuation.
    let forearm_angle = shoulder_angle + elbow_bend;
    let hand = joint_offset(elbow, forearm_angle, FOREARM_LEN, dir);
    draw_body_line(display, shoulder, elbow, color)?;
    draw_body_line(display, elbow, hand, color)?;
    Ok(())
}
