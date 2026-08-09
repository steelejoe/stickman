//! Sword stance — standing one-handed sword ready pose.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

pub struct SwordStanceBehavior;

impl SwordStanceBehavior {
    pub fn new() -> Self {
        Self
    }
}

impl Behavior for SwordStanceBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::SwordStance
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;
        s.y = geometry::floor_y();
        s.crouch = 0;
        s.begging = false;
        s.sword_stance = true;
        s.sword_stab = 0;
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
