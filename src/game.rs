//! Shared game state and update/draw loop (device + simulation).

use crate::assets::{self, Rgb565Image};
use crate::behavior::box_beh::BoxBrain;
use crate::behavior::event::{Event, EventCtx};
use crate::behavior::plugin::BehaviorManager;
use crate::collision::{self, apply_gravity, support_y, ContactMemory, World, LAND_SLOP};
use crate::dirty::{self, DIRTY_BUF_LEN};
use crate::stickman::eval;
use crate::stickman::geometry::floor_y;
use crate::stickman::ir::{Actor, ClipId, PoseScratch};
use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;
use embedded_graphics::primitives::Rectangle;

/// Platform-independent stickman game.
pub struct Game {
    behavior_mgr: BehaviorManager,
    box_brain: BoxBrain,
    actor: Actor,
    /// Crate on the same layer and walk baseline as [`Self::actor`].
    box_actor: Actor,
    prev_actor: Option<Actor>,
    prev_box: Option<Actor>,
    prev_rect: Option<Rectangle>,
    prev_box_rect: Option<Rectangle>,
    scratch: PoseScratch,
    box_scratch: PoseScratch,
    /// Layer 0 has been painted; later frames only dirty-restore under figures.
    background_drawn: bool,
    /// Optional layer-0 backdrop (`'static` — embedded or leaked at startup).
    background: Option<Rgb565Image<'static>>,
    /// Scratch tile for flicker-free dirty presents (composed in RAM, one blit).
    dirty_buf: [Rgb565; DIRTY_BUF_LEN],
    contacts: ContactMemory,
    /// Last tick the stickman was on a support (floor or model top).
    stick_grounded: bool,
}

impl Game {
    pub fn new() -> Self {
        let mut box_actor = Actor::default();
        box_actor.play(ClipId::BoxIdle);
        // Right of spawn so the walker meets it; y/layer stay at floor / middle.
        box_actor.x = (DISPLAY_WIDTH as i32) * 3 / 4;
        Self {
            behavior_mgr: BehaviorManager::new(),
            box_brain: BoxBrain::new(),
            actor: Actor::default(),
            box_actor,
            prev_actor: None,
            prev_box: None,
            prev_rect: None,
            prev_box_rect: None,
            scratch: PoseScratch::new(),
            box_scratch: PoseScratch::new(),
            background_drawn: false,
            background: assets::embedded_background(),
            dirty_buf: [Rgb565::BLACK; DIRTY_BUF_LEN],
            contacts: ContactMemory::new(),
            stick_grounded: true,
        }
    }

    /// Install a layer-0 backdrop (replaces any embedded background).
    pub fn set_background(&mut self, image: Rgb565Image<'static>) {
        self.background = Some(image);
        self.background_drawn = false;
        self.prev_actor = None;
        self.prev_box = None;
        self.prev_rect = None;
        self.prev_box_rect = None;
    }

    /// True when a backdrop image is installed.
    pub fn has_background_image(&self) -> bool {
        self.background.is_some()
    }

    /// Cycle to the next stickman behavior (device BOOT button / sim Space).
    pub fn on_cycle_input(&mut self) {
        self.behavior_mgr.cycle_next(&mut self.actor);
    }

    /// Hit-test tap: the entity under the point rolls its tap table.
    /// Empty space: 15% flip facing, otherwise a random other stickman behavior.
    pub fn on_tap(&mut self, x: u32, y: u32) {
        eval::sample(&self.actor, &mut self.scratch);
        eval::sample(&self.box_actor, &mut self.box_scratch);
        let p = Point::new(x as i32, y as i32);
        let on_stick = collision::contains_point(eval::hitbox(&self.scratch), p);
        let on_box = collision::contains_point(eval::hitbox(&self.box_scratch), p);
        let entropy = x ^ y.wrapping_shl(16);
        if on_stick {
            self.behavior_mgr
                .on_event(&mut self.actor, Event::Tap, EventCtx::default(), entropy);
        } else if on_box {
            self.box_brain.on_event(
                &mut self.box_actor,
                Event::Tap,
                EventCtx::default(),
                entropy,
            );
        } else {
            self.behavior_mgr.cycle_empty_tap(&mut self.actor, entropy);
        }
    }

