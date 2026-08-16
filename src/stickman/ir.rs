//! Flash-resident motion IR: a species rig plus clips that key its bones.
//!
//! Trees stay in `.rodata`. RAM is [`Actor`] (tiny) plus one reused
//! [`PoseScratch`] in [`crate::game::Game`]. Clips interpolate joint angles;
//! world logic (bounce, facing, jump height) stays in Rust behaviors.

use crate::layer::LayerId;
use crate::stickman::geometry::floor_y;
use crate::DISPLAY_WIDTH;

/// Which clip an [`Actor`] is playing. Data lives in [`crate::stickman::library`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipId {
    Walk,
    Idle,
    Jump,
    Crouch,
    Beg,
    SwordStance,
    SwordStab,
    SwordCrouchStance,
    SwordCrouchStab,
    Knockback,
    Tumble,
}

/// Compile-time cap for one species. Scratch is sized to this, not clip count.
pub const MAX_BONES: usize = 16;

/// Transform-only or drawable attachment on a bone tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoneKind {
    /// Joint / socket. Occupies the hierarchy; nothing is stroked.
    Joint,
    /// Segment from parent tip (this origin) to this tip.
    Line,
    /// Circle centered on this tip (`diameter` matches embedded-graphics).
    Circle { diameter: u32 },
}

/// One bone in a species. Angles are absolute degrees from vertical
/// (0 = down / +Y), same convention as the old FK renderer.
#[derive(Clone, Copy, Debug)]
pub struct Bone {
    /// Parent index, or -1 for the root (feet).
    pub parent: i8,
    pub length: i16,
    pub rest_deg: i16,
    pub kind: BoneKind,
    pub visible: bool,
}

/// Shared drawing for every clip of a figure.
#[derive(Clone, Copy, Debug)]
pub struct Species {
    pub bones: &'static [Bone],
}

/// Keyframe interpolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interp {
    Lerp,
    Hold,
}

/// A keyed numeric channel on one bone.
#[derive(Clone, Copy, Debug)]
pub struct Track {
    pub bone: u8,
    pub prop: Prop,
    pub keys: &'static [Key],
}

/// Bone (or clip-level) property a track may drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    /// Absolute degrees from vertical.
    Rot,
    /// Bone length in pixels.
    Len,
    /// Root visual offset X (facing-space, before mirror).
    Tx,
    /// Root visual offset Y (screen +Y down). Jump height can live here or on `Actor.y`.
    Ty,
    /// 0 = hidden, 1 = drawn. Sampled as hold.
    Visible,
    /// Body spin in degrees about the hip tip (knockback / tumble).
    Spin,
}

#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub t_ms: u16,
    pub value: i16,
    pub interp: Interp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Once,
    Loop,
}

/// How [`Prop::Spin`] is signed from facing (matches the old roll modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spin {
    None,
    /// Facing right → negate (front-facing tuck).
    Knockback,
    /// Facing left → negate (side-profile roll).
    Tumble,
}

/// One animation on a species. Topology is the species; this is tracks only.
#[derive(Clone, Copy, Debug)]
pub struct Clip {
    pub species: &'static Species,
    pub duration_ms: u16,
    pub loop_mode: LoopMode,
    /// Pixels of logical X per loop, along facing (negative = opposite).
    pub travel_dx: i16,
    pub spin: Spin,
    pub tracks: &'static [Track],
}

impl Clip {
    /// No motion channels and no travel — idle / held poses.
    pub fn is_static(&self) -> bool {
        self.tracks.is_empty() && self.travel_dx == 0 && self.spin == Spin::None
    }
}

/// World instance. Pose is clip + time, not a RAM copy of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actor {
    pub x: i32,
    pub y: i32,
    pub facing_left: bool,
    pub layer: LayerId,
    pub clip: ClipId,
    pub time_ms: u32,
    travel_rem: i32,
}

impl Default for Actor {
    fn default() -> Self {
        Self {
            x: (DISPLAY_WIDTH / 2) as i32,
            y: floor_y(),
            facing_left: false,
            layer: LayerId::Middle,
            clip: ClipId::Walk,
            time_ms: 0,
            travel_rem: 0,
        }
    }
}

