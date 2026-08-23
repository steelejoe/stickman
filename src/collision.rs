//! Configurable AABB collision against models, display edges, and the baseline.
//!
//! Contacts fire on **enter** (new contact this tick) so a lasting overlap
//! does not re-trigger every frame. Facing and other reactions are owned by
//! behavior tables; this module only reports hits and keeps bodies on-screen.

use crate::stickman::ir::Actor;
use embedded_graphics::geometry::Point;
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

/// Display and floor used as colliders.
#[derive(Clone, Copy, Debug)]
pub struct World {
    pub width: i32,
    pub height: i32,
    pub baseline_y: i32,
}

/// How close feet may sit above a surface and still count as supported.
pub const LAND_SLOP: i32 = 2;
/// Downward acceleration while falling (px/s²).
const GRAVITY_PX_S2: i32 = 900;
/// Cap on fall speed (px/s).
const TERMINAL_PX_S: i32 = 360;

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

    pub fn models_overlap(&self, i: usize, j: usize) -> bool {
        self.pairs & pair_bit(i, j) != 0
    }
}

/// Who entered a contact this tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct CollisionHits {
    pub entered: u8,
    pub model_enter: bool,
    pub kind: [Option<CollisionKind>; MAX_BODIES],
}

impl CollisionHits {
    pub fn body_entered(self, i: usize) -> bool {
        self.entered & (1 << i) != 0
    }

    fn mark(&mut self, i: usize, kind: CollisionKind) {
        self.entered |= 1 << i;
        if kind == CollisionKind::Model {
            self.model_enter = true;
            self.kind[i] = Some(CollisionKind::Model);
        } else if self.kind[i].is_none() {
            self.kind[i] = Some(kind);
        }
    }
}

