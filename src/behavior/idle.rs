//! Idle (standing still) behavior.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::StickmanState;
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

pub struct IdleBehavior;

impl IdleBehavior {
    pub fn new() -> Self {
        Self
    }
}

impl Behavior for IdleBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::Idle
    }

    fn update(&mut self, _ctx: &mut UpdateContext) -> Option<BehaviorId> {
        // No movement, no phase change
        None
    }

    fn draw<D>(&self, display: &mut D, state: &StickmanState) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        render::draw_stickman(display, state)
    }
}
