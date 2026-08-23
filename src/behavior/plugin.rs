//! Behavior table: cycle order, clip, and locomotion.
//!
//! Drawing is not per-behavior except [`BehaviorId::Talking`], which overlays a
//! speech bubble above the head. Each row names a [`ClipId`]; [`crate::game::Game`]
//! evaluates that clip. This module sets the travel vector (walk / jump impulse).
//! Facing follows that vector. Gravity, integration, and edge reflection live
//! in [`crate::collision`]; entered hits still roll this table.
//!
//! Add a behavior with one row in [`behaviors!`]. Unique update code is a
//! [`Loco`] variant, not a new file.

use crate::behavior::event::{Event, EventCtx, Rng32};
use crate::collision::{self, CollisionKind};
use crate::stickman::geometry::{self, floor_y, JUMP_FORWARD_RISE};
use crate::stickman::ir::{Actor, ClipId, LoopMode};
use crate::stickman::library;

const FACE_PAUSE_MS: u32 = 500;
const FACE_STEPS: u32 = 4;
/// Auto-switch waits at least this long so a pose is visible.
const AUTO_SWITCH_MIN_MS: u32 = 1000;
/// Auto-switch never waits longer than this.
const AUTO_SWITCH_MAX_MS: u32 = 5000;
/// Empty-space tap chance of [`BehaviorId::FlipFacing`].
const EMPTY_FLIP_PCT: u32 = 15;

/// World-logic mode. Most clips are [`Loco::InPlace`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Loco {
    /// Advance the clip (no-op if static). Horizontal velocity is zero.
    InPlace,
    /// [`InPlace`] plus clip travel along facing (walls reflect the vector).
    WalkBounce,
    /// Travel with knockback wall facing (face away from the edge).
    Knockback,
    /// Upward impulse; clip is the in-air tuck.
    Jump,
    /// Forward hop: same tuck plus clip travel along facing.
    JumpForward,
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

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$id => stringify!($id),)+
                }
            }
        }
    };
}

