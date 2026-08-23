//! World events that drive per-entity probability tables.

use crate::collision::CollisionKind;

/// Something that happened to one entity this tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    /// Hit-test tap on this entity's hitbox.
    Tap,
    /// New contact, or still overlapping after a looping clip finished.
    Collision,
    /// A looping clip completed one cycle. Held / `Once` poses never emit this.
    BehaviorFinished,
    /// Left a raised baseline; gravity is pulling toward the default floor.
    Falling,
}

/// Extra data for a table roll (mostly collision).
#[derive(Clone, Copy, Debug, Default)]
pub struct EventCtx {
    pub collision: Option<CollisionKind>,
    pub other_x: Option<i32>,
    pub other_facing_left: Option<bool>,
}

/// xorshift32; state is never zero.
pub struct Rng32 {
    state: u32,
}

impl Rng32 {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    pub fn mix(&mut self, entropy: u32) {
        self.state ^= entropy.wrapping_mul(0x9E37_79B9);
        if self.state == 0 {
            self.state = 1;
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = if x == 0 { 1 } else { x };
        self.state
    }

    pub fn pick<T: Copy>(&mut self, entries: &[(T, u16)]) -> T {
        pick_weighted(entries, self.next_u32())
    }
}

/// Weighted pick. `roll` is any u32; empty tables panic in debug (authoring bug).
pub fn pick_weighted<T: Copy>(entries: &[(T, u16)], roll: u32) -> T {
    debug_assert!(!entries.is_empty(), "empty probability table");
    let total: u32 = entries.iter().map(|e| e.1 as u32).sum();
    if total == 0 {
        return entries[0].0;
    }
    let mut r = roll % total;
    for &(item, w) in entries {
        if r < w as u32 {
            return item;
        }
        r -= w as u32;
    }
    entries[0].0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_weighted_sure_thing() {
        let entries = [('a', 0), ('b', 10), ('c', 0)];
        for roll in 0..32 {
            assert_eq!(pick_weighted(&entries, roll), 'b');
        }
    }

    #[test]
    fn pick_weighted_splits_range() {
        let entries = [('a', 2), ('b', 2)];
        assert_eq!(pick_weighted(&entries, 0), 'a');
        assert_eq!(pick_weighted(&entries, 1), 'a');
        assert_eq!(pick_weighted(&entries, 2), 'b');
        assert_eq!(pick_weighted(&entries, 3), 'b');
    }
}
