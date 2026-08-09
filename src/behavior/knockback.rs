//! Knockback — front-facing spinning tuck, traveling opposite facing at walk pace.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, RollMode, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

/// Same cadence as [`super::walking::WalkingBehavior`].
const PHASE_RATE: u32 = 90;

pub struct KnockbackBehavior {
    phase_rem: u32,
    travel_rem: i32,
}

impl KnockbackBehavior {
    pub fn new() -> Self {
        Self {
            phase_rem: 0,
            travel_rem: 0,
        }
    }
}

impl Behavior for KnockbackBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::Knockback
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;
        let w = ctx.display_width as i32;
        let delta = ctx.delta_ms as u32;

        let phase_num = PHASE_RATE * delta + self.phase_rem;
        let dphase = phase_num / 1000;
        self.phase_rem = phase_num % 1000;
        s.leg_phase = (s.leg_phase + dphase) % 100;
        s.arm_phase = s.leg_phase;

        let stride = geometry::stride_length_px();
        let travel_num = stride * dphase as i32 + self.travel_rem;
        let steps = travel_num / 100;
        self.travel_rem = travel_num % 100;
        // Travel opposite facing (knocked backward).
        let dx = if s.facing_left { steps } else { -steps };
        s.x += dx;
        s.y = geometry::floor_y();
        s.crouch = 0;
        s.begging = false;
        s.sword_stance = false;
        s.sword_stab = 0;
        s.roll_mode = RollMode::Knockback;
        s.roll_deg = (s.leg_phase as i32 * 360) / 100;

        let margin = 40;
        if s.x <= margin {
            s.x = margin;
            // Face left → reverse travel goes right, away from the wall.
            s.facing_left = true;
        } else if s.x >= w - margin {
            s.x = w - margin;
            s.facing_left = false;
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