    pub fn update(&mut self, delta_ms: u64) {
        self.sample_poses();
        let floor = floor_y();
        let box_hit = eval::hitbox(&self.box_scratch);
        let support0 = support_y(self.actor.x, self.actor.y, &[box_hit], floor);
        let jumping = self.behavior_mgr.in_jump_arc();
        let airborne = jumping || self.actor.y + LAND_SLOP < support0;
        let prev_y = self.actor.y;

        let stick_fin =
            self.behavior_mgr
                .update_loco(delta_ms, &mut self.actor, support0, airborne);
        let box_fin = self.box_brain.update(delta_ms, &mut self.box_actor);

        self.sample_poses();
        let box_hit = eval::hitbox(&self.box_scratch);
        let support = support_y(self.actor.x, self.actor.y, &[box_hit], floor);
        let dt = delta_ms as u32;

        let mut landed_jump = false;
        let mut started_falling = false;
        if jumping {
            let rising = self.actor.y < prev_y;
            let on_raised = !rising
                && support + LAND_SLOP < floor
                && self.actor.y + LAND_SLOP >= support
                && support + LAND_SLOP < self.behavior_mgr.jump_takeoff_y();
            if on_raised {
                self.actor.y = support;
                self.actor.vy = 0;
                self.stick_grounded = true;
                landed_jump = true;
            } else {
                self.actor.vy = 0;
                self.stick_grounded = false;
            }
        } else if self.actor.y + LAND_SLOP < support {
            started_falling = self.stick_grounded;
            self.actor.y = apply_gravity(self.actor.y, &mut self.actor.vy, support, dt);
            self.stick_grounded = self.actor.y >= support;
        } else {
            self.actor.y = support;
            self.actor.vy = 0;
            self.stick_grounded = true;
        }

        let jump_ended_air =
            jumping && stick_fin && !landed_jump && self.actor.y + LAND_SLOP < support;

        let stick_chained = if stick_fin || landed_jump {
            self.behavior_mgr.on_event(
                &mut self.actor,
                Event::BehaviorFinished,
                EventCtx::default(),
                delta_ms as u32,
            )
        } else {
            false
        };
        if box_fin {
            self.box_brain.on_event(
                &mut self.box_actor,
                Event::BehaviorFinished,
                EventCtx::default(),
                delta_ms as u32,
            );
        }

        self.sample_poses();
        let hit_a = eval::hitbox(&self.scratch);
        let hit_b = eval::hitbox(&self.box_scratch);
        let hits = collision::resolve(
            &mut [(&mut self.actor, hit_a), (&mut self.box_actor, hit_b)],
            &mut self.contacts,
            World {
                width: DISPLAY_WIDTH as i32,
                height: DISPLAY_HEIGHT as i32,
                baseline_y: floor,
            },
        );

        let entropy = delta_ms as u32;
        let ctx_stick = EventCtx {
            collision: hits.kind[0],
            other_x: Some(self.box_actor.x),
            other_facing_left: Some(self.box_actor.facing_left),
        };
        let ctx_box = EventCtx {
            collision: hits.kind[1],
            other_x: Some(self.actor.x),
            other_facing_left: Some(self.actor.facing_left),
        };
        let ctx_model = EventCtx {
            collision: Some(collision::CollisionKind::Model),
            other_x: Some(self.actor.x),
            other_facing_left: Some(self.actor.facing_left),
        };
        let ctx_model_from_box = EventCtx {
            collision: Some(collision::CollisionKind::Model),
            other_x: Some(self.box_actor.x),
            other_facing_left: Some(self.box_actor.facing_left),
        };
        if started_falling || jump_ended_air {
            self.behavior_mgr.on_event(
                &mut self.actor,
                Event::Falling,
                EventCtx::default(),
                entropy ^ 0xF,
            );
        }
        if hits.body_entered(0) {
            self.behavior_mgr
                .on_event(&mut self.actor, Event::Collision, ctx_stick, entropy ^ 0xA);
        }
        if hits.body_entered(1) {
            self.box_brain.on_event(
                &mut self.box_actor,
                Event::Collision,
                ctx_box,
                entropy ^ 0xB,
            );
        } else if (stick_fin || box_fin) && !hits.model_enter && self.contacts.models_overlap(0, 1)
        {
            if !stick_chained {
                self.behavior_mgr.on_event(
                    &mut self.actor,
                    Event::Collision,
                    ctx_model_from_box,
                    entropy ^ 0xC,
                );
            }
            self.box_brain.on_event(
                &mut self.box_actor,
                Event::Collision,
                ctx_model,
                entropy ^ 0xD,
            );
        }
    }

