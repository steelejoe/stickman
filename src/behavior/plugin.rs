//! Behavior table: cycle order, clip, and locomotion.
//!
//! Drawing is not per-behavior. Each row names a [`ClipId`]; [`crate::game::Game`]
//! evaluates that clip. This module only runs world logic (travel, jump height,
//! facing). Screen-edge and model contacts are resolved by [`crate::collision`].
//!
//! Add a behavior with one row in [`behaviors!`]. Unique update code is a
//! [`Loco`] variant, not a new file.

use crate::behavior::event::{Event, EventCtx, Rng32};
use crate::collision::CollisionKind;
use crate::stickman::geometry::{self, floor_y};
use crate::stickman::ir::{Actor, ClipId, LoopMode};
use crate::stickman::library;

const FACE_PAUSE_MS: u32 = 500;
const FACE_STEPS: u32 = 4;
/// Auto-switch waits at least this long so a pose is visible.
const AUTO_SWITCH_MIN_MS: u32 = 1000;
/// Auto-switch never waits longer than this.
const AUTO_SWITCH_MAX_MS: u32 = 5000;

/// World-logic mode. Most clips are [`Loco::InPlace`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Loco {
    /// Advance the clip (no-op if static) and pin feet to the floor.
    InPlace,
    /// [`InPlace`] plus `travel_dx` along facing, bounce at the screen edges.
    WalkBounce,
    /// Travel with knockback wall facing (face away from the edge).
    Knockback,
    /// Parabolic hop; clip is the in-air tuck.
    Jump,
    /// Crouch clip; glance left↔right on a timer.
    Search,
}

/// Declare every behavior in cycle order: id, clip, locomotion.
macro_rules! behaviors {
    ($(($id:ident, $clip:ident, $loco:ident)),+ $(,)?) => {
        /// Identifies each behavior for transitions and input cycling.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum BehaviorId {
            $($id,)+
        }

        const BEHAVIOR_ORDER: &[BehaviorId] = &[$(BehaviorId::$id,)+];

        impl BehaviorId {
            pub fn clip(self) -> ClipId {
                match self {
                    $(Self::$id => ClipId::$clip,)+
                }
            }

            fn loco(self) -> Loco {
                match self {
                    $(Self::$id => Loco::$loco,)+
                }
            }

            fn emits_cycle_finished(self) -> bool {
                library::clip(self.clip()).loop_mode == LoopMode::Loop
            }
        }
    };
}

behaviors! {
    (Walking, Walk, WalkBounce),
    (Idle, Idle, InPlace),
    (Jumping, Jump, Jump),
    (Crouching, Crouch, InPlace),
    (Searching, Crouch, Search),
    (Begging, Beg, InPlace),
    (SwordStance, SwordStance, InPlace),
    (SwordStab, SwordStab, InPlace),
    (SwordCrouchStance, SwordCrouchStance, InPlace),
    (SwordCrouchStab, SwordCrouchStab, InPlace),
    (Knockback, Knockback, Knockback),
    (Tumbling, Tumble, WalkBounce),
    (FlipFacing, Flip, InPlace),
}

/// Current behavior, cycle index, and RNG. Search timer lives here so
/// searching does not need its own type.
pub struct BehaviorManager {
    current: BehaviorId,
    index: usize,
    rng: Rng32,
    /// Elapsed ms for [`Loco::Search`]; reset on switch.
    timer_ms: u32,
    /// Ms until the next automatic random behavior switch (held poses only).
    switch_remain_ms: u32,
    /// Remainder of a multi-step table outcome (`steps[next..]`).
    chain: Option<(&'static [BehaviorId], usize)>,
}

impl BehaviorManager {
    pub fn new() -> Self {
        let mut this = Self {
            current: BehaviorId::Walking,
            index: 0,
            rng: Rng32::new(0xA5A5_5A5A),
            timer_ms: 0,
            switch_remain_ms: AUTO_SWITCH_MAX_MS,
            chain: None,
        };
        this.roll_auto_switch();
        this
    }

    pub fn current(&self) -> BehaviorId {
        self.current
    }

    fn switch(&mut self, actor: &mut Actor, id: BehaviorId, index: usize) {
        self.index = index;
        self.current = id;
        self.timer_ms = 0;
        if id == BehaviorId::FlipFacing {
            actor.facing_left = !actor.facing_left;
        }
        actor.play(id.clip());
        self.roll_auto_switch();
    }