/// Apply world and model collisions. `bodies` are (actor, unpadded hitbox).
///
/// Same-layer models are tested against each other. At most 4 bodies
/// are considered. Left/right edges always separate so walkers stay on screen.
pub fn resolve(
    bodies: &mut [(&mut Actor, Rectangle)],
    mem: &mut ContactMemory,
    world: World,
) -> CollisionHits {
    let n = bodies.len().min(MAX_BODIES);
    let mut hits = CollisionHits::default();

    for i in 0..n {
        for j in (i + 1)..n {
            let same_layer = bodies[i].0.layer == bodies[j].0.layer;
            let overlap = same_layer && rects_overlap(bodies[i].1, bodies[j].1);
            let bit = pair_bit(i, j);
            let was = mem.pairs & bit != 0;
            // Off the default floor (jumping or on a raised top): pass through
            // model sides so a hop can land on the lid instead of bumping it.
            let off_floor = bodies[i].0.y + LAND_SLOP < world.baseline_y
                || bodies[j].0.y + LAND_SLOP < world.baseline_y;
            if overlap && !was && !off_floor {
                hits.mark(i, CollisionKind::Model);
                hits.mark(j, CollisionKind::Model);
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
            i,
            &mut hits,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeRight,
            right,
            prev & F_RIGHT != 0,
            i,
            &mut hits,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeTop,
            top,
            prev & F_TOP != 0,
            i,
            &mut hits,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::EdgeBottom,
            bottom,
            prev & F_BOTTOM != 0,
            i,
            &mut hits,
        );
        enter_edge(
            bodies[i].0,
            hit,
            world,
            CollisionKind::Baseline,
            baseline,
            prev & F_BASE != 0,
            i,
            &mut hits,
        );

        mem.edges[i] = (u8::from(left) * F_LEFT)
            | (u8::from(right) * F_RIGHT)
            | (u8::from(top) * F_TOP)
            | (u8::from(bottom) * F_BOTTOM)
            | (u8::from(baseline) * F_BASE);
    }
    hits
}

pub fn contains_point(r: Rectangle, p: Point) -> bool {
    p.x >= r.top_left.x && p.y >= r.top_left.y && p.x < max_x(r) && p.y < max_y(r)
}

/// Highest surface under `feet` (smallest y). The default floor is used when
/// no model top spans `feet_x` at or below the feet.
pub fn support_y(feet_x: i32, feet_y: i32, platforms: &[Rectangle], floor_y: i32) -> i32 {
    let mut best = floor_y;
    for p in platforms {
        if feet_x < p.top_left.x || feet_x >= max_x(*p) {
            continue;
        }
        let top = p.top_left.y;
        if top >= feet_y - LAND_SLOP && top < best {
            best = top;
        }
    }
    best
}

/// Accelerate `y` toward `support_y` (screen +Y down). `vy` is px/s.
pub fn apply_gravity(y: i32, vy: &mut i32, support_y: i32, dt_ms: u32) -> i32 {
    if y >= support_y {
        *vy = 0;
        return support_y;
    }
    let dt = dt_ms as i32;
    *vy = (*vy + GRAVITY_PX_S2 * dt / 1000).min(TERMINAL_PX_S);
    let dy = (*vy * dt / 1000).max(1);
    let next = y + dy;
    if next >= support_y {
        *vy = 0;
        support_y
    } else {
        next
    }
}

fn enter_edge(
    actor: &mut Actor,
    hit: Rectangle,
    world: World,
    kind: CollisionKind,
    touching: bool,
    was: bool,
    index: usize,
    hits: &mut CollisionHits,
) {
    if touching {
        if matches!(kind, CollisionKind::EdgeLeft | CollisionKind::EdgeRight) {
            separate_from_edge(actor, hit, world, kind);
        }
        if !was {
            hits.mark(index, kind);
        }
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
    fn left_edge_enter_separates_without_flipping() {
        let mut a = actor(5, true);
        let mut mem = ContactMemory::new();
        let hits = resolve(&mut [(&mut a, rect(-4, 50, 20, 20))], &mut mem, world());
        assert!(a.facing_left);
        assert_eq!(a.x, 9);
        assert!(hits.body_entered(0));
        assert_eq!(hits.kind[0], Some(CollisionKind::EdgeLeft));
        resolve(&mut [(&mut a, rect(0, 50, 20, 20))], &mut mem, world());
        assert!(a.facing_left);
        assert_eq!(a.x, 9);
    }

    #[test]
    fn right_edge_enter_separates_without_flipping() {
        let mut a = actor(190, false);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(190, 50, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.x, 180);
    }

    #[test]
    fn right_edge_stay_still_separates() {
        let mut a = actor(190, false);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(190, 50, 20, 20))], &mut mem, world());
        assert_eq!(a.x, 180);
        a.x = 210;
        let hits = resolve(&mut [(&mut a, rect(210, 50, 20, 20))], &mut mem, world());
        assert!(!hits.body_entered(0));
        assert_eq!(a.x, 180);
    }

    #[test]
    fn top_and_bottom_default_do_not_separate() {
        let mut a = actor(50, false);
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(40, -2, 20, 20))], &mut mem, world());
        assert_eq!(a.y, 80);
        resolve(&mut [(&mut a, rect(40, 90, 20, 20))], &mut mem, world());
        assert_eq!(a.y, 80);
    }

    #[test]
    fn model_enter_reports_both_stay_does_not() {
        let mut a = actor(40, false);
        let mut b = actor(60, true);
        let mut mem = ContactMemory::new();
        let ha = rect(30, 60, 20, 20);
        let hb = rect(40, 60, 20, 20);
        let hits = resolve(&mut [(&mut a, ha), (&mut b, hb)], &mut mem, world());
        assert!(!a.facing_left);
        assert!(b.facing_left);
        assert!(hits.model_enter);
        assert!(hits.body_entered(0));
        assert!(hits.body_entered(1));
        let stay = resolve(&mut [(&mut a, ha), (&mut b, hb)], &mut mem, world());
        assert!(!stay.model_enter);
        assert!(!stay.body_entered(0));
        assert!(mem.models_overlap(0, 1));
    }

    #[test]
    fn different_layers_do_not_collide() {
        let mut a = actor(40, false);
        let mut b = actor(60, true);
        b.layer = LayerId::Foreground;
        let mut mem = ContactMemory::new();
        let hits = resolve(
            &mut [
                (&mut a, rect(30, 60, 20, 20)),
                (&mut b, rect(40, 60, 20, 20)),
            ],
            &mut mem,
            world(),
        );
        assert!(!hits.model_enter);
        assert!(!a.facing_left);
        assert!(b.facing_left);
    }

    #[test]
    fn contains_point_matches_half_open_rect() {
        let r = rect(10, 20, 5, 5);
        assert!(contains_point(r, Point::new(10, 20)));
        assert!(contains_point(r, Point::new(14, 24)));
        assert!(!contains_point(r, Point::new(15, 20)));
        assert!(!contains_point(r, Point::new(10, 25)));
    }

    #[test]
    fn support_y_defaults_to_floor() {
        assert_eq!(support_y(50, 80, &[], 80), 80);
        let platform = rect(40, 50, 20, 30);
        assert_eq!(support_y(10, 80, &[platform], 80), 80);
    }

    #[test]
    fn support_y_uses_model_top_when_feet_span_it() {
        let platform = rect(40, 50, 20, 30);
        assert_eq!(support_y(50, 50, &[platform], 80), 50);
        assert_eq!(support_y(50, 48, &[platform], 80), 50);
        // Already below the lid: fall through to the default floor.
        assert_eq!(support_y(50, 70, &[platform], 80), 80);
    }

    #[test]
    fn gravity_pulls_down_to_support_and_stops() {
        let mut vy = 0;
        let mut y = 40;
        for _ in 0..40 {
            y = apply_gravity(y, &mut vy, 80, 33);
        }
        assert_eq!(y, 80);
        assert_eq!(vy, 0);
    }

    #[test]
    fn airborne_overlap_does_not_mark_model() {
        let mut a = actor(40, false);
        a.y = 50;
        let mut b = actor(60, true);
        let mut mem = ContactMemory::new();
        let hits = resolve(
            &mut [
                (&mut a, rect(30, 40, 20, 30)),
                (&mut b, rect(40, 55, 20, 20)),
            ],
            &mut mem,
            world(),
        );
        assert!(!hits.model_enter);
        assert!(mem.models_overlap(0, 1));
    }
}