impl Actor {
    pub fn play(&mut self, clip: ClipId) {
        self.clip = clip;
        self.time_ms = 0;
        self.travel_rem = 0;
    }

    /// Advance clip time. Static clips keep `time_ms` at 0 so idle can skip draws.
    pub fn advance(&mut self, dt_ms: u32) {
        let clip = crate::stickman::library::clip(self.clip);
        if clip.is_static() {
            self.time_ms = 0;
            return;
        }
        self.time_ms = wrap_time(self.time_ms.saturating_add(dt_ms), clip);
    }

    /// Pixels to add to `x` this tick from [`Clip::travel_dx`].
    pub fn take_travel(&mut self, dt_ms: u32) -> i32 {
        let clip = crate::stickman::library::clip(self.clip);
        if clip.travel_dx == 0 || clip.duration_ms == 0 {
            return 0;
        }
        let num = clip.travel_dx as i32 * dt_ms as i32 + self.travel_rem;
        let den = clip.duration_ms as i32;
        let steps = num / den;
        self.travel_rem = num % den;
        if self.facing_left {
            -steps
        } else {
            steps
        }
    }
}

/// Reused eval output (one per `Game`, not per actor).
#[derive(Clone, Copy, Debug)]
pub struct PoseScratch {
    pub n: usize,
    pub species: Option<&'static Species>,
    pub origin: [embedded_graphics::geometry::Point; MAX_BONES],
    pub tip: [embedded_graphics::geometry::Point; MAX_BONES],
    pub visible: [bool; MAX_BONES],
}

impl PoseScratch {
    pub const fn new() -> Self {
        Self {
            n: 0,
            species: None,
            origin: [embedded_graphics::geometry::Point::new(0, 0); MAX_BONES],
            tip: [embedded_graphics::geometry::Point::new(0, 0); MAX_BONES],
            visible: [false; MAX_BONES],
        }
    }
}

impl Default for PoseScratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Map clip time through once / loop. Idempotent.
pub fn wrap_time(time_ms: u32, clip: &Clip) -> u32 {
    let d = clip.duration_ms as u32;
    if d == 0 {
        return 0;
    }
    match clip.loop_mode {
        LoopMode::Once => time_ms.min(d),
        LoopMode::Loop => time_ms % d,
    }
}

/// Sample a track at local time `t` (ms into the clip).
pub fn sample_track(keys: &[Key], t: u16, duration: u16, looping: bool) -> i32 {
    if keys.is_empty() {
        return 0;
    }
    if t <= keys[0].t_ms {
        return keys[0].value as i32;
    }
    for pair in keys.windows(2) {
        if t < pair[1].t_ms {
            return blend(&pair[0], &pair[1], t);
        }
    }
    let last = keys[keys.len() - 1];
    if looping && duration > last.t_ms {
        let first = keys[0];
        if last.interp == Interp::Hold {
            return last.value as i32;
        }
        let t1 = last.t_ms as i32;
        let t2 = first.t_ms as i32 + duration as i32;
        let span = t2 - t1;
        if span <= 0 {
            return last.value as i32;
        }
        last.value as i32 + (first.value as i32 - last.value as i32) * (t as i32 - t1) / span
    } else {
        last.value as i32
    }
}

fn blend(a: &Key, b: &Key, t: u16) -> i32 {
    if a.interp == Interp::Hold {
        return a.value as i32;
    }
    let span = b.t_ms as i32 - a.t_ms as i32;
    if span <= 0 {
        return b.value as i32;
    }
    a.value as i32 + (b.value as i32 - a.value as i32) * (t as i32 - a.t_ms as i32) / span
}

/// Screen-edge bounce used by walk / tumble. Knockback overrides facing.
pub fn bounce_edges(actor: &mut Actor, display_width: i32, margin: i32) {
    if actor.x <= margin {
        actor.x = margin;
        actor.facing_left = false;
    } else if actor.x >= display_width - margin {
        actor.x = display_width - margin;
        actor.facing_left = true;
    }
}