behaviors! {
    (Walking, Walk, WalkBounce),
    (Idle, Idle, InPlace),
    (Jumping, Jump, Jump),
    (JumpForward, JumpForward, JumpForward),
    (Crouching, Crouch, InPlace),
    (Crawling, Crawl, WalkBounce),
    (Searching, Crouch, Search),
    (Begging, Beg, InPlace),
    (SwordStance, SwordStance, InPlace),
    (SwordStab, SwordStab, InPlace),
    (SwordCrouchStance, SwordCrouchStance, InPlace),
    (SwordCrouchStab, SwordCrouchStab, InPlace),
    (Knockback, Knockback, Knockback),
    (Tumbling, Tumble, WalkBounce),
    (FlipFacing, Flip, InPlace),
    (Talking, Idle, InPlace),
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
    /// Last three behaviors entered before the current one.
    recent: [BehaviorId; 3],
    recent_n: u8,
    /// Preferred speech-bubble side while [`BehaviorId::Talking`].
    bubble_left: bool,
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
            recent: [BehaviorId::Walking; 3],
            recent_n: 0,
            bubble_left: false,
        };
        this.roll_auto_switch();
        this
    }

    pub fn current(&self) -> BehaviorId {
        self.current
    }

    pub fn is_talking(&self) -> bool {
        self.current == BehaviorId::Talking
    }

    /// Side of the head for the speech bubble (rolled when talking starts).
    pub fn bubble_left(&self) -> bool {
        self.bubble_left
    }

    /// Write the last (up to three) behavior names, one per line. Returns bytes.
    pub fn write_recent_names(&self, buf: &mut [u8]) -> usize {
        let n = self.recent_n as usize;
        if n == 0 || buf.is_empty() {
            return 0;
        }
        let mut i = 0usize;
        for k in 0..n {
            if k > 0 {
                if i >= buf.len() {
                    break;
                }
                buf[i] = b'\n';
                i += 1;
            }
            let name = self.recent[k].name().as_bytes();
            let take = name.len().min(buf.len().saturating_sub(i));
            buf[i..i + take].copy_from_slice(&name[..take]);
            i += take;
        }
        i
    }

    /// True while a jump loco is playing (impulse already applied).
    pub fn in_jump_arc(&self) -> bool {
        matches!(self.current.loco(), Loco::Jump | Loco::JumpForward)
    }

    fn switch(&mut self, actor: &mut Actor, id: BehaviorId, index: usize) {
        if self.current != id {
            self.push_recent(self.current);
        }
        self.index = index;
        self.current = id;
        self.timer_ms = 0;
        if id == BehaviorId::Talking {
            self.bubble_left = self.rng.next_u32() & 1 == 1;
        }
        if id == BehaviorId::FlipFacing {
            // Turn around: reverse any travel vector, then face along it.
            actor.vx = -actor.vx;
            if actor.vx == 0 {
                actor.facing_left = !actor.facing_left;
            } else {
                actor.sync_facing();
            }
        }
        actor.play(id.clip());
        apply_loco_vector(id.loco(), actor);
        self.roll_auto_switch();
    }

    fn index_of(id: BehaviorId) -> usize {
        BEHAVIOR_ORDER.iter().position(|&b| b == id).unwrap_or(0)
    }

    fn roll_auto_switch(&mut self) {
        let span = AUTO_SWITCH_MAX_MS - AUTO_SWITCH_MIN_MS + 1;
        self.switch_remain_ms = AUTO_SWITCH_MIN_MS + self.rng.next_u32() % span;
    }

    fn push_recent(&mut self, id: BehaviorId) {
        if (self.recent_n as usize) < self.recent.len() {
            self.recent[self.recent_n as usize] = id;
            self.recent_n += 1;
            return;
        }
        self.recent[0] = self.recent[1];
        self.recent[1] = self.recent[2];
        self.recent[2] = id;
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

    /// Empty-space tap: 15% [`BehaviorId::FlipFacing`], otherwise a uniform
    /// other behavior (never FlipFacing in that 85%, so the flip share stays 15%).
    pub fn cycle_empty_tap(&mut self, actor: &mut Actor, entropy: u32) {
        self.chain = None;
        self.rng.mix(entropy);
        let n = BEHAVIOR_ORDER.len();
        if n <= 1 {
            return;
        }
        let offer_flip = self.current != BehaviorId::FlipFacing;
        if offer_flip && self.rng.next_u32() % 100 < EMPTY_FLIP_PCT {
            self.switch(
                actor,
                BehaviorId::FlipFacing,
                Self::index_of(BehaviorId::FlipFacing),
            );
            return;
        }
        let mut eligible = 0usize;
        for &id in BEHAVIOR_ORDER {
            if id == self.current {
                continue;
            }
            if offer_flip && id == BehaviorId::FlipFacing {
                continue;
            }
            eligible += 1;
        }
        if eligible == 0 {
            self.cycle_random(actor, entropy);
            return;
        }
        let pick = self.rng.next_u32() as usize % eligible;
        let mut i = 0usize;
        for &id in BEHAVIOR_ORDER {
            if id == self.current {
                continue;
            }
            if offer_flip && id == BehaviorId::FlipFacing {
                continue;
            }
            if i == pick {
                self.switch(actor, id, Self::index_of(id));
                return;
            }
            i += 1;
        }
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
        let retrigger = matches!(first.loco(), Loco::Jump | Loco::JumpForward);
        if first != self.current || steps.len() > 1 || retrigger {
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
        apply_loco(self.current.loco(), actor, dt, &mut self.timer_ms)
    }
}

fn apply_loco_vector(loco: Loco, actor: &mut Actor) {
    match loco {
        Loco::WalkBounce | Loco::Knockback | Loco::JumpForward => {
            actor.apply_clip_velocity();
        }
        Loco::Jump | Loco::InPlace | Loco::Search => {
            actor.vx = 0;
        }
    }
    if matches!(loco, Loco::Jump | Loco::JumpForward) {
        let rise = match loco {
            Loco::JumpForward => JUMP_FORWARD_RISE,
            _ => {
                let apex = geometry::jump_apex_foot_y(crate::DISPLAY_HEIGHT as i32);
                (floor_y() - apex).max(1)
            }
        };
        actor.vy = -collision::jump_speed(rise);
    }
}

fn apply_loco(loco: Loco, actor: &mut Actor, dt_ms: u32, timer_ms: &mut u32) -> bool {
    match loco {
        Loco::InPlace => {
            actor.vx = 0;
            actor.advance(dt_ms)
        }
        Loco::WalkBounce | Loco::Knockback => {
            actor.apply_clip_velocity();
            actor.advance(dt_ms)
        }
        Loco::Jump => {
            actor.vx = 0;
            actor.advance(dt_ms)
        }
        Loco::JumpForward => {
            actor.apply_clip_velocity();
            actor.advance(dt_ms)
        }
        Loco::Search => {
            *timer_ms = timer_ms.saturating_add(dt_ms) % (FACE_PAUSE_MS * FACE_STEPS);
            let step = *timer_ms / FACE_PAUSE_MS;
            actor.facing_left = step % 2 == 0;
            actor.vx = 0;
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
            Some(k) if k.is_vertical()
        )
    {
        return STICKMAN_EDGE;
    }
    if event == Event::Collision && ctx.collision == Some(CollisionKind::Model) {
        return STICKMAN_MODEL;
    }
    match (id, event) {
        (BehaviorId::Walking, Event::BehaviorFinished) => &[
            (WALK, 66),
            (IDLE, 8),
            (TALK, 4),
            (JUMP, 6),
            (JUMP_FWD, 6),
            (CROUCH, 5),
            (CRAWL, 5),
        ],
        (BehaviorId::Walking, Event::Collision) => &[(WALK, 40), (IDLE, 30), (KNOCKBACK, 30)],
        (BehaviorId::Jumping, Event::BehaviorFinished) => {
            &[(WALK, 35), (JUMP, 20), (JUMP_FWD, 20), (IDLE, 25)]
        }
        (BehaviorId::JumpForward, Event::BehaviorFinished) => {
            &[(WALK, 40), (JUMP_FWD, 30), (JUMP, 10), (IDLE, 20)]
        }
        (BehaviorId::Crawling, Event::BehaviorFinished) => &[(CRAWL, 50), (CROUCH, 25), (WALK, 25)],
        (BehaviorId::SwordStab, Event::BehaviorFinished) => &[(SWORD_STANCE, 80), (SWORD_STAB, 20)],
        (BehaviorId::SwordCrouchStab, Event::BehaviorFinished) => {
            &[(SWORD_CROUCH_STANCE, 80), (SWORD_CROUCH_STAB, 20)]
        }
        (BehaviorId::Knockback, Event::BehaviorFinished) => {
            &[(WALK, 50), (IDLE, 30), (KNOCKBACK, 20)]
        }
        (BehaviorId::Tumbling, Event::BehaviorFinished) => &[(WALK, 20), (IDLE, 10), (TUMBLE, 70)],
        (BehaviorId::FlipFacing, Event::BehaviorFinished) => &[(WALK, 70), (IDLE, 20), (FLIP, 10)],
        (BehaviorId::FlipFacing, Event::Collision) => STICKMAN_EDGE,
        (_, Event::Falling) => STICKMAN_FALL,
        (_, Event::Collision) => STICKMAN_COLLIDE,
        (_, Event::Tap) => STICKMAN_TAP,
        (_, Event::BehaviorFinished) => STICKMAN_STAY_WALK,
    }
}

const WALK: &[BehaviorId] = &[BehaviorId::Walking];
const IDLE: &[BehaviorId] = &[BehaviorId::Idle];
const JUMP: &[BehaviorId] = &[BehaviorId::Jumping];
const JUMP_FWD: &[BehaviorId] = &[BehaviorId::JumpForward];
const CROUCH: &[BehaviorId] = &[BehaviorId::Crouching];
const CRAWL: &[BehaviorId] = &[BehaviorId::Crawling];
const KNOCKBACK: &[BehaviorId] = &[BehaviorId::Knockback];
const TUMBLE: &[BehaviorId] = &[BehaviorId::Tumbling];
const FLIP: &[BehaviorId] = &[BehaviorId::FlipFacing];
const SWORD_STANCE: &[BehaviorId] = &[BehaviorId::SwordStance];
const SWORD_STAB: &[BehaviorId] = &[BehaviorId::SwordStab];
const SWORD_CROUCH_STANCE: &[BehaviorId] = &[BehaviorId::SwordCrouchStance];
const SWORD_CROUCH_STAB: &[BehaviorId] = &[BehaviorId::SwordCrouchStab];
const SEARCH: &[BehaviorId] = &[BehaviorId::Searching];
const BEG: &[BehaviorId] = &[BehaviorId::Begging];
const TALK: &[BehaviorId] = &[BehaviorId::Talking];
/// Tap / authored chain: reverse facing, then walk. Edge collisions use [`WALK`]
/// instead — the bounce already turned the vector (and facing).
#[allow(dead_code)]
const FLIP_THEN_WALK: &[BehaviorId] = &[BehaviorId::FlipFacing, BehaviorId::Walking];

const STICKMAN_COLLIDE: &[WeightedChain] = &[(WALK, 40), (IDLE, 30), (KNOCKBACK, 30)];

/// Box / model: hop onto the lid; leftover weight is bounce-and-walk / knockback.
const STICKMAN_MODEL: &[WeightedChain] = &[(JUMP_FWD, 80), (WALK, 14), (KNOCKBACK, 6)];

/// Screen edges: the vector already bounced and facing follows it. Tables pick
/// whether to keep walking, stop, or knockback along that heading.
const STICKMAN_EDGE: &[WeightedChain] = &[(WALK, 40), (IDLE, 30), (KNOCKBACK, 30)];

const STICKMAN_TAP: &[WeightedChain] = &[
    (FLIP, 15),
    (WALK, 7),
    (IDLE, 6),
    (TALK, 6),
    (JUMP, 7),
    (JUMP_FWD, 7),
    (CROUCH, 6),
    (CRAWL, 6),
    (SEARCH, 6),
    (BEG, 6),
    (SWORD_STANCE, 6),
    (SWORD_STAB, 6),
    (SWORD_CROUCH_STANCE, 4),
    (SWORD_CROUCH_STAB, 4),
    (KNOCKBACK, 4),
    (TUMBLE, 4),
];

const STICKMAN_STAY_WALK: &[WeightedChain] = &[(WALK, 1)];

/// Walk off a lid: keep a grounded clip; jump locos would restart a hop in air.
const STICKMAN_FALL: &[WeightedChain] = &[(WALK, 50), (IDLE, 30), (CROUCH, 20)];

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
    fn empty_tap_never_stays_on_current() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let n = BEHAVIOR_ORDER.len();
        for i in 0..n * 20 {
            let prev = mgr.index;
            mgr.cycle_empty_tap(&mut actor, i as u32);
            assert_ne!(mgr.index, prev);
            assert_eq!(actor.clip, BEHAVIOR_ORDER[mgr.index].clip());
        }
    }

    #[test]
    fn empty_tap_flip_facing_is_about_15_percent() {
        let mut flips = 0u32;
        const N: u32 = 8000;
        for i in 0..N {
            let mut mgr = BehaviorManager::new();
            let mut actor = Actor::default();
            assert_ne!(mgr.current(), BehaviorId::FlipFacing);
            mgr.cycle_empty_tap(&mut actor, i.wrapping_mul(0x9E37_79B9));
            if mgr.current() == BehaviorId::FlipFacing {
                flips += 1;
            }
        }
        let pct = flips * 100 / N;
        assert!(
            (12..=18).contains(&pct),
            "empty-tap FlipFacing {flips}/{N} = {pct}%, expected ~15%"
        );
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
        assert_eq!(actor.vx, actor.clip_vx());
        assert!(actor.vx > 0);
        let chained = mgr.on_event(&mut actor, Event::BehaviorFinished, EventCtx::default(), 1);
        assert!(!chained);
    }

    #[test]
    fn idle_zeros_the_travel_vector() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        actor.apply_clip_velocity();
        assert!(actor.vx != 0);
        mgr.begin_chain(&mut actor, IDLE);
        assert_eq!(mgr.current, BehaviorId::Idle);
        assert_eq!(actor.vx, 0);
    }

    #[test]
    fn walk_faces_along_the_travel_vector() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        mgr.begin_chain(&mut actor, IDLE);
        actor.facing_left = true;
        mgr.begin_chain(&mut actor, WALK);
        assert!(actor.vx < 0);
        assert!(actor.facing_left);
        mgr.begin_chain(&mut actor, IDLE);
        actor.facing_left = false;
        mgr.begin_chain(&mut actor, WALK);
        assert!(actor.vx > 0);
        assert!(!actor.facing_left);
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
        mgr.cycle_next(&mut actor);
        assert_eq!(mgr.current, BehaviorId::Crouching);
        mgr.update((mgr.switch_remain_ms - 1) as u64, &mut actor);
        assert_eq!(mgr.switch_remain_ms, 1);
        mgr.cycle_random(&mut actor, 7);
        assert_auto_switch_in_range(mgr.switch_remain_ms);
    }

    #[test]
    fn jump_forward_clip_travels_unlike_vertical_jump() {
        assert_eq!(BehaviorId::JumpForward.clip(), ClipId::JumpForward);
        assert_eq!(BehaviorId::Jumping.clip(), ClipId::Jump);
        assert_ne!(BehaviorId::Jumping.loco(), BehaviorId::JumpForward.loco());
        assert_eq!(
            library::clip(ClipId::JumpForward).travel_dx,
            library::clip(ClipId::Walk).travel_dx
        );
        assert_eq!(library::clip(ClipId::Jump).travel_dx, 0);
    }

    #[test]
    fn jump_forward_sets_upward_and_forward_vector() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        actor.facing_left = false;
        mgr.switch(
            &mut actor,
            BehaviorId::JumpForward,
            BehaviorManager::index_of(BehaviorId::JumpForward),
        );
        assert_eq!(actor.vy, -collision::jump_speed(JUMP_FORWARD_RISE));
        assert_eq!(actor.vx, actor.clip_vx());
        assert!(actor.vx > 0);

        actor.facing_left = true;
        actor.apply_clip_velocity();
        assert!(actor.vx < 0);
    }

    #[test]
    fn jump_forward_travels_in_facing_direction() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        actor.facing_left = true;
        let start_x = actor.x;
        mgr.switch(
            &mut actor,
            BehaviorId::JumpForward,
            BehaviorManager::index_of(BehaviorId::JumpForward),
        );
        let period = library::clip(ClipId::JumpForward).duration_ms as u32;
        actor.integrate(period);
        assert_eq!(start_x - actor.x, actor.vx.abs() * period as i32 / 1000);
        assert!(actor.vx < 0);
    }

    #[test]
    fn vertical_jump_does_not_travel() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let start_x = actor.x;
        mgr.switch(
            &mut actor,
            BehaviorId::Jumping,
            BehaviorManager::index_of(BehaviorId::Jumping),
        );
        let period = library::clip(ClipId::Jump).duration_ms as u32;
        mgr.update(period as u64, &mut actor);
        actor.integrate(period);
        assert_eq!(actor.x, start_x);
        assert_eq!(actor.vx, 0);
        assert!(actor.vy < 0);
    }

    #[test]
    fn crawling_uses_walk_bounce_from_crouch_clip_family() {
        assert_eq!(BehaviorId::Crawling.clip(), ClipId::Crawl);
        assert_eq!(BehaviorId::Crawling.loco(), Loco::WalkBounce);
        assert_eq!(BehaviorId::Crouching.loco(), Loco::InPlace);
        assert!(library::clip(ClipId::Crawl).travel_dx > 0);
        assert_eq!(library::clip(ClipId::Crouch).travel_dx, 0);
    }

    #[test]
    fn crawling_travels_on_floor_along_facing() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        actor.facing_left = false;
        let start_x = actor.x;
        let start_y = actor.y;
        mgr.switch(
            &mut actor,
            BehaviorId::Crawling,
            BehaviorManager::index_of(BehaviorId::Crawling),
        );
        let period = library::clip(ClipId::Crawl).duration_ms as u32;
        let vx = actor.vx;
        assert!(vx > 0);
        actor.integrate(period);
        assert_eq!(actor.y, start_y);
        assert_eq!(actor.x - start_x, vx * period as i32 / 1000);

        actor.facing_left = true;
        actor.apply_clip_velocity();
        let mid_x = actor.x;
        let vx = actor.vx;
        actor.integrate(period);
        assert_eq!(mid_x - actor.x, vx.abs() * period as i32 / 1000);
        assert_eq!(actor.y, start_y);
    }

    #[test]
    fn walk_loco_does_not_change_y() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        let y0 = floor_y() - 24;
        actor.y = y0;
        mgr.update(50, &mut actor);
        assert_eq!(actor.y, y0);
    }

    #[test]
    fn talking_reuses_idle_clip() {
        assert_eq!(BehaviorId::Talking.clip(), ClipId::Idle);
        assert_eq!(BehaviorId::Talking.loco(), Loco::InPlace);
        assert_eq!(BehaviorId::Talking.name(), "Talking");
        assert_ne!(BehaviorId::Talking, BehaviorId::Idle);
    }

    #[test]
    fn talking_history_is_the_last_three_names() {
        let mut mgr = BehaviorManager::new();
        let mut actor = Actor::default();
        mgr.cycle_next(&mut actor);
        mgr.cycle_next(&mut actor);
        mgr.cycle_next(&mut actor);
        assert_eq!(mgr.current, BehaviorId::JumpForward);
        mgr.switch(
            &mut actor,
            BehaviorId::Talking,
            BehaviorManager::index_of(BehaviorId::Talking),
        );
        let mut buf = [0u8; 64];
        let n = mgr.write_recent_names(&mut buf);
        assert_eq!(&buf[..n], b"Idle\nJumping\nJumpForward");
        assert!(mgr.is_talking());
        assert_eq!(actor.clip, ClipId::Idle);
        assert_eq!(actor.vx, 0);
    }
}
