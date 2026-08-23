//! AABB collision against model edges, display borders, and floor ramps.
//!
//! Floor edges are line segments of any slope. A hit that is approaching
//! (`v · n < 0`) is reflected:
//!
//! `v' = v - (1 + e) (v · n) n`
//!
//! Ground (floor ramps, model tops) uses `e = 0` (slide along the tangent).
//! Vertical walls use `e = 1` (bounce). Entered wall/side hits become
//! [`crate::behavior::event::Event::Collision`].

use crate::stickman::geometry::{self, Segment, MAX_FLOOR_SEGS};
use crate::stickman::ir::Actor;
use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::geometry::Point;
use embedded_graphics::primitives::Rectangle;

/// What was hit. Floor/ramp contacts ground the body without an enter event;
/// tables see walls and model *sides*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionKind {
    Model,
    EdgeLeft,
    EdgeRight,
    EdgeTop,
    EdgeBottom,
}

impl CollisionKind {
    /// Screen or model face with a horizontal normal.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::EdgeLeft | Self::EdgeRight)
    }
}

/// Display, floor polyline, and screen-edge colliders.
#[derive(Clone, Copy, Debug)]
pub struct World {
    pub width: i32,
    pub height: i32,
    pub floor: [Segment; MAX_FLOOR_SEGS],
    pub floor_n: usize,
}

impl World {
    pub fn display() -> Self {
        let mut floor = [Segment::ZERO; MAX_FLOOR_SEGS];
        let floor_n = geometry::fill_floor_segments(&mut floor);
        Self {
            width: DISPLAY_WIDTH as i32,
            height: DISPLAY_HEIGHT as i32,
            floor,
            floor_n,
        }
    }

    pub fn flat(width: i32, height: i32, floor_y: i32) -> Self {
        let mut floor = [Segment::ZERO; MAX_FLOOR_SEGS];
        floor[0] = Segment::new(0, floor_y, width, floor_y);
        Self {
            width,
            height,
            floor,
            floor_n: 1,
        }
    }

    pub fn floor_y_at(self, x: i32) -> i32 {
        geometry::floor_y_at_in(&self.floor[..self.floor_n], x)
    }

    fn seg_at(self, x: i32) -> Option<Segment> {
        let n = self.floor_n;
        self.floor[..n]
            .iter()
            .enumerate()
            .find(|(i, s)| s.covers_x(x, *i + 1 == n))
            .map(|(_, s)| *s)
    }
}

/// How close feet may sit to a supporting edge and still count as grounded.
pub const LAND_SLOP: i32 = 2;
/// Downward acceleration (px/s²).
const GRAVITY_PX_S2: i32 = 900;
/// Cap on fall speed (px/s).
const TERMINAL_PX_S: i32 = 360;

const MILLI: i32 = 1000;
/// Leave the floor when the outward normal speed exceeds this (px/s · milli).
const LEAVE_VN: i32 = 80 * MILLI;
const MAX_BODIES: usize = 4;
const F_LEFT: u8 = 1 << 0;
const F_RIGHT: u8 = 1 << 1;
const F_TOP: u8 = 1 << 2;
const F_BOTTOM: u8 = 1 << 3;
const F_FLOOR: u8 = 1 << 4;

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

/// Who entered a contact this tick, plus which bodies sit on a supporting edge.
#[derive(Clone, Copy, Debug, Default)]
pub struct CollisionHits {
    pub entered: u8,
    pub grounded: u8,
    pub model_enter: bool,
    pub kind: [Option<CollisionKind>; MAX_BODIES],
    /// Outward normal of the entered hit (into free space).
    pub nx: [i8; MAX_BODIES],
    pub ny: [i8; MAX_BODIES],
}

impl CollisionHits {
    pub fn body_entered(self, i: usize) -> bool {
        self.entered & (1 << i) != 0
    }

    pub fn is_grounded(self, i: usize) -> bool {
        self.grounded & (1 << i) != 0
    }

    fn mark(&mut self, i: usize, kind: CollisionKind, nx: i8, ny: i8) {
        self.entered |= 1 << i;
        if kind == CollisionKind::Model {
            self.model_enter = true;
            self.kind[i] = Some(CollisionKind::Model);
            self.nx[i] = nx;
            self.ny[i] = ny;
        } else if self.kind[i].is_none() {
            self.kind[i] = Some(kind);
            self.nx[i] = nx;
            self.ny[i] = ny;
        }
    }

    fn ground(&mut self, i: usize) {
        self.grounded |= 1 << i;
    }
}

