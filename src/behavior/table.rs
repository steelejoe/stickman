//! Kind × current behavior × event → weighted next behavior.
//!
//! Lists live next to each kind to avoid a module cycle. This module re-exports
//! them and holds the cross-kind table tests.

pub use crate::behavior::box_beh::{box_weights, BOX_IDLE_COLLIDE};
pub use crate::behavior::plugin::stickman_weights;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::box_beh::BoxBehaviorId;
    use crate::behavior::event::{Event, EventCtx};
    use crate::behavior::plugin::BehaviorId;
    use crate::collision::CollisionKind;

    fn max_item<T: Copy + PartialEq>(rows: &[(T, u16)]) -> T {
        rows.iter()
            .max_by_key(|(_, w)| *w)
            .map(|(id, _)| *id)
            .unwrap()
    }

    fn max_chain(rows: &[crate::behavior::plugin::WeightedChain]) -> &'static [BehaviorId] {
        max_item(rows)
    }

    #[test]
    fn box_idle_collision_favors_do_nothing() {
        assert_eq!(
            max_item(box_weights(BoxBehaviorId::Idle, Event::Collision)),
            BoxBehaviorId::Idle
        );
        assert_eq!(BOX_IDLE_COLLIDE[0], (BoxBehaviorId::Idle, 80));
    }

    #[test]
    fn box_loop_finished_favors_idle() {
        assert_eq!(
            max_item(box_weights(BoxBehaviorId::Sliding, Event::BehaviorFinished)),
            BoxBehaviorId::Idle
        );
        assert_eq!(
            max_item(box_weights(BoxBehaviorId::Rolling, Event::BehaviorFinished)),
            BoxBehaviorId::Idle
        );
        assert_eq!(
            max_item(box_weights(BoxBehaviorId::Shudder, Event::BehaviorFinished)),
            BoxBehaviorId::Idle
        );
    }

    #[test]
    fn walking_finished_favors_walk() {
        assert_eq!(
            max_chain(stickman_weights(
                BehaviorId::Walking,
                Event::BehaviorFinished,
                EventCtx::default(),
            )),
            &[BehaviorId::Walking][..]
        );
    }

    #[test]
    fn walking_collision_is_flip_then_walk_or_knockback() {
        let rows = stickman_weights(BehaviorId::Walking, Event::Collision, EventCtx::default());
        assert!(rows
            .iter()
            .any(|(c, _)| *c == [BehaviorId::FlipFacing, BehaviorId::Walking]));
        assert!(rows.iter().any(|(c, _)| *c == [BehaviorId::Knockback]));
        assert!(!rows.iter().any(|(c, _)| {
            c.len() >= 2 && c[0] == BehaviorId::FlipFacing && c[1] == BehaviorId::Knockback
        }));
    }

    #[test]
    fn walking_edge_collision_is_only_flip_then_walk() {
        let ctx = EventCtx {
            collision: Some(CollisionKind::EdgeRight),
            ..EventCtx::default()
        };
        let rows = stickman_weights(BehaviorId::Walking, Event::Collision, ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].0,
            &[BehaviorId::FlipFacing, BehaviorId::Walking][..]
        );
    }
}