    fn index_of(id: BehaviorId) -> usize {
        BEHAVIOR_ORDER.iter().position(|&b| b == id).unwrap_or(0)
    }

    fn roll_auto_switch(&mut self) {
        let span = AUTO_SWITCH_MAX_MS - AUTO_SWITCH_MIN_MS + 1;
        self.switch_remain_ms = AUTO_SWITCH_MIN_MS + self.rng.next_u32() % span;
    }

    pub fn cycle_next(&mut self, actor: &mut Actor) {
        self.chain = None;
        let index = (self.index + 1) % BEHAVIOR_ORDER.len();
        self.switch(actor, BEHAVIOR_ORDER[index], index);
    }

    /// Switch to a uniformly chosen behavior other than the current one.
    ///
    /// `entropy` is mixed into the generator (tap X, frame delta, …) so device
    /// and sim picks are not a fixed sequence from a constant seed.
    pub fn cycle_random(&mut self, actor: &mut Actor, entropy: u32) {
        self.chain = None;
        self.rng.mix(entropy);
        let n = BEHAVIOR_ORDER.len();
        if n <= 1 {
            return;
        }
        let skip = 1 + (self.rng.next_u32() as usize % (n - 1));
        let index = (self.index + skip) % n;
        self.switch(actor, BEHAVIOR_ORDER[index], index);
    }

    /// Handle an event. Returns true when `BehaviorFinished` advanced a chain
    /// (so the caller should not also fire collision this tick).
    pub fn on_event(
        &mut self,
        actor: &mut Actor,
        event: Event,
        ctx: EventCtx,
        entropy: u32,
    ) -> bool {
        if event == Event::BehaviorFinished {
            if let Some(next) = self.take_chain_step() {
                self.switch(actor, next, Self::index_of(next));
                return true;
            }
        }
        self.rng.mix(entropy);
        let steps = self.rng.pick(stickman_weights(self.current, event, ctx));
        self.begin_chain(actor, steps);
        false
    }

    fn take_chain_step(&mut self) -> Option<BehaviorId> {
        let (steps, i) = self.chain?;
        let next = *steps.get(i)?;
        self.chain = if i + 1 < steps.len() {
            Some((steps, i + 1))
        } else {
            None
        };
        Some(next)
    }

    fn begin_chain(&mut self, actor: &mut Actor, steps: &'static [BehaviorId]) {
        debug_assert!(!steps.is_empty());
        let first = steps[0];
        if first != self.current || steps.len() > 1 {
            self.switch(actor, first, Self::index_of(first));
        }
        self.chain = if steps.len() > 1 {
            Some((steps, 1))
        } else {
            None
        };
    }