/// Upward launch speed (px/s) that peaks `rise` pixels above takeoff.
pub fn jump_speed(rise: i32) -> i32 {
    isqrt(2 * GRAVITY_PX_S2 * rise.max(0))
}

/// Accelerate downward. Call when the body is not on a supporting edge.
pub fn apply_gravity(vy: &mut i32, dt_ms: u32) {
    let dt = dt_ms as i32;
    *vy = (*vy + GRAVITY_PX_S2 * dt / 1000).min(TERMINAL_PX_S);
}

/// Snap feet to the floor polyline and project walk speed along the ramp tangent.
/// No-op when there is no segment at `actor.x`.
pub fn align_to_support(actor: &mut Actor, world: World) {
    let Some(seg) = world.seg_at(actor.x) else {
        return;
    };
    actor.y = seg.y_at(actor.x);
    actor.clear_remainder(false, true);
    let speed = actor.vx;
    if speed == 0 {
        actor.vy = 0;
        return;
    }
    let (tx, ty) = tangent_milli(seg);
    actor.vx = speed * tx / MILLI;
    actor.vy = speed * ty / MILLI;
    actor.sync_facing();
}

/// True when the feet are on the floor polyline (not a model lid).
pub fn on_floor_polyline(actor: &Actor, world: World) -> bool {
    (actor.y - world.floor_y_at(actor.x)).abs() <= LAND_SLOP
}

/// Apply world and model collisions. `bodies` are (actor, unpadded hitbox).
///
/// Same-layer models are tested against each other. At most 4 bodies
/// are considered. Incoming velocity picks the face: a downward vector on a
/// lid or the floor lands; a horizontal vector on a wall or model side bounces.
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

            if same_layer {
                resolve_pair(bodies, i, j, overlap, was, &mut hits);
            }

            if overlap {
                mem.pairs |= bit;
            } else {
                mem.pairs &= !bit;
            }
        }
    }

    for i in 0..n {
        resolve_world(bodies[i].0, bodies[i].1, world, i, mem, &mut hits);
    }
    hits
}

pub fn contains_point(r: Rectangle, p: Point) -> bool {
    p.x >= r.top_left.x && p.y >= r.top_left.y && p.x < max_x(r) && p.y < max_y(r)
}

fn resolve_pair(
    bodies: &mut [(&mut Actor, Rectangle)],
    i: usize,
    j: usize,
    overlap: bool,
    was: bool,
    hits: &mut CollisionHits,
) {
    let hi = bodies[i].1;
    let hj = bodies[j].1;
    // Support uses feet (`actor.y`) against the other's top edge, even when
    // the AABBs only share the contact line (standing on a lid).
    let i_on_j = land_on_top(bodies[i].0, hi, hj);
    let j_on_i = land_on_top(bodies[j].0, hj, hi);
    if i_on_j {
        snap_to_top(bodies[i].0, hi, hj);
        hits.ground(i);
    }
    if j_on_i {
        snap_to_top(bodies[j].0, hj, hi);
        hits.ground(j);
    }
    if !overlap || i_on_j || j_on_i {
        return;
    }
    // Rising through a volume: skip side faces so a hop can reach the lid.
    let i_rise = bodies[i].0.vy < 0;
    let j_rise = bodies[j].0.vy < 0;
    if i_rise || j_rise {
        return;
    }
    let (nx, ny) = side_normal(bodies[i].0.x, bodies[j].0.x);
    let vn_i = bodies[i].0.vx * nx as i32 + bodies[i].0.vy * ny as i32;
    let vn_j = bodies[j].0.vx * (-nx as i32) + bodies[j].0.vy * (-ny as i32);
    // Leaving (walk past while a wide AABB still overlaps) is not a side hit.
    // Resting overlap (vn == 0) still counts: a held sword can enter a volume
    // without a travel vector.
    if vn_i > 0 && vn_j > 0 {
        return;
    }
    if vn_i < 0 {
        bounce_actor(bodies[i].0, hi, hj, nx, ny);
    }
    if vn_j < 0 {
        bounce_actor(bodies[j].0, hj, hi, -nx, -ny);
    }
    if !was {
        hits.mark(i, CollisionKind::Model, nx, ny);
        hits.mark(j, CollisionKind::Model, -nx, -ny);
    }
}

