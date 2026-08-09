//! Jumping behavior — loops a parabolic hop.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

/// Full hop duration (floor → apex → floor).
const JUMP_PERIOD_MS: u32 = 750;

pub struct JumpingBehavior {
    elapsed_ms: u32,
}

impl JumpingBehavior {
    pub fn new() -> Self {
        Self { elapsed_ms: 0 }
    }
}

impl Behavior for JumpingBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::Jumping
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;
        let floor = geometry::floor_y();
        let apex = geometry::jump_apex_foot_y(ctx.display_height as i32);
        let rise = (floor - apex).max(1);

        self.elapsed_ms = self.elapsed_ms.saturating_add(ctx.delta_ms as u32) % JUMP_PERIOD_MS;
        // t in 0..1000 over the hop; parabola peaks at t=500.
        let t = self.elapsed_ms * 1000 / JUMP_PERIOD_MS;
        let height = rise * 4 * t as i32 * (1000 - t as i32) / (1000 * 1000);

        s.y = floor - height;
        s.crouch = 0;
        s.begging = false;
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