    fn sample_poses(&mut self) {
        eval::sample(&self.actor, &mut self.scratch);
        eval::sample(&self.box_actor, &mut self.box_scratch);
    }

    /// True when the displayed poses already match the current actors.
    pub fn is_frame_static(&self) -> bool {
        self.background_drawn
            && self.prev_actor.as_ref() == Some(&self.actor)
            && self.prev_box.as_ref() == Some(&self.box_actor)
    }

    /// Draw the current frame if a pose changed.
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

        self.sample_poses();
        let new_stick = eval::dirty_rect(&self.scratch);
        let new_box = eval::dirty_rect(&self.box_scratch);
        let stick_changed = self.prev_actor.as_ref() != Some(&self.actor);
        let box_changed = self.prev_box.as_ref() != Some(&self.box_actor);

        if stick_changed && box_changed {
            let area = union_optional(
                union_optional(self.prev_rect, Some(new_stick)),
                union_optional(self.prev_box_rect, Some(new_box)),
            );
            if let Some(area) = area {
                if area.size.width <= dirty::DIRTY_MAX_W && area.size.height <= dirty::DIRTY_MAX_H {
                    dirty::blit_composed_area(
                        display,
                        &mut self.dirty_buf,
                        area,
                        &self.scratch,
                        &[&self.box_scratch],
                        self.background.as_ref(),
                        true,
                    )?;
                } else {
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_box_rect,
                        &self.box_scratch,
                        new_box,
                        &[&self.scratch],
                        self.background.as_ref(),
                    )?;
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_rect,
                        &self.scratch,
                        new_stick,
                        &[&self.box_scratch],
                        self.background.as_ref(),
                    )?;
                }
            }
        } else if box_changed {
            dirty::present_actor_frame(
                display,
                &mut self.dirty_buf,
                self.prev_box_rect,
                &self.box_scratch,
                new_box,
                &[&self.scratch],
                self.background.as_ref(),
            )?;
        } else {
            dirty::present_actor_frame(
                display,
                &mut self.dirty_buf,
                self.prev_rect,
                &self.scratch,
                new_stick,
                &[&self.box_scratch],
                self.background.as_ref(),
            )?;
        }

        self.prev_rect = Some(new_stick);
        self.prev_box_rect = Some(new_box);
        self.prev_actor = Some(self.actor);
        self.prev_box = Some(self.box_actor);
        Ok(())
    }
}

fn union_optional(a: Option<Rectangle>, b: Option<Rectangle>) -> Option<Rectangle> {
    match (a, b) {
        (None, None) => None,
        (Some(r), None) | (None, Some(r)) => Some(r),
        (Some(a), Some(b)) => Some(union_rects(a, b)),
    }
}

