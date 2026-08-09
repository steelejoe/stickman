//! Stick figure joint and limb geometry.

use crate::layer::LayerId;

/// Pixels above the bottom of the display for the floor line.
const FLOOR_MARGIN: i32 = 18;

/// Leg segment lengths (pixels); shared by FK draw and stride length.
pub const THIGH_LEN: i32 = 15;
pub const SHIN_LEN: i32 = 13;

/// Standing pose: head-center distance above the feet (`y`).
pub const HEAD_CENTER_ABOVE_FEET: i32 = 58;

/// Y coordinate of the floor (feet contact line).
pub fn floor_y() -> i32 {
    crate::DISPLAY_HEIGHT as i32 - FLOOR_MARGIN
}

/// Feet `y` so the standing head center sits just above the screen midline.
pub fn jump_apex_foot_y(display_height: i32) -> i32 {
    let head_target = display_height / 2 - 6;
    head_target + HEAD_CENTER_ABOVE_FEET
}

/// How a non-zero [`StickmanState::roll_deg`] should be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RollMode {
    /// Upright / ignore roll.
    #[default]
    None,
    /// Front-facing spinning tuck (knockback).
    Knockback,
    /// Side-profile forward flip with limbs folded to the torso.
    Tumbling,
}

/// Stick figure animation/model state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickmanState {
    /// X position (center of figure)
    pub x: i32,
    /// Y position (feet / floor contact level)
    pub y: i32,
    /// Facing left (true) or right (false)
    pub facing_left: bool,
    /// Walk cycle phase 0..2*PI (approximated as 0..100 for fixed-point)
    pub leg_phase: u32,
    /// Arm swing phase (0..100)
    pub arm_phase: u32,
    /// Crouch amount 0..=100 (0 = stand, 100 = full crouch).
    pub crouch: u8,
    /// When crouched: arms reach forward (begging) vs hang by the sides.
    pub begging: bool,
    /// Standing one-handed sword ready stance.
    pub sword_stance: bool,
    /// Sword stab extension 0..=100 (0 = ready stance, 100 = full horizontal thrust).
    pub sword_stab: u8,
    /// Body roll in degrees (0..360) for knockback / tumbling.
    pub roll_deg: i32,
    /// Selects knockback vs side-profile tumble rendering.
    pub roll_mode: RollMode,
    /// Depth layer: [`LayerId::Middle`] or [`LayerId::Foreground`].
    pub layer: LayerId,
}

impl Default for StickmanState {
    fn default() -> Self {
        Self {
            x: (crate::DISPLAY_WIDTH / 2) as i32,
            y: floor_y(),
            facing_left: false,
            leg_phase: 0,
            arm_phase: 0,
            crouch: 0,
            begging: false,
            sword_stance: false,
            sword_stab: 0,
            roll_deg: 0,
            roll_mode: RollMode::None,
            layer: LayerId::Middle,
        }
    }
}

/// Approximate sin(phase) for phase in 0..100 ≡ 0..2π.
/// Returns milli-units in [-1000, 1000].
pub fn phase_sin_milli(phase: u32) -> i32 {
    // Quarter-wave: index 0..25 → sin(0)..sin(π/2)
    const Q: [i32; 26] = [
        0, 63, 125, 187, 248, 309, 368, 425, 482, 535, 588, 637, 685, 729, 771, 809, 844, 876,
        905, 929, 951, 968, 982, 992, 998, 1000,
    ];
    let p = phase % 100;
    match p {
        0..=25 => Q[p as usize],
        26..=50 => Q[(50 - p) as usize],
        51..=75 => -Q[(p - 50) as usize],
        _ => -Q[(100 - p) as usize],
    }
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
    let s = sin_deg_milli(deg);
    let c = sin_deg_milli(deg + 90);
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
        0, 87, 174, 259, 342, 423, 500, 574, 643, 707, 766, 819, 866, 906, 940, 966, 985, 996,
        1000,
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

/// Hip / knee angles (degrees from vertical) for one leg.
/// `phase_offset` is 0 for one leg, 50 for the opposite.
pub fn leg_joint_angles(phase: u32, phase_offset: u32) -> (i32, i32) {
    let p = (phase + phase_offset) % 100;
    let swing = phase_sin_milli(p);
    let hip = swing * 32 / 1000;

    // Stance (~0..50): nearly straight. Swing (~50..100): bend then extend.
    let knee = if p < 50 {
        8 + (25 - (p as i32 - 25).abs()) * 6 / 25
    } else {
        let t = (p - 50) as i32;
        12 + (25 - (t - 25).abs()) * 48 / 25
    };
    (hip, knee)
}

/// Horizontal foot offset relative to the hip (facing right, pixels).
pub fn foot_x_rel(phase: u32, phase_offset: u32) -> i32 {
    let (hip, knee) = leg_joint_angles(phase, phase_offset);
    let shin = hip - knee;
    let (hs, _) = sin_cos_deg_milli(hip);
    let (ss, _) = sin_cos_deg_milli(shin);
    hs * THIGH_LEN / 1000 + ss * SHIN_LEN / 1000
}

/// Body travel (pixels) for one full gait cycle (phase wrap 0..100).
/// Two steps; step length ≈ peak left/right foot separation.
pub fn stride_length_px() -> i32 {
    // Separation peaks near phase 25 (and 75 with legs swapped).
    let step = (foot_x_rel(25, 0) - foot_x_rel(25, 50)).abs();
    2 * step
}

/// Shoulder / elbow angles (degrees from vertical) for one arm.
/// Use offset 50 relative to the same-side leg so arms swing contralaterally.
pub fn arm_joint_angles(phase: u32, phase_offset: u32) -> (i32, i32) {
    let p = (phase + phase_offset) % 100;
    let swing = phase_sin_milli(p);
    let shoulder = swing * 28 / 1000;
    let elbow = 18 + swing.unsigned_abs() as i32 * 14 / 1000;
    (shoulder, elbow)
}
