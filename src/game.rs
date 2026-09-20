//! Shared game state and update/draw loop (device + simulation).

use crate::assets::{self, Backdrop, Rgb565Image};
use crate::behavior::box_beh::BoxBrain;
use crate::config::ConfigCmd;
use crate::behavior::event::{Event, EventCtx, Rng32};
use crate::behavior::plugin::{BehaviorId, BehaviorManager};
use crate::collision::{
    self, align_to_support, apply_gravity, on_floor_polyline, CollisionKind, ContactMemory, World,
};
use crate::dirty::{self, DIRTY_BUF_LEN};
use crate::room::{self, RoomId};
use crate::stickman::eval;
use crate::stickman::geometry::floor_y_at;
use crate::stickman::ir::{Actor, ClipId, PoseScratch};
use crate::stickman::library;
use crate::DISPLAY_WIDTH;

/// Crate parked in a room the stickman is not standing in.
#[derive(Clone, Copy)]
struct StashedBox {
    actor: Actor,
    brain: BoxBrain,
    grounded: bool,
}
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;
use embedded_graphics::primitives::Rectangle;

/// Platform-independent stickman game.
pub struct Game {
    behavior_mgr: BehaviorManager,
    box_brain: BoxBrain,
    dog_brain: BehaviorManager,
    actor: Actor,
    /// Crate on the same layer and floor edge as [`Self::actor`].
    box_actor: Actor,
    /// Quadruped on the left flat, same layer as the stickman and crate.
    dog_actor: Actor,
    prev_actor: Option<Actor>,
    prev_box: Option<Actor>,
    prev_dog: Option<Actor>,
    prev_rect: Option<Rectangle>,
    prev_box_rect: Option<Rectangle>,
    prev_dog_rect: Option<Rectangle>,
    scratch: PoseScratch,
    box_scratch: PoseScratch,
    dog_scratch: PoseScratch,
    /// Layer 0 has been painted; later frames only dirty-restore under figures.
    background_drawn: bool,
    /// Optional layer-0 image for [`RoomId::Home`] (`'static` — embedded or leaked).
    home_image: Option<Rgb565Image<'static>>,
    /// Website/USB color override for home (wins over [`Self::home_image`]).
    home_color: Option<Rgb565>,
    current_room: RoomId,
    /// Crate of each room the stickman has left (live crate stays on the fields).
    stashed_boxes: [Option<StashedBox>; room::ROOM_COUNT],
    /// Scratch tile for flicker-free dirty presents (composed in RAM, one blit).
    dirty_buf: [Rgb565; DIRTY_BUF_LEN],
    contacts: ContactMemory,
    /// Last tick the stickman was on a supporting edge (floor or model top).
    stick_grounded: bool,
    /// Last tick the box was on a supporting edge.
    box_grounded: bool,
    /// Last tick the dog was on a supporting edge.
    dog_grounded: bool,
    /// Previous stickman behavior (bubble overlay is not stored on [`Actor`]).
    prev_behavior: Option<BehaviorId>,
    /// Previous crate behavior (same reason as [`Self::prev_behavior`]).
    prev_box_behavior: Option<crate::behavior::box_beh::BoxBehaviorId>,
    /// Previous dog behavior (same reason as [`Self::prev_behavior`]).
    prev_dog_behavior: Option<BehaviorId>,
}

impl Game {
    pub fn new() -> Self {
        let mut box_actor = Actor::default();
        box_actor.play(ClipId::BoxIdle);
        // Right of spawn, on the flat wing past the middle-third bump.
        box_actor.x = (DISPLAY_WIDTH as i32) * 3 / 4;
        box_actor.y = floor_y_at(box_actor.x);
        let mut dog_actor = Actor::default();
        dog_actor.play(ClipId::DogWalk);
        // Left flat wing, facing the stickman.
        dog_actor.x = (DISPLAY_WIDTH as i32) / 4;
        dog_actor.y = floor_y_at(dog_actor.x);
        Self {
            behavior_mgr: BehaviorManager::new(),
            box_brain: BoxBrain::new(),
            dog_brain: BehaviorManager::dog(),
            actor: Actor::default(),
            box_actor,
            dog_actor,
            prev_actor: None,
            prev_box: None,
            prev_dog: None,
            prev_rect: None,
            prev_box_rect: None,
            prev_dog_rect: None,
            scratch: PoseScratch::new(),
            box_scratch: PoseScratch::new(),
            dog_scratch: PoseScratch::new(),
            background_drawn: false,
            home_image: assets::embedded_background(),
            home_color: None,
            current_room: RoomId::Home,
            stashed_boxes: [None; room::ROOM_COUNT],
            dirty_buf: [Rgb565::BLACK; DIRTY_BUF_LEN],
            contacts: ContactMemory::new(),
            stick_grounded: true,
            box_grounded: true,
            dog_grounded: true,
            prev_behavior: None,
            prev_box_behavior: None,
            prev_dog_behavior: None,
        }
    }