fn resolve_world(
    actor: &mut Actor,
    hit: Rectangle,
    world: World,
    index: usize,
    mem: &mut ContactMemory,
    hits: &mut CollisionHits,
) {
    let left = hit.top_left.x <= 0;
    let right = max_x(hit) >= world.width;
    let top = hit.top_left.y <= 0;
    let bottom = max_y(hit) >= world.height;
    let on_floor = supporting_floor(actor, world);
    let prev = mem.edges[index];

    enter_vertical(
        actor,
        hit,
        world,
        CollisionKind::EdgeLeft,
        1,
        0,
        left,
        prev & F_LEFT != 0,
        index,
        hits,
    );
    enter_vertical(
        actor,
        hit,
        world,
        CollisionKind::EdgeRight,
        -1,
        0,
        right,
        prev & F_RIGHT != 0,
        index,
        hits,
    );
    if top {
        // Ceiling: inelastic stop (e = 0) on the upward vector.
        reflect(actor, 0, MILLI, 0);
        actor.y += 0 - hit.top_left.y;
        actor.clear_remainder(false, true);
        if prev & F_TOP == 0 {
            hits.mark(index, CollisionKind::EdgeTop, 0, 1);
        }
    }
    if on_floor {
        snap_to_floor(actor, world);
        hits.ground(index);
    } else if bottom {
        snap_y(actor, hit, world.height);
        hits.ground(index);
        if prev & F_BOTTOM == 0 && prev & F_FLOOR == 0 {
            hits.mark(index, CollisionKind::EdgeBottom, 0, -1);
        }
    }

    mem.edges[index] = (u8::from(left) * F_LEFT)
        | (u8::from(right) * F_RIGHT)
        | (u8::from(top) * F_TOP)
        | (u8::from(bottom) * F_BOTTOM)
        | (u8::from(on_floor) * F_FLOOR);
}

fn enter_vertical(
    actor: &mut Actor,
    hit: Rectangle,
    world: World,
    kind: CollisionKind,
    nx: i8,
    ny: i8,
    touching: bool,
    was: bool,
    index: usize,
    hits: &mut CollisionHits,
) {
    if !touching {
        return;
    }
    reflect(actor, nx as i32 * MILLI, ny as i32 * MILLI, 1);
    match kind {
        CollisionKind::EdgeLeft => actor.x += 0 - hit.top_left.x,
        CollisionKind::EdgeRight => actor.x -= max_x(hit) - world.width,
        _ => {}
    }
    actor.clear_remainder(true, false);
    if !was {
        hits.mark(index, kind, nx, ny);
    }
}

fn land_on_top(actor: &Actor, _hit: Rectangle, platform: Rectangle) -> bool {
    if actor.vy < 0 {
        return false;
    }
    let feet_x = actor.x;
    if feet_x < platform.top_left.x || feet_x >= max_x(platform) {
        return false;
    }
    let top = platform.top_left.y;
    let window = land_window(actor.vy);
    let feet = actor.y;
    feet >= top - LAND_SLOP && feet <= top + window
}

fn land_window(vy: i32) -> i32 {
    LAND_SLOP.max(vy.abs() / 30).max(1)
}

fn supporting_floor(actor: &Actor, world: World) -> bool {
    let fy = world.floor_y_at(actor.x);
    if actor.y < fy - LAND_SLOP {
        return false;
    }
    let Some(seg) = world.seg_at(actor.x) else {
        return actor.vy >= 0;
    };
    let (nx, ny) = normal_milli(seg);
    let vn = actor.vx * nx + actor.vy * ny;
    vn <= LEAVE_VN
}

fn snap_to_floor(actor: &mut Actor, world: World) {
    actor.y = world.floor_y_at(actor.x);
    if let Some(seg) = world.seg_at(actor.x) {
        let (nx, ny) = normal_milli(seg);
        reflect(actor, nx, ny, 0);
    } else {
        reflect(actor, 0, -MILLI, 0);
    }
    actor.clear_remainder(false, true);
}

fn snap_to_top(actor: &mut Actor, hit: Rectangle, platform: Rectangle) {
    snap_y(actor, hit, platform.top_left.y);
}

fn snap_y(actor: &mut Actor, _hit: Rectangle, surface_y: i32) {
    actor.y = surface_y;
    reflect(actor, 0, -MILLI, 0);
    actor.clear_remainder(false, true);
}

fn bounce_actor(actor: &mut Actor, hit: Rectangle, solid: Rectangle, nx: i8, ny: i8) {
    let vn = actor.vx * nx as i32 + actor.vy * ny as i32;
    if vn >= 0 {
        return;
    }
    reflect(actor, nx as i32 * MILLI, ny as i32 * MILLI, 1);
    if nx < 0 {
        actor.x -= max_x(hit) - solid.top_left.x;
    } else if nx > 0 {
        actor.x += max_x(solid) - hit.top_left.x;
    }
    actor.clear_remainder(true, false);
}

fn side_normal(ax: i32, bx: i32) -> (i8, i8) {
    if ax < bx {
        (-1, 0)
    } else {
        (1, 0)
    }
}

