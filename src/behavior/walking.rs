//! Walking back and forth behavior.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

/// Walk-cycle phase units per second (100 = one full gait cycle).
/// ≈3 units per 33ms frame — smooth cadence without per-frame phase jumps.
const PHASE_RATE: u32 = 90;

pub struct WalkingBehavior {
    /// Remainder for `PHASE_RATE * delta_ms / 1000`.
    phase_rem: u32,
    /// Remainder for `stride * dphase / 100`.
    travel_rem: i32,
}

impl WalkingBehavior {
    pub fn new() -> Self {
        Self {
            phase_rem: 0,
            travel_rem: 0,
        }
    }
}

impl Behavior for WalkingBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::Walking
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;
        let w = ctx.display_width as i32;
        let delta = ctx.delta_ms as u32;

        // Advance walk cycle with fixed-point remainder so small frames still accumulate.
        let phase_num = PHASE_RATE * delta + self.phase_rem;
        let dphase = phase_num / 1000;
        self.phase_rem = phase_num % 1000;
        s.leg_phase = (s.leg_phase + dphase) % 100;
        s.arm_phase = s.leg_phase;

        // Travel from gait: stride length × fraction of cycle advanced.
        let stride = geometry::stride_length_px();
        let travel_num = stride * dphase as i32 + self.travel_rem;
        let steps = travel_num / 100;
        self.travel_rem = travel_num % 100;
        let dx = if s.facing_left { -steps } else { steps };
        s.x += dx;
        s.y = geometry::floor_y();

        // Bounce at edges (with margin for stickman width)
        let margin = 40;
        if s.x <= margin {
            s.x = margin;
            s.facing_left = false;
        } else if s.x >= w - margin {
            s.x = w - margin;
            s.facing_left = true;
        }

        None
    }

    fn draw<D>(&self, display: &mut D, state: &StickmanState) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        render::draw_stickman(display, state)
    }
}