    /// Install a layer-0 image for the home room (replaces any embedded background).
    pub fn set_background(&mut self, image: Rgb565Image<'static>) {
        self.home_image = Some(image);
        self.invalidate_frame();
    }

    /// True when a home-room backdrop image is installed.
    pub fn has_background_image(&self) -> bool {
        self.home_image.is_some()
    }

    /// Room the stickman is in.
    pub fn current_room(&self) -> RoomId {
        self.current_room
    }

    fn backdrop(&self) -> Backdrop {
        if self.current_room == RoomId::Home {
            if let Some(color) = self.home_color {
                return Backdrop::Color(color);
            }
        }
        self.current_room.backdrop(self.home_image)
    }

    /// Apply a live setting from the Wi-Fi website (core 0 only).
    pub fn apply_config(&mut self, cmd: ConfigCmd) {
        match cmd {
            ConfigCmd::SetBackdropColor { r, g, b } => {
                self.home_color = Some(crate::config::rgb888_to_565(r, g, b));
            }
            ConfigCmd::ClearBackdropColor => {
                self.home_color = None;
            }
        }
        self.invalidate_frame();
    }

    fn dog_in_room(&self) -> bool {
        self.current_room == RoomId::Home
    }

    fn invalidate_frame(&mut self) {
        self.background_drawn = false;
        self.prev_actor = None;
        self.prev_box = None;
        self.prev_dog = None;
        self.prev_rect = None;
        self.prev_box_rect = None;
        self.prev_dog_rect = None;
        self.prev_behavior = None;
        self.prev_box_behavior = None;
        self.prev_dog_behavior = None;
    }

    /// Cycle to the next stickman behavior (device BOOT button / sim Space).
    pub fn on_cycle_input(&mut self) {
        self.behavior_mgr.cycle_next(&mut self.actor);
    }

    /// Menu taps only. Room taps do nothing. Returns true for **config** (enable Wi-Fi).
    pub fn on_tap(&mut self, x: u32, y: u32) -> bool {
        match crate::menu::hit_button(x, y) {
            Some(crate::menu::MenuButton::Config) => true,
            Some(crate::menu::MenuButton::Box) => {
                self.tap_box();
                false
            }
            Some(crate::menu::MenuButton::Dog) => {
                self.tap_dog();
                false
            }
            Some(crate::menu::MenuButton::Man) => {
                self.tap_man();
                false
            }
            None => false,
        }
    }

    fn tap_entropy(&self, salt: u32) -> u32 {
        salt ^ (self.actor.x as u32).wrapping_mul(0x45D9_F3B)
            ^ self.actor.time_ms
            ^ (self.box_actor.x as u32)
            ^ self.dog_actor.time_ms
    }

    fn tap_box(&mut self) {
        let entropy = self.tap_entropy(0xB0);
        self.box_brain.on_event(
            &mut self.box_actor,
            Event::Tap,
            EventCtx::default(),
            entropy,
        );
    }

    fn tap_dog(&mut self) {
        let entropy = self.tap_entropy(0xD0);
        self.dog_brain.on_event(
            &mut self.dog_actor,
            Event::Tap,
            EventCtx::default(),
            entropy,
        );
    }

    fn tap_man(&mut self) {
        let entropy = self.tap_entropy(0xA1);
        self.behavior_mgr
            .on_event(&mut self.actor, Event::Tap, EventCtx::default(), entropy);
    }

