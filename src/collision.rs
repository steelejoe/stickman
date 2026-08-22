//! Configurable AABB collision against models, display edges, and the baseline.
//!
//! Responses fire on **enter** (new contact this tick) so a lasting overlap
//! does not flip facing every frame. Each [`Actor`] carries its own
//! [`CollisionPolicy`].

use crate::stickman::ir::Actor;
use embedded_graphics::primitives::Rectangle;

/// What was hit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionKind {
    Model,
    EdgeLeft,
    EdgeRight,
    EdgeTop,
    EdgeBottom,
    Baseline,
}

/// What to do when a [`CollisionKind`] is entered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollisionResponse {
    /// Ignore the contact.
    #[default]
    None,
    /// Toggle [`Actor::facing_left`]. Left/right (and top/bottom) edges also
    /// push the actor back so the hitbox sits on the display bound.
    FlipFacing,
}

/// Per-kind outcomes. Defaults match the world rules:
/// model + left/right edges flip facing; baseline + top/bottom do nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollisionPolicy {
    pub on_model: CollisionResponse,
    pub on_edge_left: CollisionResponse,
    pub on_edge_right: CollisionResponse,
    pub on_edge_top: CollisionResponse,
    pub on_edge_bottom: CollisionResponse,
    pub on_baseline: CollisionResponse,
}

impl Default for CollisionPolicy {
    fn default() -> Self {
        Self {
            on_model: CollisionResponse::FlipFacing,
            on_edge_left: CollisionResponse::FlipFacing,
            on_edge_right: CollisionResponse::FlipFacing,
            on_edge_top: CollisionResponse::None,
            on_edge_bottom: CollisionResponse::None,
            on_baseline: CollisionResponse::None,
        }
    }
}

impl CollisionPolicy {
    pub fn response(self, kind: CollisionKind) -> CollisionResponse {
        match kind {
            CollisionKind::Model => self.on_model,
            CollisionKind::EdgeLeft => self.on_edge_left,
            CollisionKind::EdgeRight => self.on_edge_right,
            CollisionKind::EdgeTop => self.on_edge_top,
            CollisionKind::EdgeBottom => self.on_edge_bottom,
            CollisionKind::Baseline => self.on_baseline,
        }
    }
}

/// Display and floor used as colliders.
#[derive(Clone, Copy, Debug)]
pub struct World {
    pub width: i32,
    pub height: i32,
    pub baseline_y: i32,
}

const MAX_BODIES: usize = 4;
const F_LEFT: u8 = 1 << 0;
const F_RIGHT: u8 = 1 << 1;
const F_TOP: u8 = 1 << 2;
const F_BOTTOM: u8 = 1 << 3;
const F_BASE: u8 = 1 << 4;

/// Previous-tick contacts so resolve can tell enter from stay.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContactMemory {
    edges: [u8; MAX_BODIES],
    pairs: u16,
}

impl ContactMemory {
    pub const fn new() -> Self {
        Self {
            edges: [0; MAX_BODIES],
            pairs: 0,
        }
    }
}

/// Apply world and model collisions. `bodies` are (actor, unpadded hitbox).
///
/// Same-layer models are tested against each other. At most 4 bodies
/// are considered.
pub fn resolve(bodies: &mut [(&mut Actor, Rectangle)], mem: &mut ContactMemory, world: World) {
    let n = bodies.len().min(MAX_BODIES);

    for i in 0..n {
        for j in (i + 1)..n {
            let same_layer = bodies[i].0.layer == bodies[j].0.layer;
            let overlap = same_layer && rects_overlap(bodies[i].1, bodies[j].1);
            let bit = pair_bit(i, j);
            let was = mem.pairs & bit != 0;
            if overlap && !was {
                apply_response(bodies[i].0, bodies[i].0.collision.on_model);
                apply_response(bodies[j].0, bodies[j].0.collision.on_model);
            }
            if overlap {
                mem.pairs |= bit;
            } else {
                mem.pairs &= !bit;
            }
        }
    }

    for i in 0..n {
        let hit = bodies[i].1;
        let left = hit.top_left.x <= 0;
        let right = max_x(hit) >= world.width;
        let top = hit.top_left.y <= 0;
        let bottom = max_y(hit) >= world.height;
        let baseline = max_y(hit) >= world.baseline_y;
        let prev = mem.edges[i];

        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeLeft,
            left,
            prev & F_LEFT != 0,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeRight,
            right,
            prev & F_RIGHT != 0,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeTop,
            top,
            prev & F_TOP != 0,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeBottom,
            bottom,
            prev & F_BOTTOM != 0,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::Baseline,
            baseline,
            prev & F_BASE != 0,
        );

        mem.edges[i] = (u8::from(left) * F_LEFT)
            | (u8::from(right) * F_RIGHT)
            | (u8::from(top) * F_TOP)
            | (u8::from(bottom) * F_BOTTOM)
            | (u8::from(baseline) * F_BASE);
    }
}

fn enter_edge(
    actor: &mut Actor,
    hit: Rectangle,
    world: World,
    kind: CollisionKind,
    touching: bool,
    was: bool,
) {
    if touching && !was {
        let response = actor.collision.response(kind);
        apply_response(actor, response);
        if response == CollisionResponse::FlipFacing {
            separate_from_edge(actor, hit, world, kind);
        }
    }
}

