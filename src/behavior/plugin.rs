//! Behavior table: cycle order, clip, and locomotion.
//!
//! Drawing is not per-behavior. Each row names a [`ClipId`]; [`crate::game::Game`]
//! evaluates that clip. This module only runs world logic (travel, jump height,
//! facing). Screen-edge and model contacts are resolved by [`crate::collision`].
//!
//! Add a behavior with one row in [`behaviors!`]. Unique update code is a
//! [`Loco`] variant, not a new file.

use crate::stickman::geometry::{self, floor_y};
use crate::stickman::ir::{Actor, ClipId};
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
}

/// Current behavior, cycle index, and RNG. Search timer lives here so
/// searching does not need its own type.
pub struct BehaviorManager {
    current: BehaviorId,
    index: usize,
    /// xorshift32 state; never zero.
    rng: u32,
    /// Elapsed ms for [`Loco::Search`]; reset on switch.
    timer_ms: u32,
    /// Ms until the next automatic random behavior switch.
    switch_remain_ms: u32,
}

impl BehaviorManager {
    pub fn new() -> Self {
        let mut this = Self {
            current: BehaviorId::Walking,
            index: 0,
            rng: 0xA5A5_5A5A,
            timer_ms: 0,
            switch_remain_ms: AUTO_SWITCH_MAX_MS,
        };
        this.roll_auto_switch();
        this
    }

    fn switch(&mut self, actor: &mut Actor, id: BehaviorId, index: usize) {
        self.index = index;
        self.current = id;
        self.timer_ms = 0;
        actor.play(id.clip());
        self.roll_auto_switch();
    }

    fn roll_auto_switch(&mut self) {
        let span = AUTO_SWITCH_MAX_MS - AUTO_SWITCH_MIN_MS + 1;
        self.switch_remain_ms = AUTO_SWITCH_MIN_MS + self.next_u32() % span;
    }

    pub fn cycle_next(&mut self, actor: &mut Actor) {
        let index = (self.index + 1) % BEHAVIOR_ORDER.len();
        self.switch(actor, BEHAVIOR_ORDER[index], index);
    }

    /// Switch to a uniformly chosen behavior other than the current one.
    ///
    /// `entropy` is mixed into the generator (tap X, frame delta, …) so device
    /// and sim picks are not a fixed sequence from a constant seed.
    pub fn cycle_random(&mut self, actor: &mut Actor, entropy: u32) {
        self.mix_entropy(entropy);
        let n = BEHAVIOR_ORDER.len();
        if n <= 1 {
            return;
        }
        let skip = 1 + (self.next_u32() as usize % (n - 1));
        let index = (self.index + skip) % n;
        self.switch(actor, BEHAVIOR_ORDER[index], index);
    }

    fn mix_entropy(&mut self, entropy: u32) {
        self.rng ^= entropy.wrapping_mul(0x9E37_79B9);
        if self.rng == 0 {
            self.rng = 1;
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = if x == 0 { 1 } else { x };
        self.rng
    }

    pub fn update(&mut self, delta_ms: u64, actor: &mut Actor) {
        self.mix_entropy(delta_ms as u32);
        let dt = delta_ms as u32;
        self.switch_remain_ms = self.switch_remain_ms.saturating_sub(dt);
        if self.switch_remain_ms == 0 {
            self.cycle_random(actor, dt);
        }
        apply_loco(
            self.current.loco(),
            actor,
            dt,
            crate::DISPLAY_HEIGHT,
            &mut self.timer_ms,
        );
    }
}

fn apply_loco(loco: Loco, actor: &mut Actor, dt_ms: u32, display_height: u32, timer_ms: &mut u32) {
    match loco {
        Loco::InPlace => {
            actor.advance(dt_ms);
            actor.y = floor_y();
        }
        Loco::WalkBounce => {
            actor.advance(dt_ms);
            actor.x += actor.take_travel(dt_ms);
            actor.y = floor_y();
        }
        Loco::Knockback => {
            actor.advance(dt_ms);
            actor.x += actor.take_travel(dt_ms);
            actor.y = floor_y();
        }
        Loco::Jump => {
            actor.advance(dt_ms);
            let floor = floor_y();
            let apex = geometry::jump_apex_foot_y(display_height as i32);
            let rise = (floor - apex).max(1);
            let period = library::clip(actor.clip).duration_ms.max(1) as u32;
            let t = actor.time_ms * 1000 / period;
            let height = rise * 4 * t as i32 * (1000 - t as i32) / (1000 * 1000);
            actor.y = floor - height;
        }
        Loco::Search => {
            *timer_ms = timer_ms.saturating_add(dt_ms) % (FACE_PAUSE_MS * FACE_STEPS);
            let step = *timer_ms / FACE_PAUSE_MS;
            actor.facing_left = step % 2 == 0;
            actor.y = floor_y();
        }
    }
}

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
        mgr.update((mgr.switch_remain_ms - 1) as u64, &mut actor);
        assert_eq!(mgr.switch_remain_ms, 1);
        mgr.cycle_next(&mut actor);
        assert_auto_switch_in_range(mgr.switch_remain_ms);
        mgr.update((mgr.switch_remain_ms - 1) as u64, &mut actor);
        assert_eq!(mgr.switch_remain_ms, 1);
        mgr.cycle_random(&mut actor, 7);
        assert_auto_switch_in_range(mgr.switch_remain_ms);
    }
}