    pub fn update(&mut self, delta_ms: u64) {
        let dt = delta_ms as u32;
        let dog_here = self.dog_in_room();
        let was_stick = self.stick_grounded;
        let was_dog = self.dog_grounded;
        let stick_jumping = self.behavior_mgr.in_jump_arc();
        let dog_jumping = dog_here && self.dog_brain.in_jump_arc();
        let world = World::display();

        let stick_fin = self.behavior_mgr.update(delta_ms, &mut self.actor);
        let box_fin = self.box_brain.update(delta_ms, &mut self.box_actor);
        let dog_fin = if dog_here {
            self.dog_brain.update(delta_ms, &mut self.dog_actor)
        } else {
            false
        };

        if was_stick && !stick_jumping && on_floor_polyline(&self.actor, world) {
            align_to_support(&mut self.actor, world);
        }
        if self.box_grounded && on_floor_polyline(&self.box_actor, world) {
            align_to_support(&mut self.box_actor, world);
        }
        if dog_here && was_dog && !dog_jumping && on_floor_polyline(&self.dog_actor, world) {
            align_to_support(&mut self.dog_actor, world);
        }

        self.actor.integrate(dt);
        self.box_actor.integrate(dt);
        if dog_here {
            self.dog_actor.integrate(dt);
        }

        self.sample_poses();
        let hit_a = eval::hitbox(&self.scratch);
        let hit_b = eval::hitbox(&self.box_scratch);
        let stick_vx = self.actor.vx;
        let hits = if dog_here {
            let hit_c = eval::hitbox(&self.dog_scratch);
            collision::resolve(
                &mut [
                    (&mut self.actor, hit_a),
                    (&mut self.box_actor, hit_b),
                    (&mut self.dog_actor, hit_c),
                ],
                &mut self.contacts,
                world,
            )
        } else {
            collision::resolve(
                &mut [(&mut self.actor, hit_a), (&mut self.box_actor, hit_b)],
                &mut self.contacts,
                world,
            )
        };
        self.stick_grounded = hits.is_grounded(0);
        self.box_grounded = hits.is_grounded(1);
        if dog_here {
            self.dog_grounded = hits.is_grounded(2);
        }
        if !hits.is_grounded(0) {
            apply_gravity(&mut self.actor.vy, dt);
        }
        if !hits.is_grounded(1) {
            apply_gravity(&mut self.box_actor.vy, dt);
        }
        if dog_here && !hits.is_grounded(2) {
            apply_gravity(&mut self.dog_actor.vy, dt);
        }

        if let Some(kind) = hits.kind[0] {
            if let Some(next) = self.current_room.neighbor(kind) {
                self.enter_room(next, kind, stick_vx);
                return;
            }
        }

        let stick_landed = stick_jumping && self.stick_grounded && !was_stick;
        let stick_air_end = stick_jumping && stick_fin && !self.stick_grounded;
        let stick_falling =
            was_stick && !self.stick_grounded && self.actor.vy >= 0 && !stick_jumping;
        let dog_landed = dog_here && dog_jumping && self.dog_grounded && !was_dog;
        let dog_air_end = dog_here && dog_jumping && dog_fin && !self.dog_grounded;
        let dog_falling =
            dog_here && was_dog && !self.dog_grounded && self.dog_actor.vy >= 0 && !dog_jumping;

        let stick_chained = if stick_landed || (stick_fin && !stick_air_end) {
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
        let dog_chained = if dog_here && (dog_landed || (dog_fin && !dog_air_end)) {
            self.dog_brain.on_event(
                &mut self.dog_actor,
                Event::BehaviorFinished,
                EventCtx::default(),
                delta_ms as u32,
            )
        } else {
            false
        };

        let entropy = delta_ms as u32;
        let xs = [self.actor.x, self.box_actor.x, self.dog_actor.x];
        let faces = [
            self.actor.facing_left,
            self.box_actor.facing_left,
            self.dog_actor.facing_left,
        ];
        if stick_falling || stick_air_end {
            self.behavior_mgr.on_event(
                &mut self.actor,
                Event::Falling,
                EventCtx::default(),
                entropy ^ 0xF,
            );
        }
        if dog_here && (dog_falling || dog_air_end) {
            self.dog_brain.on_event(
                &mut self.dog_actor,
                Event::Falling,
                EventCtx::default(),
                entropy ^ 0xE,
            );
        }
        if hits.body_entered(0) {
            let ctx = hit_ctx(&hits, 0, &self.contacts, &xs, &faces);
            self.behavior_mgr
                .on_event(&mut self.actor, Event::Collision, ctx, entropy ^ 0xA);
        }
        if hits.body_entered(1) {
            let ctx = hit_ctx(&hits, 1, &self.contacts, &xs, &faces);
            self.box_brain
                .on_event(&mut self.box_actor, Event::Collision, ctx, entropy ^ 0xB);
        }
        if dog_here && hits.body_entered(2) {
            let ctx = hit_ctx(&hits, 2, &self.contacts, &xs, &faces);
            self.dog_brain
                .on_event(&mut self.dog_actor, Event::Collision, ctx, entropy ^ 0x9);
        }
        if !hits.model_enter {
            self.overlap_after_finish(
                0,
                1,
                stick_fin || box_fin,
                stick_chained,
                false,
                &hits,
                &xs,
                &faces,
                entropy,
            );
            if dog_here {
                self.overlap_after_finish(
                    0,
                    2,
                    stick_fin || dog_fin,
                    stick_chained,
                    dog_chained,
                    &hits,
                    &xs,
                    &faces,
                    entropy ^ 0x11,
                );
                self.overlap_after_finish(
                    1,
                    2,
                    box_fin || dog_fin,
                    false,
                    dog_chained,
                    &hits,
                    &xs,
                    &faces,
                    entropy ^ 0x22,
                );
            }
        }
    }

    fn enter_room(&mut self, next: RoomId, from_edge: CollisionKind, vx: i32) {
        let old = self.current_room;
        self.stashed_boxes[old.index()] = Some(StashedBox {
            actor: self.box_actor,
            brain: self.box_brain,
            grounded: self.box_grounded,
        });
        self.current_room = next;
        match self.stashed_boxes[next.index()].take() {
            Some(stashed) => {
                self.box_actor = stashed.actor;
                self.box_brain = stashed.brain;
                self.box_grounded = stashed.grounded;
            }
            None => self.spawn_room_box(),
        }
        self.place_stickman_entering(from_edge, vx);
        self.contacts = ContactMemory::new();
        self.invalidate_frame();
    }

    fn spawn_room_box(&mut self) {
        let seed = 0xB100_0001
            ^ (self.current_room.index() as u32).wrapping_mul(0x9E37_79B9)
            ^ (self.actor.x as u32);
        self.box_brain = BoxBrain::with_seed(seed);
        let mut rng = Rng32::new(seed ^ 0x51ED);
        rng.mix(self.actor.y as u32);
        let x = room::random_floor_x(&mut rng, library::BOX_WIDTH as i32);
        let mut box_actor = Actor::default();
        box_actor.play(ClipId::BoxIdle);
        box_actor.x = x;
        box_actor.y = floor_y_at(x);
        self.box_actor = box_actor;
        self.box_grounded = true;
    }

    fn place_stickman_entering(&mut self, from_edge: CollisionKind, vx: i32) {
        const INSET: i32 = 28;
        match from_edge {
            CollisionKind::EdgeRight => {
                self.actor.x = crate::menu::ROOM_LEFT + INSET;
                self.actor.facing_left = false;
            }
            CollisionKind::EdgeLeft => {
                self.actor.x = DISPLAY_WIDTH as i32 - INSET;
                self.actor.facing_left = true;
            }
            _ => {}
        }
        self.actor.apply_clip_velocity();
        if self.actor.vx == 0 && vx != 0 {
            self.actor.vx = if self.actor.facing_left {
                -vx.abs()
            } else {
                vx.abs()
            };
        }
        if self.stick_grounded {
            self.actor.y = floor_y_at(self.actor.x);
            self.actor.vy = 0;
        } else {
            let fy = floor_y_at(self.actor.x);
            if self.actor.y > fy {
                self.actor.y = fy;
                self.actor.vy = 0;
                self.stick_grounded = true;
            }
        }
        self.actor.clear_remainder(true, true);

        eval::sample(&self.actor, &mut self.scratch);
        let hit = eval::hitbox(&self.scratch);
        let left = hit.top_left.x;
        let right = hit.top_left.x + hit.size.width as i32;
        if left < crate::menu::ROOM_LEFT + 1 {
            self.actor.x += crate::menu::ROOM_LEFT + 1 - left;
        }
        if right > DISPLAY_WIDTH as i32 - 1 {
            self.actor.x -= right - (DISPLAY_WIDTH as i32 - 1);
        }
    }

    fn overlap_after_finish(
        &mut self,
        i: usize,
        j: usize,
        finished: bool,
        chained_i: bool,
        chained_j: bool,
        hits: &collision::CollisionHits,
        xs: &[i32; 3],
        faces: &[bool; 3],
        entropy: u32,
    ) {
        if !finished || !self.contacts.models_overlap(i, j) {
            return;
        }
        if !chained_i {
            self.dispatch_collision(i, model_ctx(hits, i, j, xs, faces), entropy ^ 0xC);
        }
        if !chained_j {
            self.dispatch_collision(j, model_ctx(hits, j, i, xs, faces), entropy ^ 0xD);
        }
    }

    fn dispatch_collision(&mut self, i: usize, ctx: EventCtx, entropy: u32) {
        match i {
            0 => {
                self.behavior_mgr
                    .on_event(&mut self.actor, Event::Collision, ctx, entropy);
            }
            1 => self
                .box_brain
                .on_event(&mut self.box_actor, Event::Collision, ctx, entropy),
            _ => {
                self.dog_brain
                    .on_event(&mut self.dog_actor, Event::Collision, ctx, entropy);
            }
        }
    }

    fn sample_poses(&mut self) {
        eval::sample(&self.actor, &mut self.scratch);
        eval::sample(&self.box_actor, &mut self.box_scratch);
        if self.dog_in_room() {
            eval::sample(&self.dog_actor, &mut self.dog_scratch);
        }
        if let Some(text) = self.behavior_mgr.talk_phrase() {
            self.scratch.bubble = crate::stickman::bubble::for_pose(
                &self.scratch,
                text,
                self.behavior_mgr.bubble_left(),
            );
        }
        if let Some(text) = self.box_brain.talk_phrase() {
            self.box_scratch.bubble = crate::stickman::bubble::for_pose(
                &self.box_scratch,
                text,
                self.box_brain.bubble_left(),
            );
        }
        if self.dog_in_room() {
            if let Some(text) = self.dog_brain.talk_phrase() {
                self.dog_scratch.bubble = crate::stickman::bubble::for_pose(
                    &self.dog_scratch,
                    text,
                    self.dog_brain.bubble_left(),
                );
            }
        }
    }

    /// True when the displayed poses already match the current actors.
    pub fn is_frame_static(&self) -> bool {
        self.background_drawn
            && self.prev_actor.as_ref() == Some(&self.actor)
            && self.prev_box.as_ref() == Some(&self.box_actor)
            && self.prev_behavior == Some(self.behavior_mgr.current())
            && self.prev_box_behavior == Some(self.box_brain.current())
            && (!self.dog_in_room()
                || (self.prev_dog.as_ref() == Some(&self.dog_actor)
                    && self.prev_dog_behavior == Some(self.dog_brain.current())))
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
            dirty::draw_background(display, self.backdrop())?;
            self.background_drawn = true;
        }

        self.sample_poses();
        let dog_here = self.dog_in_room();
        let new_stick = eval::dirty_rect(&self.scratch);
        let new_box = eval::dirty_rect(&self.box_scratch);
        let new_dog = if dog_here {
            Some(eval::dirty_rect(&self.dog_scratch))
        } else {
            None
        };
        let stick_changed = self.prev_actor.as_ref() != Some(&self.actor)
            || self.prev_behavior != Some(self.behavior_mgr.current());
        let box_changed = self.prev_box.as_ref() != Some(&self.box_actor)
            || self.prev_box_behavior != Some(self.box_brain.current());
        let dog_changed = dog_here
            && (self.prev_dog.as_ref() != Some(&self.dog_actor)
                || self.prev_dog_behavior != Some(self.dog_brain.current()));
        let bg = self.backdrop();

        let mut area = None;
        if stick_changed {
            area = union_optional(area, union_optional(self.prev_rect, Some(new_stick)));
        }
        if box_changed {
            area = union_optional(area, union_optional(self.prev_box_rect, Some(new_box)));
        }
        if dog_changed {
            area = union_optional(area, union_optional(self.prev_dog_rect, new_dog));
        }

        if let Some(area) = area {
            if area.size.width <= dirty::DIRTY_MAX_W && area.size.height <= dirty::DIRTY_MAX_H {
                if dog_here {
                    dirty::blit_composed_area(
                        display,
                        &mut self.dirty_buf,
                        area,
                        &self.scratch,
                        &[&self.box_scratch, &self.dog_scratch],
                        bg,
                        true,
                    )?;
                } else {
                    dirty::blit_composed_area(
                        display,
                        &mut self.dirty_buf,
                        area,
                        &self.scratch,
                        &[&self.box_scratch],
                        bg,
                        true,
                    )?;
                }
            } else if dog_here {
                if stick_changed {
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_rect,
                        &self.scratch,
                        new_stick,
                        &[&self.box_scratch, &self.dog_scratch],
                        bg,
                    )?;
                }
                if box_changed {
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_box_rect,
                        &self.box_scratch,
                        new_box,
                        &[&self.scratch, &self.dog_scratch],
                        bg,
                    )?;
                }
                if dog_changed {
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_dog_rect,
                        &self.dog_scratch,
                        new_dog.unwrap_or(new_stick),
                        &[&self.scratch, &self.box_scratch],
                        bg,
                    )?;
                }
            } else {
                if stick_changed {
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_rect,
                        &self.scratch,
                        new_stick,
                        &[&self.box_scratch],
                        bg,
                    )?;
                }
                if box_changed {
                    dirty::present_actor_frame(
                        display,
                        &mut self.dirty_buf,
                        self.prev_box_rect,
                        &self.box_scratch,
                        new_box,
                        &[&self.scratch],
                        bg,
                    )?;
                }
            }
        }

        self.prev_rect = Some(new_stick);
        self.prev_box_rect = Some(new_box);
        self.prev_dog_rect = new_dog;
        self.prev_actor = Some(self.actor);
        self.prev_box = Some(self.box_actor);
        if dog_here {
            self.prev_dog = Some(self.dog_actor);
            self.prev_dog_behavior = Some(self.dog_brain.current());
        }
        self.prev_behavior = Some(self.behavior_mgr.current());
        self.prev_box_behavior = Some(self.box_brain.current());
        Ok(())
    }
}

