//! Shared game state and update/draw loop (device + simulation).

use crate::assets::{self, Rgb565Image};
use crate::behavior::plugin::BehaviorManager;
use crate::collision::{self, ContactMemory, World};
use crate::dirty::{self, DIRTY_BUF_LEN};
use crate::stickman::eval;
use crate::stickman::geometry::floor_y;
use crate::stickman::ir::{Actor, ClipId, PoseScratch};
use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;
use embedded_graphics::primitives::Rectangle;

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
    actor: Actor,
    /// Static crate on the same layer and walk baseline as [`Self::actor`].
    box_actor: Actor,
    prev_actor: Option<Actor>,
    prev_rect: Option<Rectangle>,
    scratch: PoseScratch,
    box_scratch: PoseScratch,
    /// Layer 0 has been painted; later frames only dirty-restore under the figure.
    background_drawn: bool,
    /// Crate has been presented once (redrawn when a dirty tile overlaps it).
    box_drawn: bool,
    /// Optional layer-0 backdrop (`'static` — embedded or leaked at startup).
    background: Option<Rgb565Image<'static>>,
    /// Scratch tile for flicker-free dirty presents (composed in RAM, one blit).
    dirty_buf: [Rgb565; DIRTY_BUF_LEN],
    contacts: ContactMemory,
}

impl Game {
    pub fn new() -> Self {
        let mut box_actor = Actor::default();
        box_actor.play(ClipId::BoxIdle);
        // Right of spawn so the walker meets it; y/layer stay at floor / middle.
        box_actor.x = (DISPLAY_WIDTH as i32) * 3 / 4;
        Self {
            behavior_mgr: BehaviorManager::new(),
            actor: Actor::default(),
            box_actor,
            prev_actor: None,
            prev_rect: None,
            scratch: PoseScratch::new(),
            box_scratch: PoseScratch::new(),
            background_drawn: false,
            box_drawn: false,
            background: assets::embedded_background(),
            dirty_buf: [Rgb565::BLACK; DIRTY_BUF_LEN],
            contacts: ContactMemory::new(),
        }
    }

    /// Install a layer-0 backdrop (replaces any embedded background).
    pub fn set_background(&mut self, image: Rgb565Image<'static>) {
        self.background = Some(image);
        self.background_drawn = false;
        self.box_drawn = false;
        self.prev_actor = None;
        self.prev_rect = None;
    }

    /// True when a backdrop image is installed.
    pub fn has_background_image(&self) -> bool {
        self.background.is_some()
    }

    /// Cycle to the next behavior (device BOOT button / sim Space).
    pub fn on_cycle_input(&mut self) {
        self.behavior_mgr.cycle_next(&mut self.actor);
    }

    /// Handle a positioned tap: left/right thirds change facing; center picks a
    /// random other behavior.
    pub fn on_tap(&mut self, x: u32) {
        match tap_action_for_x(x, DISPLAY_WIDTH) {
            TapAction::FaceLeft => self.actor.facing_left = true,
            TapAction::FaceRight => self.actor.facing_left = false,
            TapAction::Random => self.behavior_mgr.cycle_random(&mut self.actor, x),
        }
    }

    pub fn update(&mut self, delta_ms: u64) {
        self.behavior_mgr.update(delta_ms, &mut self.actor);
        eval::sample(&self.actor, &mut self.scratch);
        eval::sample(&self.box_actor, &mut self.box_scratch);
        let hit_a = eval::hitbox(&self.scratch);
        let hit_b = eval::hitbox(&self.box_scratch);
        collision::resolve(
            &mut [(&mut self.actor, hit_a), (&mut self.box_actor, hit_b)],
            &mut self.contacts,
            World {
                width: DISPLAY_WIDTH as i32,
                height: DISPLAY_HEIGHT as i32,
                baseline_y: floor_y(),
            },
        );
    }

    /// True when the displayed pose already matches the current actor.
    ///
    /// When animation is paused (e.g. idle), the AMOLED can keep showing the
    /// last image with no further QSPI traffic.
    pub fn is_frame_static(&self) -> bool {
        self.background_drawn && self.box_drawn && self.prev_actor.as_ref() == Some(&self.actor)
    }

    /// Draw the current frame if the pose changed.
    ///
    /// After the initial layer-0 paint, updates are composed into a dirty tile
    /// in RAM and pushed with one `fill_contiguous`.
    pub fn draw<D>(&mut self, display: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        if self.is_frame_static() {
            return Ok(());
        }

        if !self.background_drawn {
            dirty::draw_background(display, self.background.as_ref())?;
            self.background_drawn = true;
        }

        eval::sample(&self.actor, &mut self.scratch);
        eval::sample(&self.box_actor, &mut self.box_scratch);

        if !self.box_drawn {
            let box_rect = eval::dirty_rect(&self.box_scratch);
            dirty::present_actor_frame(
                display,
                &mut self.dirty_buf,
                None,
                &self.box_scratch,
                box_rect,
                &[],
                self.background.as_ref(),
            )?;
            self.box_drawn = true;
        }

        let new_rect = eval::dirty_rect(&self.scratch);
        let prev_rect = self.prev_rect.take();
        dirty::present_actor_frame(
            display,
            &mut self.dirty_buf,
            prev_rect,
            &self.scratch,
            new_rect,
            &[&self.box_scratch],
            self.background.as_ref(),
        )?;
        self.prev_rect = Some(new_rect);
        self.prev_actor = Some(self.actor);
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

    #[test]
    fn box_shares_stickman_layer_and_baseline() {
        let game = Game::new();
        assert_eq!(game.box_actor.layer, game.actor.layer);
        assert_eq!(game.box_actor.y, game.actor.y);
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
        assert_ne!(game.box_actor.x, game.actor.x);
    }

    #[test]
    fn update_flips_facing_when_models_overlap() {
        let mut game = Game::new();
        game.actor.x = game.box_actor.x;
        game.actor.facing_left = false;
        game.box_actor.facing_left = false;
        game.update(0);
        assert!(game.actor.facing_left);
        assert!(game.box_actor.facing_left);
    }
}