fn union_rects(a: Rectangle, b: Rectangle) -> Rectangle {
    if a.size.width == 0 || a.size.height == 0 {
        return b;
    }
    if b.size.width == 0 || b.size.height == 0 {
        return a;
    }
    let x0 = a.top_left.x.min(b.top_left.x);
    let y0 = a.top_left.y.min(b.top_left.y);
    let x1 = (a.top_left.x + a.size.width as i32).max(b.top_left.x + b.size.width as i32);
    let y1 = (a.top_left.y + a.size.height as i32).max(b.top_left.y + b.size.height as i32);
    Rectangle::new(
        Point::new(x0, y0),
        embedded_graphics::geometry::Size::new((x1 - x0) as u32, (y1 - y0) as u32),
    )
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::box_beh::BoxBehaviorId;
    use crate::stickman::geometry::floor_y;
    use crate::stickman::library;

    #[test]
    fn box_shares_stickman_layer_and_baseline() {
        let game = Game::new();
        assert_eq!(game.box_actor.layer, game.actor.layer);
        assert_eq!(game.box_actor.y, game.actor.y);
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
        assert_ne!(game.box_actor.x, game.actor.x);
        assert_eq!(game.box_brain.current(), BoxBehaviorId::Idle);
    }

    #[test]
    fn tap_empty_space_picks_other_stickman_behavior() {
        let mut game = Game::new();
        let clip = game.actor.clip;
        game.on_tap(1, 1);
        assert_ne!(game.actor.clip, clip);
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
    }

    #[test]
    fn tap_on_box_does_not_cycle_stickman() {
        let mut game = Game::new();
        game.actor.facing_left = true;
        let stick_clip = game.actor.clip;
        eval::sample(&game.box_actor, &mut game.box_scratch);
        let hit = eval::hitbox(&game.box_scratch);
        let x = hit.top_left.x as u32 + hit.size.width / 2;
        let y = hit.top_left.y as u32 + hit.size.height / 2;
        game.on_tap(x, y);
        assert_eq!(game.actor.clip, stick_clip);
        assert!(game.actor.facing_left);
        assert!(matches!(
            game.box_actor.clip,
            ClipId::BoxIdle | ClipId::BoxSlide | ClipId::BoxRoll | ClipId::BoxShudder
        ));
    }

    #[test]
    fn update_overlap_keeps_box_species() {
        let mut game = Game::new();
        game.actor.x = game.box_actor.x;
        game.update(0);
        let clip = library::clip(game.box_actor.clip);
        assert!(core::ptr::eq(clip.species, &library::BOX));
    }

    #[test]
    fn looping_walk_reports_finished_after_one_cycle() {
        let mut actor = Actor::default();
        actor.play(ClipId::Walk);
        let d = library::clip(ClipId::Walk).duration_ms as u32;
        assert!(!actor.advance(d - 1));
        assert!(actor.advance(1));
    }

    #[test]
    fn held_idle_never_reports_finished() {
        let mut actor = Actor::default();
        actor.play(ClipId::Idle);
        assert!(!actor.advance(1000));
        assert_eq!(actor.time_ms, 0);
    }

    #[test]
    fn jump_forward_lands_on_box_top() {
        let mut game = Game::new();
        let floor = floor_y();
        let top = floor - library::BOX_HEIGHT as i32;
        game.actor.x = game.box_actor.x - 24;
        game.actor.facing_left = false;
        game.on_cycle_input();
        game.on_cycle_input();
        game.on_cycle_input();
        assert_eq!(game.actor.clip, ClipId::JumpForward);
        for _ in 0..48 {
            game.update(33);
            if game.actor.y == top
                && game.actor.x >= game.box_actor.x - 20
                && game.actor.x < game.box_actor.x + 20
            {
                break;
            }
        }
        assert_eq!(game.actor.y, top);
        assert!(
            (game.actor.x - game.box_actor.x).abs() < library::BOX_WIDTH as i32,
            "x={} box={}",
            game.actor.x,
            game.box_actor.x
        );
    }

    #[test]
    fn walking_off_box_falls_to_floor() {
        let mut game = Game::new();
        let floor = floor_y();
        let top = floor - library::BOX_HEIGHT as i32;
        game.actor.x = game.box_actor.x;
        game.actor.y = top;
        game.stick_grounded = true;
        game.actor.facing_left = false;
        let start_y = game.actor.y;
        for _ in 0..48 {
            game.update(33);
            if game.actor.y >= floor {
                break;
            }
        }
        assert!(game.actor.y > start_y);
        assert_eq!(game.actor.y, floor);
        assert_eq!(game.box_actor.y, floor);
    }
}
