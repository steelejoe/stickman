//! Box behaviors: idle, slide, roll, shudder.

use crate::behavior::event::{Event, EventCtx, Rng32};
use crate::collision::CollisionKind;
use crate::stickman::geometry::floor_y;
use crate::stickman::ir::{Actor, ClipId};

/// Kind-specific box behaviors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoxBehaviorId {
    Idle,
    Sliding,
    Rolling,
    Shudder,
}

impl BoxBehaviorId {
    pub fn clip(self) -> ClipId {
        match self {
            Self::Idle => ClipId::BoxIdle,
            Self::Sliding => ClipId::BoxSlide,
            Self::Rolling => ClipId::BoxRoll,
            Self::Shudder => ClipId::BoxShudder,
        }
    }
}

pub struct BoxBrain {
    current: BoxBehaviorId,
    rng: Rng32,
}

impl BoxBrain {
    pub fn new() -> Self {
        Self {
            current: BoxBehaviorId::Idle,
            rng: Rng32::new(0xB0B0_B0B0),
        }
    }

    pub fn current(&self) -> BoxBehaviorId {
        self.current
    }

    pub fn on_event(&mut self, actor: &mut Actor, event: Event, ctx: EventCtx, entropy: u32) {
        self.rng.mix(entropy);
        let next = self.rng.pick(box_weights(self.current, event));
        if next != self.current {
            self.switch(actor, next, ctx);
        } else if matches!(next, BoxBehaviorId::Sliding | BoxBehaviorId::Rolling)
            && event == Event::Collision
        {
            apply_push_facing(actor, ctx);
        }
    }

    fn switch(&mut self, actor: &mut Actor, id: BoxBehaviorId, ctx: EventCtx) {
        self.current = id;
        if matches!(id, BoxBehaviorId::Sliding | BoxBehaviorId::Rolling) {
            apply_push_facing(actor, ctx);
        }
        actor.play(id.clip());
    }

    /// Advance clip + loco. Returns true when a looping clip finished a cycle.
    pub fn update(&mut self, delta_ms: u64, actor: &mut Actor) -> bool {
        self.rng.mix(delta_ms as u32);
        let dt = delta_ms as u32;
        let finished = actor.advance(dt);
        match self.current {
            BoxBehaviorId::Idle | BoxBehaviorId::Shudder => {
                actor.y = floor_y();
            }
            BoxBehaviorId::Sliding | BoxBehaviorId::Rolling => {
                actor.x += actor.take_travel(dt);
                actor.y = floor_y();
            }
        }
        finished
    }
}

impl Default for BoxBrain {
    fn default() -> Self {
        Self::new()
    }
}

