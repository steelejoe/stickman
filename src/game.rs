//! Shared game state and update/draw loop (device + simulation).

use crate::assets::{self, Rgb565Image};
use crate::behavior::plugin::BehaviorManager;
use crate::dirty::{self, DIRTY_BUF_LEN};
use crate::layer;
use crate::stickman::geometry::StickmanState;
use crate::DISPLAY_WIDTH;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

/// Action selected by a screen tap's horizontal position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TapAction {
    FaceLeft,
    FaceRight,
    Random,
}

/// Map a display X coordinate to a left / center / right tap action.
pub fn tap_action_for_x(x: u32, width: u32) -> TapAction {
    let third = width / 3;
    if x < third {
        TapAction::FaceLeft
    } else if x < third * 2 {
        TapAction::Random
    } else {
        TapAction::FaceRight
    }
}

/// Platform-independent stickman game.
pub struct Game {
    behavior_mgr: BehaviorManager,
    stickman_state: StickmanState,
    /// Previous pose, used to erase before redrawing (avoids full-screen clears).
    prev_state: Option<StickmanState>,
    /// Layer 0 has been painted; later frames only dirty-restore under the figure.
    background_drawn: bool,
    /// Optional layer-0 backdrop (`'static` — embedded or leaked at startup).
    background: Option<Rgb565Image<'static>>,
    /// Scratch tile for flicker-free dirty presents (composed in RAM, one blit).
    dirty_buf: [Rgb565; DIRTY_BUF_LEN],
}

impl Game {
    pub fn new() -> Self {
        Self {
            behavior_mgr: BehaviorManager::new(StickmanState::default()),
            stickman_state: StickmanState::default(),
            prev_state: None,
            background_drawn: false,
            background: assets::embedded_background(),
            dirty_buf: [Rgb565::BLACK; DIRTY_BUF_LEN],
        }
    }

    /// Install a layer-0 backdrop (replaces any embedded background).
    pub fn set_background(&mut self, image: Rgb565Image<'static>) {
        self.background = Some(image);
        self.background_drawn = false;
        self.prev_state = None;
    }

    /// True when a backdrop image is installed.
    pub fn has_background_image(&self) -> bool {
        self.background.is_some()
    }

    /// Cycle to the next behavior (device BOOT button / sim Space).
    pub fn on_cycle_input(&mut self) {
        self.behavior_mgr.cycle_next();
    }

    /// Handle a positioned tap: left/right thirds change facing; center picks a
    /// random other behavior.
    pub fn on_tap(&mut self, x: u32) {
        match tap_action_for_x(x, DISPLAY_WIDTH) {
            TapAction::FaceLeft => self.stickman_state.facing_left = true,
            TapAction::FaceRight => self.stickman_state.facing_left = false,
            TapAction::Random => self.behavior_mgr.cycle_random(x),
        }
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
        self.background_drawn && self.prev_state.as_ref() == Some(&self.stickman_state)
    }

    /// Draw the current frame if the pose changed.
    ///
    /// After the initial layer-0 paint, stickman updates are composed into a
    /// dirty tile in RAM and pushed with one `fill_contiguous` — avoiding the
    /// erase hole that caused flicker on the device QSPI panel.
    pub fn draw<D>(&mut self, display: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        if self.is_frame_static() {
            return Ok(());
        }

        if !self.background_drawn {
            layer::draw_background(display, self.background.as_ref())?;
            self.background_drawn = true;
        }

        let prev = self.prev_state.take();
        dirty::present_stickman_frame(
            display,
            &mut self.dirty_buf,
            prev.as_ref(),
            &self.stickman_state,
            self.background.as_ref(),
        )?;
        self.prev_state = Some(self.stickman_state.clone());
        Ok(())
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_center_is_random_action() {
        assert_eq!(tap_action_for_x(0, 536), TapAction::FaceLeft);
        assert_eq!(tap_action_for_x(268, 536), TapAction::Random);
        assert_eq!(tap_action_for_x(535, 536), TapAction::FaceRight);
    }
}
