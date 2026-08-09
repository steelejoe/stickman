//! Searching behavior — crouched, glancing left↔right twice per cycle.

use super::plugin::{Behavior, BehaviorId, UpdateContext};
use crate::stickman::geometry::{self, StickmanState};
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

/// Pause between facing changes.
const FACE_PAUSE_MS: u32 = 500;

/// Facing steps for one search: left → right → left → right.
const FACE_STEPS: u32 = 4;

pub struct SearchingBehavior {
    elapsed_ms: u32,
}

impl SearchingBehavior {
    pub fn new() -> Self {
        Self { elapsed_ms: 0 }
    }
}

impl Behavior for SearchingBehavior {
    fn id(&self) -> BehaviorId {
        BehaviorId::Searching
    }

    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId> {
        let s = &mut *ctx.stickman_state;

        self.elapsed_ms = self
            .elapsed_ms
            .saturating_add(ctx.delta_ms as u32)
            % (FACE_PAUSE_MS * FACE_STEPS);

        // Even steps face left; odd steps face right — two left→right switches.
        let step = self.elapsed_ms / FACE_PAUSE_MS;
        s.facing_left = step % 2 == 0;

        s.y = geometry::floor_y();
        s.crouch = 100;
        s.begging = false;
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
