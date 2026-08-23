//! Box behaviors: idle, slide, roll, shudder, talking.

use crate::behavior::dialog::{self, BOX_LINES};
use crate::behavior::event::{Event, EventCtx, Rng32};
use crate::collision::CollisionKind;
use crate::stickman::ir::{Actor, ClipId};

const TALK_MIN_MS: u32 = 1500;
const TALK_MAX_MS: u32 = 3000;

/// Kind-specific box behaviors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoxBehaviorId {
    Idle,
    Sliding,
    Rolling,
    Shudder,
    Talking,
}

impl BoxBehaviorId {
    pub fn clip(self) -> ClipId {
        match self {
            Self::Idle | Self::Talking => ClipId::BoxIdle,
            Self::Sliding => ClipId::BoxSlide,
            Self::Rolling => ClipId::BoxRoll,
            Self::Shudder => ClipId::BoxShudder,
        }
    }
}

pub struct BoxBrain {
    current: BoxBehaviorId,
    rng: Rng32,
    talk_remain_ms: u32,
    bubble_left: bool,
    phrase: &'static str,
}

impl BoxBrain {
    pub fn new() -> Self {
        Self {
            current: BoxBehaviorId::Idle,
            rng: Rng32::new(0xB0B0_B0B0),
            talk_remain_ms: 0,
            bubble_left: false,
            phrase: "",
        }
    }

    pub fn current(&self) -> BoxBehaviorId {
        self.current
    }

    pub fn is_talking(&self) -> bool {
        self.current == BoxBehaviorId::Talking
    }

    pub fn bubble_left(&self) -> bool {
        self.bubble_left
    }

