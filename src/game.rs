//! Shared game state and update/draw loop (device + simulation).

use crate::behavior::plugin::BehaviorManager;
use crate::stickman::geometry::StickmanState;
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

/// Platform-independent stickman game.
pub struct Game {
    behavior_mgr: BehaviorManager,
    stickman_state: StickmanState,
    /// Previous pose, used to erase before redrawing (avoids full-screen clears).
    prev_state: Option<StickmanState>,
    floor_drawn: bool,
}

impl Game {
    pub fn new() -> Self {
        Self {
            behavior_mgr: BehaviorManager::new(StickmanState::default()),
            stickman_state: StickmanState::default(),
            prev_state: None,
            floor_drawn: false,
        }
    }

    /// Cycle to the next behavior (device button / touch, or sim input).
    pub fn on_cycle_input(&mut self) {
        self.behavior_mgr.cycle_next();
    }

    pub fn update(&mut self, delta_ms: u64) {
        self.behavior_mgr
            .update(delta_ms, &mut self.stickman_state);
    }

    /// True when the displayed pose already matches the current state.
    ///
    /// When animation is paused (e.g. idle), the AMOLED can keep showing the
    /// last image with no further QSPI traffic.
    pub fn is_frame_static(&self) -> bool {
        self.floor_drawn && self.prev_state.as_ref() == Some(&self.stickman_state)
    }

    /// Draw the current frame if the pose changed.
    ///
    /// On device, prefer calling this without a full-screen clear: the previous
    /// stickman is erased in black, then the new pose is drawn. Static frames
    /// are a no-op so paused animation does not redraw.
    pub fn draw<D>(&mut self, display: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        if self.is_frame_static() {
            return Ok(());
        }

        if !self.floor_drawn {
            render::draw_floor(display)?;
            self.floor_drawn = true;
        }

        if let Some(prev) = self.prev_state.take() {
            // Erase the previous pose. Floor pixels under the feet may need a redraw.
            render::draw_stickman_colored(display, &prev, Rgb565::BLACK)?;
            render::draw_floor(display)?;
        }

        self.behavior_mgr.draw(display, &self.stickman_state)?;
        self.prev_state = Some(self.stickman_state.clone());
        Ok(())
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
