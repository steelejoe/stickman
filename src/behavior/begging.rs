//! Begging behavior — bent-knee crouch with arms reaching forward.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

pub struct BeggingBehavior;

impl BeggingBehavior {
    pub fn new() -> Self {
        Self
    }
}

impl Behavior for BeggingBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::Begging
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;
        s.y = geometry::floor_y();
        s.crouch = 100;
        s.begging = true;
        s.sword_stance = false;
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
