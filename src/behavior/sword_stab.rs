//! Sword stab — from sword stance, thrust horizontal with a front-leg slide, then recover.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

/// Full stab cycle: extend then recover.
const STAB_PERIOD_MS: u32 = 900;

pub struct SwordStabBehavior {
    elapsed_ms: u32,
}

impl SwordStabBehavior {
    pub fn new() -> Self {
        Self { elapsed_ms: 0 }
    }
}

impl Behavior for SwordStabBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::SwordStab
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;

        self.elapsed_ms = self
            .elapsed_ms
            .saturating_add(ctx.delta_ms as u32)
            % STAB_PERIOD_MS;

        // Triangle: 0 → 100 → 0 over the period.
        let half = STAB_PERIOD_MS / 2;
        let stab = if self.elapsed_ms < half {
            self.elapsed_ms * 100 / half
        } else {
            100 - (self.elapsed_ms - half) * 100 / half
        };

        s.y = geometry::floor_y();
        s.crouch = 0;
        s.begging = false;
        s.sword_stance = true;
        s.sword_stab = stab.min(100) as u8;
        s.roll_deg = 0;
        s.roll_mode = geometry::RollMode::None;
        s.leg_phase = 0;
        s.arm_phase = 0;
        None
    }

    fn draw<D>(&self, display: &mut D, state: &StickmanState) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        render::draw_stickman(display, state)
    }
}