fn apply_response(actor: &mut Actor, response: CollisionResponse) {
    if response == CollisionResponse::FlipFacing {
        actor.facing_left = !actor.facing_left;
    }
}

fn separate_from_edge(actor: &mut Actor, hit: Rectangle, world: World, kind: CollisionKind) {
    match kind {
        CollisionKind::EdgeLeft => actor.x += 0 - hit.top_left.x,
        CollisionKind::EdgeRight => actor.x -= max_x(hit) - world.width,
        CollisionKind::EdgeTop => actor.y += 0 - hit.top_left.y,
        CollisionKind::EdgeBottom => actor.y -= max_y(hit) - world.height,
        CollisionKind::Model | CollisionKind::Baseline => {}
    }
}

fn rects_overlap(a: Rectangle, b: Rectangle) -> bool {
    a.top_left.x < max_x(b)
        && b.top_left.x < max_x(a)
        && a.top_left.y < max_y(b)
        && b.top_left.y < max_y(a)
}

fn max_x(r: Rectangle) -> i32 {
    r.top_left.x + r.size.width as i32
}

fn max_y(r: Rectangle) -> i32 {
    r.top_left.y + r.size.height as i32
}

fn pair_bit(i: usize, j: usize) -> u16 {
    1u16 << (i * MAX_BODIES + j)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::LayerId;
    use crate::stickman::ir::Actor;
    use embedded_graphics::geometry::{Point, Size};

    fn world() -> World {
        World {
            width: 200,
            height: 100,
            baseline_y: 80,
        }
    }

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    fn actor(x: i32, facing_left: bool) -> Actor {
        let mut a = Actor::default();
        a.x = x;
        a.y = 80;
        a.facing_left = facing_left;
        a
    }

    #[test]
    fn default_policy_flips_model_and_horizontal_edges() {
        let p = CollisionPolicy::default();
        assert_eq!(p.on_model, CollisionResponse::FlipFacing);
        assert_eq!(p.on_edge_left, CollisionResponse::FlipFacing);
        assert_eq!(p.on_edge_right, CollisionResponse::FlipFacing);
        assert_eq!(p.on_edge_top, CollisionResponse::None);
        assert_eq!(p.on_edge_bottom, CollisionResponse::None);
        assert_eq!(p.on_baseline, CollisionResponse::None);
    }

    #[test]
    fn left_edge_enter_flips_and_separates() {
        let mut a = actor(5, true);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(-4, 50, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.x, 9);
        resolve(&mut [(&mut a, rect(0, 50, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.x, 9);
    }

    #[test]
    fn left_edge_none_does_not_flip_or_separate() {
        let mut a = actor(5, true);
        a.collision.on_edge_left = CollisionResponse::None;
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(-4, 50, 20, 20))], &mut mem, world());
        assert!(a.facing_left);
        assert_eq!(a.x, 5);
    }

    #[test]
    fn right_edge_enter_flips_to_face_left() {
        let mut a = actor(190, false);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(190, 50, 20, 20))], &mut mem, world());
        assert!(a.facing_left);
        assert_eq!(a.x, 180);
    }

    #[test]
    fn top_and_bottom_default_do_nothing() {
        let mut a = actor(50, false);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(40, -2, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.y, 80);
        resolve(&mut [(&mut a, rect(40, 90, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.y, 80);
    }

    #[test]
    fn baseline_default_does_nothing() {
        let mut a = actor(50, true);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(40, 60, 20, 20))], &mut mem, world());
        assert!(a.facing_left);
        assert_eq!(a.y, 80);
    }

    #[test]
    fn model_enter_flips_both_stay_does_not() {
        let mut a = actor(40, false);
        let mut b = actor(60, true);
        let mut mem = ContactMemory::new();
        let ha = rect(30, 60, 20, 20);
        let hb = rect(40, 60, 20, 20);
        resolve(&mut [(&mut a, ha), (&mut b, hb)], &mut mem, world());
        assert!(a.facing_left);
        assert!(!b.facing_left);
        resolve(&mut [(&mut a, ha), (&mut b, hb)], &mut mem, world());
        assert!(a.facing_left);
        assert!(!b.facing_left);
    }

    #[test]
    fn model_policy_none_skips_that_actor() {
        let mut a = actor(40, false);
        let mut b = actor(60, true);
        b.collision.on_model = CollisionResponse::None;
        let mut mem = ContactMemory::new();
        resolve(
            &mut [
                (&mut a, rect(30, 60, 20, 20)),
                (&mut b, rect(40, 60, 20, 20)),
            ],
            &mut mem,
            world(),
        );
        assert!(a.facing_left);
        assert!(b.facing_left);
    }

    #[test]
    fn different_layers_do_not_collide() {
        let mut a = actor(40, false);
        let mut b = actor(60, true);
        b.layer = LayerId::Foreground;
        let mut mem = ContactMemory::new();
        resolve(
            &mut [
                (&mut a, rect(30, 60, 20, 20)),
                (&mut b, rect(40, 60, 20, 20)),
            ],
            &mut mem,
            world(),
        );
        assert!(!a.facing_left);
        assert!(b.facing_left);
    }
}