    /// Advance loco. Returns true when a looping clip finished a cycle.
    pub fn update(&mut self, delta_ms: u64, actor: &mut Actor) -> bool {
        self.rng.mix(delta_ms as u32);
        let dt = delta_ms as u32;
        if !self.current.emits_cycle_finished() {
            self.switch_remain_ms = self.switch_remain_ms.saturating_sub(dt);
            if self.switch_remain_ms == 0 {
                self.cycle_random(actor, dt);
            }
        }
        apply_loco(
            self.current.loco(),
            actor,
            dt,
            crate::DISPLAY_HEIGHT,
            &mut self.timer_ms,
        )
    }
}

fn apply_loco(
    loco: Loco,
    actor: &mut Actor,
    dt_ms: u32,
    display_height: u32,
    timer_ms: &mut u32,
) -> bool {
    match loco {
        Loco::InPlace => {
            let finished = actor.advance(dt_ms);
            actor.y = floor_y();
            finished
        }
        Loco::WalkBounce | Loco::Knockback => {
            let finished = actor.advance(dt_ms);
            actor.x += actor.take_travel(dt_ms);
            actor.y = floor_y();
            finished
        }
        Loco::Jump => {
            let finished = actor.advance(dt_ms);
            let floor = floor_y();
            let apex = geometry::jump_apex_foot_y(display_height as i32);
            let rise = (floor - apex).max(1);
            let period = library::clip(actor.clip).duration_ms.max(1) as u32;
            let t = actor.time_ms * 1000 / period;
            let height = rise * 4 * t as i32 * (1000 - t as i32) / (1000 * 1000);
            actor.y = floor - height;
            finished
        }
        Loco::Search => {
            *timer_ms = timer_ms.saturating_add(dt_ms) % (FACE_PAUSE_MS * FACE_STEPS);
            let step = *timer_ms / FACE_PAUSE_MS;
            actor.facing_left = step % 2 == 0;
            actor.y = floor_y();
            false
        }
    }
}

/// One table row: a short behavior sequence and its weight.
pub type WeightedChain = (&'static [BehaviorId], u16);

pub fn stickman_weights(id: BehaviorId, event: Event, ctx: EventCtx) -> &'static [WeightedChain] {
    if event == Event::Collision
        && matches!(
            ctx.collision,
            Some(CollisionKind::EdgeLeft | CollisionKind::EdgeRight)
        )
    {
        return STICKMAN_EDGE;
    }
    match (id, event) {
        (BehaviorId::Walking, Event::BehaviorFinished) => {
            &[(WALK, 80), (IDLE, 8), (JUMP, 6), (CROUCH, 6)]
        }
        (BehaviorId::Walking, Event::Collision) => &[(FLIP_THEN_WALK, 50), (KNOCKBACK, 50)],
        (BehaviorId::Jumping, Event::BehaviorFinished) => &[(WALK, 40), (JUMP, 30), (IDLE, 30)],
        (BehaviorId::SwordStab, Event::BehaviorFinished) => &[(SWORD_STANCE, 80), (SWORD_STAB, 20)],
        (BehaviorId::SwordCrouchStab, Event::BehaviorFinished) => {
            &[(SWORD_CROUCH_STANCE, 80), (SWORD_CROUCH_STAB, 20)]
        }
        (BehaviorId::Knockback, Event::BehaviorFinished) => {
            &[(WALK, 50), (IDLE, 30), (KNOCKBACK, 20)]
        }
        (BehaviorId::Tumbling, Event::BehaviorFinished) => &[(WALK, 20), (IDLE, 10), (TUMBLE, 70)],
        (BehaviorId::FlipFacing, Event::BehaviorFinished) => &[(WALK, 70), (IDLE, 20), (FLIP, 10)],
        (BehaviorId::FlipFacing, Event::Collision) => &[(FLIP_THEN_WALK, 50), (KNOCKBACK, 50)],
        (_, Event::Collision) => STICKMAN_COLLIDE,
        (_, Event::Tap) => STICKMAN_TAP,
        (_, Event::BehaviorFinished) => STICKMAN_STAY_WALK,
    }
}

const WALK: &[BehaviorId] = &[BehaviorId::Walking];
const IDLE: &[BehaviorId] = &[BehaviorId::Idle];
const JUMP: &[BehaviorId] = &[BehaviorId::Jumping];
const CROUCH: &[BehaviorId] = &[BehaviorId::Crouching];
const KNOCKBACK: &[BehaviorId] = &[BehaviorId::Knockback];
const TUMBLE: &[BehaviorId] = &[BehaviorId::Tumbling];
const FLIP: &[BehaviorId] = &[BehaviorId::FlipFacing];
const SWORD_STANCE: &[BehaviorId] = &[BehaviorId::SwordStance];
const SWORD_STAB: &[BehaviorId] = &[BehaviorId::SwordStab];
const SWORD_CROUCH_STANCE: &[BehaviorId] = &[BehaviorId::SwordCrouchStance];
const SWORD_CROUCH_STAB: &[BehaviorId] = &[BehaviorId::SwordCrouchStab];
const SEARCH: &[BehaviorId] = &[BehaviorId::Searching];
const BEG: &[BehaviorId] = &[BehaviorId::Begging];
const FLIP_THEN_WALK: &[BehaviorId] = &[BehaviorId::FlipFacing, BehaviorId::Walking];

const STICKMAN_COLLIDE: &[WeightedChain] = &[(FLIP_THEN_WALK, 70), (KNOCKBACK, 30)];

/// Screen edges: turn and walk back. Flip+knockback travels the old heading
/// (opposite the new facing) and would leave the display.
const STICKMAN_EDGE: &[WeightedChain] = &[(FLIP_THEN_WALK, 100)];

const STICKMAN_TAP: &[WeightedChain] = &[
    (WALK, 12),
    (IDLE, 12),
    (JUMP, 12),
    (CROUCH, 10),
    (SEARCH, 8),
    (BEG, 8),
    (SWORD_STANCE, 8),
    (SWORD_STAB, 8),
    (SWORD_CROUCH_STANCE, 6),
    (SWORD_CROUCH_STAB, 6),
    (KNOCKBACK, 5),
    (TUMBLE, 5),
];

const STICKMAN_STAY_WALK: &[WeightedChain] = &[(WALK, 1)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_random_never_stays_on_current() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let n = BEHAVIOR_ORDER.len();
        assert!(n > 1);
        for i in 0..n * 20 {
            let prev = mgr.index;
            mgr.cycle_random(&mut actor, i as u32);
            assert_ne!(mgr.index, prev);
            assert!(mgr.index < n);
            assert_eq!(actor.clip, BEHAVIOR_ORDER[mgr.index].clip());
        }
    }