pub fn box_weights(id: BoxBehaviorId, event: Event) -> &'static [(BoxBehaviorId, u16)] {
    match (id, event) {
        (BoxBehaviorId::Idle, Event::Collision) => BOX_IDLE_COLLIDE,
        (BoxBehaviorId::Idle, Event::Tap) => BOX_IDLE_TAP,
        (BoxBehaviorId::Sliding, Event::BehaviorFinished) => {
            &[(BoxBehaviorId::Idle, 80), (BoxBehaviorId::Sliding, 20)]
        }
        (BoxBehaviorId::Sliding, Event::Collision) => &[
            (BoxBehaviorId::Idle, 70),
            (BoxBehaviorId::Sliding, 15),
            (BoxBehaviorId::Shudder, 10),
            (BoxBehaviorId::Rolling, 5),
        ],
        (BoxBehaviorId::Sliding, Event::Tap) => &[
            (BoxBehaviorId::Idle, 60),
            (BoxBehaviorId::Shudder, 25),
            (BoxBehaviorId::Sliding, 15),
        ],
        (BoxBehaviorId::Rolling, Event::BehaviorFinished) => &[
            (BoxBehaviorId::Idle, 80),
            (BoxBehaviorId::Rolling, 15),
            (BoxBehaviorId::Sliding, 5),
        ],
        (BoxBehaviorId::Rolling, Event::Collision) => &[
            (BoxBehaviorId::Idle, 70),
            (BoxBehaviorId::Rolling, 15),
            (BoxBehaviorId::Shudder, 10),
            (BoxBehaviorId::Sliding, 5),
        ],
        (BoxBehaviorId::Rolling, Event::Tap) => &[
            (BoxBehaviorId::Idle, 70),
            (BoxBehaviorId::Shudder, 20),
            (BoxBehaviorId::Rolling, 10),
        ],
        (BoxBehaviorId::Shudder, Event::BehaviorFinished) => {
            &[(BoxBehaviorId::Idle, 90), (BoxBehaviorId::Shudder, 10)]
        }
        (BoxBehaviorId::Shudder, Event::Collision) => &[
            (BoxBehaviorId::Idle, 75),
            (BoxBehaviorId::Shudder, 15),
            (BoxBehaviorId::Sliding, 7),
            (BoxBehaviorId::Rolling, 3),
        ],
        (BoxBehaviorId::Shudder, Event::Tap) => {
            &[(BoxBehaviorId::Idle, 50), (BoxBehaviorId::Shudder, 50)]
        }
        (BoxBehaviorId::Idle, Event::BehaviorFinished) => &[(BoxBehaviorId::Idle, 1)],
        (_, Event::Falling) => &[(BoxBehaviorId::Idle, 1)],
    }
}

/// Idle × collision: do nothing is favored.
pub const BOX_IDLE_COLLIDE: &[(BoxBehaviorId, u16)] = &[
    (BoxBehaviorId::Idle, 80),
    (BoxBehaviorId::Sliding, 12),
    (BoxBehaviorId::Rolling, 5),
    (BoxBehaviorId::Shudder, 3),
];

const BOX_IDLE_TAP: &[(BoxBehaviorId, u16)] = &[
    (BoxBehaviorId::Idle, 70),
    (BoxBehaviorId::Shudder, 20),
    (BoxBehaviorId::Sliding, 7),
    (BoxBehaviorId::Rolling, 3),
];

fn apply_push_facing(actor: &mut Actor, ctx: EventCtx) {
    if let Some(face) = ctx.other_facing_left {
        actor.facing_left = face;
        return;
    }
    match ctx.collision {
        Some(CollisionKind::EdgeLeft) => actor.facing_left = false,
        Some(CollisionKind::EdgeRight) => actor.facing_left = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_is_box_idle_clip() {
        assert_eq!(BoxBehaviorId::Idle.clip(), ClipId::BoxIdle);
        assert_eq!(BoxBehaviorId::Sliding.clip(), ClipId::BoxSlide);
        assert_eq!(BoxBehaviorId::Rolling.clip(), ClipId::BoxRoll);
        assert_eq!(BoxBehaviorId::Shudder.clip(), ClipId::BoxShudder);
    }

    #[test]
    fn sliding_moves_along_facing() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        brain.switch(&mut actor, BoxBehaviorId::Sliding, EventCtx::default());
        actor.facing_left = false;
        let x0 = actor.x;
        brain.update(200, &mut actor);
        assert!(actor.x > x0);
        actor.facing_left = true;
        let x1 = actor.x;
        brain.update(200, &mut actor);
        assert!(actor.x < x1);
    }

    #[test]
    fn collision_can_start_slide_when_sure() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        actor.play(ClipId::BoxIdle);
        brain.on_event(
            &mut actor,
            Event::Collision,
            EventCtx {
                other_facing_left: Some(true),
                ..EventCtx::default()
            },
            1,
        );
        assert!(matches!(
            brain.current(),
            BoxBehaviorId::Idle
                | BoxBehaviorId::Sliding
                | BoxBehaviorId::Rolling
                | BoxBehaviorId::Shudder
        ));
        assert_eq!(actor.clip, brain.current().clip());
    }
}