fn closest_partner(mem: &ContactMemory, i: usize, xs: &[i32; 3]) -> Option<usize> {
    let mut best: Option<(i32, usize)> = None;
    for j in 0..3 {
        if j == i || !mem.models_overlap(i, j) {
            continue;
        }
        let dist = (xs[i] - xs[j]).abs();
        if best.map(|(d, _)| dist < d).unwrap_or(true) {
            best = Some((dist, j));
        }
    }
    best.map(|(_, j)| j)
}

fn hit_ctx(
    hits: &collision::CollisionHits,
    i: usize,
    mem: &ContactMemory,
    xs: &[i32; 3],
    faces: &[bool; 3],
) -> EventCtx {
    match closest_partner(mem, i, xs) {
        Some(j) => EventCtx {
            collision: hits.kind[i],
            other_x: Some(xs[j]),
            other_facing_left: Some(faces[j]),
            nx: hits.nx[i],
            ny: hits.ny[i],
        },
        None => EventCtx {
            collision: hits.kind[i],
            nx: hits.nx[i],
            ny: hits.ny[i],
            ..EventCtx::default()
        },
    }
}

fn model_ctx(
    hits: &collision::CollisionHits,
    i: usize,
    other: usize,
    xs: &[i32; 3],
    faces: &[bool; 3],
) -> EventCtx {
    EventCtx {
        collision: Some(collision::CollisionKind::Model),
        other_x: Some(xs[other]),
        other_facing_left: Some(faces[other]),
        nx: hits.nx[i],
        ny: hits.ny[i],
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
    use crate::stickman::geometry::{floor_y, floor_y_at};
    use crate::stickman::library;

    #[test]
    fn box_shares_stickman_layer_and_floor() {
        let game = Game::new();
        assert_eq!(game.box_actor.layer, game.actor.layer);
        assert_eq!(game.actor.y, floor_y_at(game.actor.x));
        assert_eq!(game.box_actor.y, floor_y_at(game.box_actor.x));
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
        assert_ne!(game.box_actor.x, game.actor.x);
        assert_eq!(game.box_brain.current(), BoxBehaviorId::Idle);
        // Spawn is the bump peak; the crate sits on the right flat.
        assert!(game.actor.y < game.box_actor.y);
    }

    #[test]
    fn dog_shares_layer_and_left_flat() {
        let game = Game::new();
        assert_eq!(game.dog_actor.layer, game.actor.layer);
        assert_eq!(game.dog_actor.y, floor_y_at(game.dog_actor.x));
        assert_eq!(game.dog_actor.clip, ClipId::DogWalk);
        assert!(game.dog_actor.x < game.actor.x);
        assert!(game.actor.x < game.box_actor.x);
        assert_eq!(
            game.dog_brain.kind(),
            crate::behavior::plugin::FigureKind::Dog
        );
        assert!(core::ptr::eq(
            library::clip(game.dog_actor.clip).species,
            &library::DOG
        ));
        assert!(game.actor.y < game.dog_actor.y);
    }

    fn menu_xy(button: crate::menu::MenuButton) -> (u32, u32) {
        let r = crate::menu::button_rect(button.index()).expect("button");
        (
            r.top_left.x as u32 + r.size.width / 2,
            r.top_left.y as u32 + r.size.height / 2,
        )
    }

    #[test]
    fn room_tap_does_nothing() {
        let mut game = Game::new();
        let stick = game.actor.clip;
        let box_clip = game.box_actor.clip;
        let dog = game.dog_actor.clip;
        eval::sample(&game.box_actor, &mut game.box_scratch);
        let hit = eval::hitbox(&game.box_scratch);
        let x = hit.top_left.x as u32 + hit.size.width / 2;
        let y = hit.top_left.y as u32 + hit.size.height / 2;
        assert!(!game.on_tap(x, y));
        assert!(!game.on_tap(crate::menu::ROOM_LEFT as u32 + 10, 1));
        assert_eq!(game.actor.clip, stick);
        assert_eq!(game.box_actor.clip, box_clip);
        assert_eq!(game.dog_actor.clip, dog);
    }

    #[test]
    fn menu_config_requests_wifi_without_tapping_entities() {
        let mut game = Game::new();
        let stick = game.actor.clip;
        let (x, y) = menu_xy(crate::menu::MenuButton::Config);
        assert!(game.on_tap(x, y));
        assert_eq!(game.actor.clip, stick);
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
        assert_eq!(game.dog_actor.clip, ClipId::DogWalk);
    }

    #[test]
    fn menu_box_taps_the_crate() {
        let mut game = Game::new();
        let stick = game.actor.clip;
        let (x, y) = menu_xy(crate::menu::MenuButton::Box);
        let mut changed = false;
        for _ in 0..24 {
            game.on_tap(x, y);
            if game.box_brain.current() != BoxBehaviorId::Idle {
                changed = true;
                break;
            }
        }
        assert!(changed, "box tap should eventually leave idle");
        assert_eq!(game.actor.clip, stick);
    }

    #[test]
    fn menu_dog_taps_the_dog() {
        let mut game = Game::new();
        let stick = game.actor.clip;
        let (x, y) = menu_xy(crate::menu::MenuButton::Dog);
        let mut changed = false;
        for _ in 0..16 {
            game.on_tap(x, y);
            if game.dog_brain.current() != crate::behavior::plugin::BehaviorId::Walking {
                changed = true;
                break;
            }
        }
        assert!(changed, "dog tap should eventually leave walk");
        assert_eq!(game.actor.clip, stick);
        assert!(core::ptr::eq(
            library::clip(game.dog_actor.clip).species,
            &library::DOG
        ));
    }

    #[test]
    fn menu_man_taps_the_stickman() {
        let mut game = Game::new();
        let box_clip = game.box_actor.clip;
        let (x, y) = menu_xy(crate::menu::MenuButton::Man);
        let mut changed = false;
        for _ in 0..16 {
            game.on_tap(x, y);
            if game.behavior_mgr.current() != crate::behavior::plugin::BehaviorId::Walking {
                changed = true;
                break;
            }
        }
        assert!(changed, "man tap should eventually leave walk");
        assert_eq!(game.box_actor.clip, box_clip);
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
    fn update_overlap_keeps_dog_species() {
        let mut game = Game::new();
        game.actor.x = game.dog_actor.x;
        game.update(0);
        let clip = library::clip(game.dog_actor.clip);
        assert!(core::ptr::eq(clip.species, &library::DOG));
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
        game.actor.y = floor_y_at(game.actor.x);
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

    #[test]
    fn walking_down_the_bump_follows_the_ramp() {
        let mut game = Game::new();
        let y0 = game.actor.y;
        assert_eq!(y0, floor_y_at(game.actor.x));
        assert!(y0 < floor_y(), "spawn should sit on the bump peak");
        game.actor.facing_left = false;
        for _ in 0..60 {
            game.update(33);
        }
        assert!(
            game.actor.y > y0,
            "walking off the peak should lower the feet, y0={y0} y={}",
            game.actor.y
        );
        assert_eq!(game.actor.y, floor_y_at(game.actor.x));
    }

    #[test]
    fn talking_draws_a_dialog_bubble_above_the_head() {
        let mut game = Game::new();
        for _ in 0..32 {
            game.on_cycle_input();
            if game.behavior_mgr.is_talking() {
                break;
            }
        }
        assert!(game.behavior_mgr.is_talking());
        game.update(0);
        let bubble = game.scratch.bubble.expect("talking should attach a bubble");
        let head = game.scratch.tip[library::HEAD as usize];
        assert!(bubble.bounds.top_left.y < head.y);
        assert!(crate::behavior::dialog::STICKMAN_LINES.contains(&bubble.text()));
        let dirty = eval::dirty_rect(&game.scratch);
        assert!(dirty.top_left.y <= bubble.bounds.top_left.y);
        assert!(dirty.size.width <= crate::dirty::DIRTY_MAX_W);
        assert!(dirty.size.height <= crate::dirty::DIRTY_MAX_H);
    }

    #[test]
    fn box_talking_draws_a_dialog_bubble() {
        let mut game = Game::new();
        let mut talked = false;
        for i in 0..800u32 {
            game.box_brain.on_event(
                &mut game.box_actor,
                Event::Tap,
                EventCtx::default(),
                i.wrapping_mul(0x9E37_79B9),
            );
            if game.box_brain.is_talking() {
                talked = true;
                break;
            }
        }
        assert!(talked, "1% talking should appear within 800 taps");
        game.update(0);
        let bubble = game
            .box_scratch
            .bubble
            .expect("talking crate should attach a bubble");
        assert!(crate::behavior::dialog::BOX_LINES.contains(&bubble.text()));
    }

    #[test]
    fn dog_talking_draws_a_dialog_bubble() {
        let mut game = Game::new();
        for _ in 0..32 {
            game.dog_brain.cycle_next(&mut game.dog_actor);
            if game.dog_brain.is_talking() {
                break;
            }
        }
        assert!(game.dog_brain.is_talking());
        game.update(0);
        let bubble = game
            .dog_scratch
            .bubble
            .expect("talking dog should attach a bubble");
        assert!(crate::behavior::dialog::DOG_LINES.contains(&bubble.text()));
        let head = game.dog_scratch.tip[library::HEAD as usize];
        assert!(bubble.bounds.top_left.y < head.y);
    }

    fn press_against_edge(game: &mut Game, left: bool) {
        game.actor.facing_left = left;
        // Sample on a flat wing first; the spawn pose is rotated on the bump.
        game.actor.x = if left {
            crate::menu::ROOM_LEFT + 40
        } else {
            DISPLAY_WIDTH as i32 - 40
        };
        game.actor.y = floor_y_at(game.actor.x);
        eval::sample(&game.actor, &mut game.scratch);
        let hit = eval::hitbox(&game.scratch);
        if left {
            let extent = game.actor.x - hit.top_left.x;
            game.actor.x = crate::menu::ROOM_LEFT + extent.max(1);
        } else {
            let extent = hit.top_left.x + hit.size.width as i32 - game.actor.x;
            game.actor.x = DISPLAY_WIDTH as i32 - extent;
        }
        game.actor.y = floor_y_at(game.actor.x);
        game.actor.apply_clip_velocity();
    }

    fn walk_off_edge(game: &mut Game, left: bool) {
        let from = game.current_room;
        press_against_edge(game, left);
        for _ in 0..12 {
            game.update(33);
            if game.current_room != from {
                return;
            }
        }
    }

    #[test]
    fn website_color_overrides_home_backdrop() {
        let mut game = Game::new();
        game.apply_config(ConfigCmd::SetBackdropColor {
            r: 255,
            g: 0,
            b: 0,
        });
        assert_eq!(game.backdrop(), Backdrop::Color(crate::config::rgb888_to_565(255, 0, 0)));
        game.apply_config(ConfigCmd::ClearBackdropColor);
        assert_ne!(game.backdrop(), Backdrop::Color(crate::config::rgb888_to_565(255, 0, 0)));
    }

    #[test]
    fn walking_off_home_right_enters_blue_room() {
        let mut game = Game::new();
        assert_eq!(game.current_room(), RoomId::Home);
        walk_off_edge(&mut game, false);
        assert_eq!(game.current_room(), RoomId::Blue);
        assert!(game.actor.x < DISPLAY_WIDTH as i32 / 2);
        assert!(!game.actor.facing_left);
        assert_eq!(game.backdrop(), Backdrop::Color(Rgb565::BLUE));
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
        assert_eq!(game.box_actor.y, floor_y_at(game.box_actor.x));
        assert!(game.box_actor.x >= library::BOX_WIDTH as i32);
        assert!(game.box_actor.x <= DISPLAY_WIDTH as i32 - library::BOX_WIDTH as i32);
        assert_eq!(game.box_brain.current(), BoxBehaviorId::Idle);
    }

    #[test]
    fn home_left_edge_still_bounces() {
        let mut game = Game::new();
        press_against_edge(&mut game, true);
        game.update(33);
        assert_eq!(game.current_room(), RoomId::Home);
        assert!(!game.actor.facing_left);
    }

    #[test]
    fn blue_right_edge_still_bounces() {
        let mut game = Game::new();
        walk_off_edge(&mut game, false);
        assert_eq!(game.current_room(), RoomId::Blue);
        press_against_edge(&mut game, false);
        game.update(33);
        assert_eq!(game.current_room(), RoomId::Blue);
        assert!(game.actor.facing_left);
    }

    #[test]
    fn walking_back_from_blue_restores_home_box() {
        let mut game = Game::new();
        let home_box_x = game.box_actor.x;
        walk_off_edge(&mut game, false);
        assert_eq!(game.current_room(), RoomId::Blue);
        walk_off_edge(&mut game, true);
        assert_eq!(game.current_room(), RoomId::Home);
        assert_eq!(game.box_actor.x, home_box_x);
        assert_eq!(game.box_actor.clip, ClipId::BoxIdle);
    }

    #[test]
    fn dog_stays_paused_in_home_while_away() {
        let mut game = Game::new();
        walk_off_edge(&mut game, false);
        assert_eq!(game.current_room(), RoomId::Blue);
        let dog_x = game.dog_actor.x;
        let dog_t = game.dog_actor.time_ms;
        let dog_clip = game.dog_actor.clip;
        let dog_vx = game.dog_actor.vx;
        for _ in 0..24 {
            game.update(33);
        }
        assert_eq!(game.dog_actor.x, dog_x);
        assert_eq!(game.dog_actor.time_ms, dog_t);
        assert_eq!(game.dog_actor.clip, dog_clip);
        assert_eq!(game.dog_actor.vx, dog_vx);
        walk_off_edge(&mut game, true);
        assert_eq!(game.current_room(), RoomId::Home);
        let after = game.dog_actor.time_ms;
        for _ in 0..8 {
            game.update(33);
        }
        assert_ne!(game.dog_actor.time_ms, after);
    }

    #[test]
    fn blue_room_box_slides_like_the_home_crate() {
        let mut game = Game::new();
        walk_off_edge(&mut game, false);
        assert_eq!(game.current_room(), RoomId::Blue);
        game.box_brain.on_event(
            &mut game.box_actor,
            Event::Collision,
            EventCtx {
                collision: Some(CollisionKind::Model),
                other_x: Some(game.actor.x),
                other_facing_left: Some(false),
                nx: -1,
                ny: 0,
            },
            0x51DE,
        );
        assert!(matches!(
            game.box_actor.clip,
            ClipId::BoxIdle | ClipId::BoxSlide | ClipId::BoxRoll | ClipId::BoxShudder
        ));
        game.update(33);
        assert_eq!(game.box_actor.y, floor_y_at(game.box_actor.x));
    }
}