    #[test]
    fn cycle_random_can_reach_every_other_behavior() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let n = BEHAVIOR_ORDER.len();
        assert!(n > 1 && n <= 32);
        let mut seen: u32 = 1 << mgr.index;
        for i in 0..n * 32 {
            mgr.cycle_random(&mut actor, i as u32);
            seen |= 1 << mgr.index;
        }
        assert_eq!(seen, (1 << n) - 1);
    }

    #[test]
    fn searching_reuses_crouch_clip() {
        assert_eq!(BehaviorId::Searching.clip(), ClipId::Crouch);
        assert_eq!(BehaviorId::Crouching.clip(), ClipId::Crouch);
        assert_ne!(BehaviorId::Searching.loco(), BehaviorId::Crouching.loco());
    }

    #[test]
    fn collision_chain_flip_then_walk_advances_on_finish() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let facing = actor.facing_left;
        mgr.begin_chain(&mut actor, FLIP_THEN_WALK);
        assert_eq!(mgr.current, BehaviorId::FlipFacing);
        assert_ne!(actor.facing_left, facing);
        assert_eq!(actor.clip, ClipId::Flip);
        let chained = mgr.on_event(&mut actor, Event::BehaviorFinished, EventCtx::default(), 1);
        assert!(chained);
        assert_eq!(mgr.current, BehaviorId::Walking);
        assert_eq!(actor.clip, ClipId::Walk);
        assert_ne!(actor.facing_left, facing);
    }

    #[test]
    fn collision_knockback_does_not_flip_first() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let facing = actor.facing_left;
        mgr.begin_chain(&mut actor, KNOCKBACK);
        assert_eq!(mgr.current, BehaviorId::Knockback);
        assert_eq!(actor.clip, ClipId::Knockback);
        assert_eq!(actor.facing_left, facing);
        let chained = mgr.on_event(&mut actor, Event::BehaviorFinished, EventCtx::default(), 1);
        assert!(!chained);
    }

    fn assert_auto_switch_in_range(ms: u32) {
        assert!(
            (AUTO_SWITCH_MIN_MS..=AUTO_SWITCH_MAX_MS).contains(&ms),
            "auto-switch interval {ms} out of range"
        );
    }

    #[test]
    fn auto_switch_interval_is_at_most_5s() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        assert_auto_switch_in_range(mgr.switch_remain_ms);
        for i in 0..32 {
            mgr.cycle_random(&mut actor, i as u32);
            assert_auto_switch_in_range(mgr.switch_remain_ms);
        }
    }

    #[test]
    fn auto_switch_fires_when_timer_elapses() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        mgr.cycle_next(&mut actor);
        assert_eq!(mgr.current, BehaviorId::Idle);
        assert!(!mgr.current.emits_cycle_finished());
        let prev = mgr.index;
        let remain = mgr.switch_remain_ms;
        mgr.update((remain - 1) as u64, &mut actor);
        assert_eq!(mgr.index, prev);
        assert_eq!(mgr.switch_remain_ms, 1);
        mgr.update(1, &mut actor);
        assert_ne!(mgr.index, prev);
        assert_auto_switch_in_range(mgr.switch_remain_ms);
    }

    #[test]
    fn manual_cycle_resets_auto_switch_timer() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        mgr.cycle_next(&mut actor);
        assert_eq!(mgr.current, BehaviorId::Idle);
        mgr.update((mgr.switch_remain_ms - 1) as u64, &mut actor);
        assert_eq!(mgr.switch_remain_ms, 1);
        mgr.cycle_next(&mut actor);
        assert_auto_switch_in_range(mgr.switch_remain_ms);
        mgr.cycle_next(&mut actor);
        assert_eq!(mgr.current, BehaviorId::Crouching);
        mgr.update((mgr.switch_remain_ms - 1) as u64, &mut actor);
        assert_eq!(mgr.switch_remain_ms, 1);
        mgr.cycle_random(&mut actor, 7);
        assert_auto_switch_in_range(mgr.switch_remain_ms);
    }
}