/// `v' = v - (1+e)(v·n̂)n̂`. `nx, ny` are milli-unit normal components.
fn reflect(actor: &mut Actor, nx: i32, ny: i32, e: i32) {
    let vn = actor.vx * nx + actor.vy * ny;
    if vn >= 0 {
        return;
    }
    actor.vx -= (1 + e) * vn / MILLI * nx / MILLI;
    actor.vy -= (1 + e) * vn / MILLI * ny / MILLI;
    actor.sync_facing();
}

fn normal_milli(seg: Segment) -> (i32, i32) {
    let dx = seg.x1 - seg.x0;
    let dy = seg.y1 - seg.y0;
    // (dy, -dx) points up (−Y) for a left-to-right segment.
    let mut nx = dy;
    let mut ny = -dx;
    if ny > 0 {
        nx = -nx;
        ny = -ny;
    }
    vec_milli(nx, ny)
}

fn tangent_milli(seg: Segment) -> (i32, i32) {
    let mut dx = seg.x1 - seg.x0;
    let mut dy = seg.y1 - seg.y0;
    if dx < 0 {
        dx = -dx;
        dy = -dy;
    }
    vec_milli(dx, dy)
}

fn vec_milli(x: i32, y: i32) -> (i32, i32) {
    let len = isqrt(x * x + y * y).max(1);
    (x * MILLI / len, y * MILLI / len)
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

fn isqrt(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::LayerId;
    use crate::stickman::ir::Actor;
    use embedded_graphics::geometry::{Point, Size};

    fn world() -> World {
        World::flat(200, 100, 80)
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
    fn jump_speed_matches_v_squared_eq_2gh() {
        assert_eq!(jump_speed(50), 300);
        assert_eq!(jump_speed(32), 240);
        assert_eq!(jump_speed(0), 0);
    }

    #[test]
    fn reflect_wall_flips_incoming_vx() {
        let mut a = actor(190, false);
        a.vx = 80;
        reflect(&mut a, -MILLI, 0, 1);
        assert_eq!(a.vx, -80);
        assert_eq!(a.vy, 0);
    }

    #[test]
    fn reflect_floor_kills_downward_vy() {
        let mut a = actor(50, false);
        a.vy = 120;
        reflect(&mut a, 0, -MILLI, 0);
        assert_eq!(a.vy, 0);
        assert_eq!(a.vx, 0);
    }

    #[test]
    fn reflect_leaving_does_not_flip() {
        let mut a = actor(50, false);
        a.vy = -200;
        reflect(&mut a, 0, -MILLI, 0);
        assert_eq!(a.vy, -200);
    }

    #[test]
    fn reflect_ramp_e0_slides_along_tangent() {
        let mut a = actor(10, false);
        a.vx = 100;
        a.vy = 0;
        // Up-right 45°: n ≈ (-707, -707). Incoming +X is into the ramp.
        reflect(&mut a, -707, -707, 0);
        assert!(a.vx > 0, "vx={}", a.vx);
        assert!(a.vy < 0, "vy={}", a.vy);
    }

    #[test]
    fn align_walk_projects_along_ramp() {
        let mut floor = [Segment::ZERO; MAX_FLOOR_SEGS];
        floor[0] = Segment::new(0, 40, 80, 20);
        let world = World {
            width: 200,
            height: 100,
            floor,
            floor_n: 1,
        };
        let mut a = actor(40, false);
        a.y = 40;
        a.vx = 80;
        align_to_support(&mut a, world);
        assert_eq!(a.y, floor[0].y_at(40));
        assert!(a.vx > 0);
        assert!(a.vy < 0, "up-ramp walk should have upward vy, got {}", a.vy);
    }

    #[test]
    fn resolve_snaps_to_slanted_floor() {
        let mut floor = [Segment::ZERO; MAX_FLOOR_SEGS];
        floor[0] = Segment::new(0, 40, 80, 20);
        let world = World {
            width: 200,
            height: 100,
            floor,
            floor_n: 1,
        };
        let mut a = actor(40, false);
        a.y = 32;
        a.vy = 90;
        let mut mem = ContactMemory::new();
        let hits = resolve(&mut [(&mut a, rect(30, 12, 20, 22))], &mut mem, world);
        assert!(hits.is_grounded(0));
        assert!(!hits.body_entered(0));
        assert_eq!(a.y, floor[0].y_at(40));
    }

    #[test]
    fn left_edge_enter_reflects_and_faces_the_vector() {
        let mut a = actor(5, true);
        a.vx = -40;
        let mut mem = ContactMemory::new();
        let hits = resolve(&mut [(&mut a, rect(-4, 50, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.x, 9);
        assert_eq!(a.vx, 40);
        assert!(hits.body_entered(0));
        assert_eq!(hits.kind[0], Some(CollisionKind::EdgeLeft));
        resolve(&mut [(&mut a, rect(0, 50, 20, 20))], &mut mem, world());
        assert!(!a.facing_left);
        assert_eq!(a.x, 9);
    }

    #[test]
    fn right_edge_enter_reflects_and_faces_the_vector() {
        let mut a = actor(190, false);
        a.vx = 40;
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(190, 50, 20, 20))], &mut mem, world());
        assert!(a.facing_left);
        assert_eq!(a.x, 180);
        assert_eq!(a.vx, -40);
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
    fn floor_lands_and_grounds_without_enter() {
        let mut a = actor(50, false);
        a.vy = 90;
        a.y = 78;
        let mut mem = ContactMemory::new();
        let hits = resolve(&mut [(&mut a, rect(40, 58, 20, 22))], &mut mem, world());
        assert!(hits.is_grounded(0));
        assert!(!hits.body_entered(0));
        assert_eq!(a.vy, 0);
        assert_eq!(a.y, 80);
    }

    #[test]
    fn rising_does_not_stick_to_floor() {
        let mut a = actor(50, false);
        a.y = 80;
        a.vy = -300;
        let mut mem = ContactMemory::new();
        let hits = resolve(&mut [(&mut a, rect(40, 60, 20, 20))], &mut mem, world());
        assert!(!hits.is_grounded(0));
        assert_eq!(a.vy, -300);
        assert_eq!(a.y, 80);
    }

    #[test]
    fn ceiling_stops_upward_vector() {
        let mut a = actor(50, false);
        a.y = 0;
        a.vy = -200;
        let mut mem = ContactMemory::new();
        resolve(&mut [(&mut a, rect(40, -2, 20, 20))], &mut mem, world());
        assert_eq!(a.y, 2);
        assert_eq!(a.vy, 0);
    }

    #[test]
    fn model_enter_reports_both_stay_does_not() {
        let mut a = actor(40, false);
        a.vx = 40;
        let mut b = actor(60, true);
        let mut mem = ContactMemory::new();
        let ha = rect(30, 60, 20, 20);
        let hb = rect(40, 60, 20, 20);
        let hits = resolve(&mut [(&mut a, ha), (&mut b, hb)], &mut mem, world());
        assert!(hits.model_enter);
        assert!(hits.body_entered(0));
        assert!(hits.body_entered(1));
        assert_eq!(hits.nx[0], -1);
        assert!(a.facing_left);
        assert_eq!(a.vx, -40);
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
    fn falling_onto_model_top_grounds_without_model_enter() {
        let mut a = actor(50, false);
        a.y = 48;
        a.vy = 90;
        let mut b = actor(50, true);
        b.y = 80;
        let mut mem = ContactMemory::new();
        let hits = resolve(
            &mut [
                (&mut a, rect(40, 28, 20, 22)),
                (&mut b, rect(40, 50, 20, 30)),
            ],
            &mut mem,
            world(),
        );
        assert!(!hits.model_enter);
        assert!(hits.is_grounded(0));
        assert_eq!(a.y, 50);
        assert_eq!(a.vy, 0);
    }

    #[test]
    fn gravity_accelerates_down() {
        let mut vy = 0;
        apply_gravity(&mut vy, 33);
        assert!(vy > 0);
        for _ in 0..40 {
            apply_gravity(&mut vy, 33);
        }
        assert_eq!(vy, TERMINAL_PX_S);
    }

    #[test]
    fn rising_overlap_does_not_mark_model() {
        let mut a = actor(40, false);
        a.y = 50;
        a.vy = -200;
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
        assert_eq!(a.vy, -200);
    }

    #[test]
    fn resting_overlap_still_enters() {
        let mut a = actor(40, false);
        let mut b = actor(60, true);
        let mut mem = ContactMemory::new();
        let hits = resolve(
            &mut [
                (&mut a, rect(30, 60, 40, 20)),
                (&mut b, rect(50, 50, 20, 30)),
            ],
            &mut mem,
            world(),
        );
        assert!(hits.model_enter);
        assert!(hits.body_entered(0));
        assert!(hits.body_entered(1));
        assert_eq!(a.vx, 0);
        assert_eq!(b.vx, 0);
        let stay = resolve(
            &mut [
                (&mut a, rect(30, 60, 40, 20)),
                (&mut b, rect(50, 50, 20, 30)),
            ],
            &mut mem,
            world(),
        );
        assert!(!stay.model_enter);
    }
}