    pub fn talk_phrase(&self) -> Option<&'static str> {
        if self.is_talking() && !self.phrase.is_empty() {
            Some(self.phrase)
        } else {
            None
        }
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
            actor.apply_clip_velocity();
        }
    }

    fn switch(&mut self, actor: &mut Actor, id: BoxBehaviorId, ctx: EventCtx) {
        self.current = id;
        if matches!(id, BoxBehaviorId::Sliding | BoxBehaviorId::Rolling) {
            apply_push_facing(actor, ctx);
        }
        if id == BoxBehaviorId::Talking {
            self.bubble_left = self.rng.next_u32() & 1 == 1;
            self.phrase = dialog::pick_line(&mut self.rng, BOX_LINES);
            let span = TALK_MAX_MS - TALK_MIN_MS + 1;
            self.talk_remain_ms = TALK_MIN_MS + self.rng.next_u32() % span;
        } else {
            self.talk_remain_ms = 0;
        }
        actor.play(id.clip());
        match id {
            BoxBehaviorId::Sliding | BoxBehaviorId::Rolling => actor.apply_clip_velocity(),
            BoxBehaviorId::Idle | BoxBehaviorId::Shudder | BoxBehaviorId::Talking => actor.vx = 0,
        }
    }

    /// Advance clip + loco. Returns true when a looping clip finished a cycle.
    pub fn update(&mut self, delta_ms: u64, actor: &mut Actor) -> bool {
        self.rng.mix(delta_ms as u32);
        let dt = delta_ms as u32;
        let finished = actor.advance(dt);
        match self.current {
            BoxBehaviorId::Idle | BoxBehaviorId::Shudder => {
                actor.vx = 0;
            }
            BoxBehaviorId::Talking => {
                actor.vx = 0;
                self.talk_remain_ms = self.talk_remain_ms.saturating_sub(dt);
                if self.talk_remain_ms == 0 {
                    self.switch(actor, BoxBehaviorId::Idle, EventCtx::default());
                }
            }
            BoxBehaviorId::Sliding | BoxBehaviorId::Rolling => {
                actor.apply_clip_velocity();
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
    let id = match id {
        BoxBehaviorId::Talking => BoxBehaviorId::Idle,
        other => other,
    };
    match (id, event) {
        (BoxBehaviorId::Idle, Event::Collision) => BOX_IDLE_COLLIDE,
        (BoxBehaviorId::Idle, Event::Tap) => BOX_IDLE_TAP,
        (BoxBehaviorId::Sliding, Event::BehaviorFinished) => &[
            (BoxBehaviorId::Idle, 79),
            (BoxBehaviorId::Sliding, 20),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Sliding, Event::Collision) => &[
            (BoxBehaviorId::Idle, 69),
            (BoxBehaviorId::Sliding, 15),
            (BoxBehaviorId::Shudder, 10),
            (BoxBehaviorId::Rolling, 5),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Sliding, Event::Tap) => &[
            (BoxBehaviorId::Idle, 59),
            (BoxBehaviorId::Shudder, 25),
            (BoxBehaviorId::Sliding, 15),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Rolling, Event::BehaviorFinished) => &[
            (BoxBehaviorId::Idle, 79),
            (BoxBehaviorId::Rolling, 15),
            (BoxBehaviorId::Sliding, 5),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Rolling, Event::Collision) => &[
            (BoxBehaviorId::Idle, 69),
            (BoxBehaviorId::Rolling, 15),
            (BoxBehaviorId::Shudder, 10),
            (BoxBehaviorId::Sliding, 5),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Rolling, Event::Tap) => &[
            (BoxBehaviorId::Idle, 69),
            (BoxBehaviorId::Shudder, 20),
            (BoxBehaviorId::Rolling, 10),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Shudder, Event::BehaviorFinished) => &[
            (BoxBehaviorId::Idle, 89),
            (BoxBehaviorId::Shudder, 10),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Shudder, Event::Collision) => &[
            (BoxBehaviorId::Idle, 74),
            (BoxBehaviorId::Shudder, 15),
            (BoxBehaviorId::Sliding, 7),
            (BoxBehaviorId::Rolling, 3),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Shudder, Event::Tap) => &[
            (BoxBehaviorId::Idle, 49),
            (BoxBehaviorId::Shudder, 50),
            (BoxBehaviorId::Talking, 1),
        ],
        (BoxBehaviorId::Idle, Event::BehaviorFinished) => {
            &[(BoxBehaviorId::Idle, 99), (BoxBehaviorId::Talking, 1)]
        }
        (_, Event::Falling) => &[(BoxBehaviorId::Idle, 99), (BoxBehaviorId::Talking, 1)],
        _ => &[(BoxBehaviorId::Idle, 99), (BoxBehaviorId::Talking, 1)],
    }
}

/// Idle × collision: do nothing is favored. Talking is 1%.
pub const BOX_IDLE_COLLIDE: &[(BoxBehaviorId, u16)] = &[
    (BoxBehaviorId::Idle, 79),
    (BoxBehaviorId::Sliding, 12),
    (BoxBehaviorId::Rolling, 5),
    (BoxBehaviorId::Shudder, 3),
    (BoxBehaviorId::Talking, 1),
];

const BOX_IDLE_TAP: &[(BoxBehaviorId, u16)] = &[
    (BoxBehaviorId::Idle, 69),
    (BoxBehaviorId::Shudder, 20),
    (BoxBehaviorId::Sliding, 7),
    (BoxBehaviorId::Rolling, 3),
    (BoxBehaviorId::Talking, 1),
];

fn apply_push_facing(actor: &mut Actor, ctx: EventCtx) {
    if let Some(other_x) = ctx.other_x {
        if other_x != actor.x {
            // Slide away from the other body, not along its facing.
            actor.facing_left = other_x > actor.x;
            return;
        }
    }
    if let Some(face) = ctx.other_facing_left {
        actor.facing_left = !face;
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

    fn table_talk_pct(rows: &[(BoxBehaviorId, u16)]) -> (u32, u32) {
        let talk: u32 = rows
            .iter()
            .filter(|(id, _)| *id == BoxBehaviorId::Talking)
            .map(|(_, w)| *w as u32)
            .sum();
        let total: u32 = rows.iter().map(|(_, w)| *w as u32).sum();
        (talk, total)
    }

    #[test]
    fn idle_is_box_idle_clip() {
        assert_eq!(BoxBehaviorId::Idle.clip(), ClipId::BoxIdle);
        assert_eq!(BoxBehaviorId::Sliding.clip(), ClipId::BoxSlide);
        assert_eq!(BoxBehaviorId::Rolling.clip(), ClipId::BoxRoll);
        assert_eq!(BoxBehaviorId::Shudder.clip(), ClipId::BoxShudder);
        assert_eq!(BoxBehaviorId::Talking.clip(), ClipId::BoxIdle);
    }

    #[test]
    fn talking_is_one_percent_of_idle_tables() {
        for event in [
            Event::Collision,
            Event::Tap,
            Event::Falling,
            Event::BehaviorFinished,
        ] {
            let (talk, total) = table_talk_pct(box_weights(BoxBehaviorId::Idle, event));
            assert_eq!(total, 100, "{event:?}");
            assert_eq!(talk, 1, "{event:?}");
        }
    }

    #[test]
    fn talking_picks_a_box_line() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        brain.switch(&mut actor, BoxBehaviorId::Talking, EventCtx::default());
        let line = brain.talk_phrase().expect("talking");
        assert!(BOX_LINES.contains(&line));
        assert_eq!(actor.clip, ClipId::BoxIdle);
        assert_eq!(actor.vx, 0);
        assert!(brain.talk_remain_ms >= TALK_MIN_MS);
        assert!(brain.talk_remain_ms <= TALK_MAX_MS);
    }

    #[test]
    fn talking_timer_returns_to_idle() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        brain.switch(&mut actor, BoxBehaviorId::Talking, EventCtx::default());
        let remain = brain.talk_remain_ms;
        brain.update((remain - 1) as u64, &mut actor);
        assert!(brain.is_talking());
        brain.update(1, &mut actor);
        assert_eq!(brain.current(), BoxBehaviorId::Idle);
        assert!(brain.talk_phrase().is_none());
    }

    #[test]
    fn slide_after_collision_moves_away_from_the_other() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        actor.x = 200;
        brain.switch(
            &mut actor,
            BoxBehaviorId::Sliding,
            EventCtx {
                other_x: Some(150),
                other_facing_left: Some(false),
                ..EventCtx::default()
            },
        );
        assert!(!actor.facing_left, "stickman on the left → slide right");
        assert!(actor.vx > 0);

        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        actor.x = 200;
        brain.switch(
            &mut actor,
            BoxBehaviorId::Sliding,
            EventCtx {
                other_x: Some(260),
                other_facing_left: Some(true),
                ..EventCtx::default()
            },
        );
        assert!(actor.facing_left, "stickman on the right → slide left");
        assert!(actor.vx < 0);
    }

    #[test]
    fn slide_inverts_other_facing_when_x_is_unknown() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        brain.switch(
            &mut actor,
            BoxBehaviorId::Sliding,
            EventCtx {
                other_facing_left: Some(false),
                ..EventCtx::default()
            },
        );
        assert!(actor.facing_left);
        assert!(actor.vx < 0);
    }

    #[test]
    fn sliding_moves_along_facing() {
        let mut brain = BoxBrain::new();
        let mut actor = Actor::default();
        brain.switch(&mut actor, BoxBehaviorId::Sliding, EventCtx::default());
        actor.facing_left = false;
        actor.apply_clip_velocity();
        let x0 = actor.x;
        let dt = 200;
        actor.integrate(dt);
        assert!(actor.x > x0);
        actor.facing_left = true;
        actor.apply_clip_velocity();
        let x1 = actor.x;
        actor.integrate(dt);
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
                | BoxBehaviorId::Talking
        ));
        assert_eq!(actor.clip, brain.current().clip());
    }
}
