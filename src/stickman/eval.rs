//! Evaluate a clip into world-space bone tips.

use crate::stickman::geometry::{rotate_point_cw, sin_cos_deg_milli};
use crate::stickman::ir::{
    sample_track, wrap_time, Actor, BoneKind, LoopMode, PoseScratch, Prop, Spin, MAX_BONES,
};
use crate::stickman::library;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::primitives::Rectangle;

/// Sample `actor`'s clip into `out` (FK + optional spin).
///
/// After FK, the pose is lifted so the body does not sit below `actor.y`
/// (the floor, or the aerial foot line while jumping).
pub fn sample(actor: &Actor, out: &mut PoseScratch) {
    let clip = library::clip(actor.clip);
    let species = clip.species;
    let n = species.bones.len().min(MAX_BONES);
    out.n = n;
    out.species = Some(species);

    let mut angle = [0i32; MAX_BONES];
    let mut length = [0i32; MAX_BONES];
    let mut vis = [false; MAX_BONES];
    let mut tx = 0i32;
    let mut ty = 0i32;
    let mut spin_deg = 0i32;

    for (i, bone) in species.bones.iter().enumerate().take(n) {
        angle[i] = bone.rest_deg as i32;
        length[i] = bone.length as i32;
        vis[i] = bone.visible;
    }

    let looping = clip.loop_mode == LoopMode::Loop;
    let t = wrap_time(actor.time_ms, clip) as u16;

    for track in clip.tracks {
        let bone = track.bone as usize;
        if bone >= n && track.prop != Prop::Tx && track.prop != Prop::Ty && track.prop != Prop::Spin
        {
            continue;
        }
        let v = sample_track(track.keys, t, clip.duration_ms, looping);
        match track.prop {
            Prop::Rot if bone < n => angle[bone] = v,
            Prop::Len if bone < n => length[bone] = v,
            Prop::Visible if bone < n => vis[bone] = v >= 1,
            Prop::Tx => tx = v,
            Prop::Ty => ty = v,
            Prop::Spin => spin_deg = v,
            _ => {}
        }
    }

    let dir: i32 = if actor.facing_left { -1 } else { 1 };
    let root = Point::new(actor.x + dir * tx, actor.y + ty);

    for i in 0..n {
        let bone = &species.bones[i];
        let origin = if bone.parent < 0 {
            root
        } else {
            let p = bone.parent as usize;
            if p < i {
                out.tip[p]
            } else {
                root
            }
        };
        out.origin[i] = origin;
        out.tip[i] = joint_tip(origin, angle[i], length[i], dir);
        out.visible[i] = vis[i];
    }

    let hip = library::HIP as usize;
    if clip.spin != Spin::None && spin_deg != 0 && n > hip {
        let signed = match clip.spin {
            Spin::Knockback => {
                if actor.facing_left {
                    spin_deg
                } else {
                    -spin_deg
                }
            }
            Spin::Tumble => {
                if actor.facing_left {
                    -spin_deg
                } else {
                    spin_deg
                }
            }
            Spin::None => 0,
        };
        let pivot = (out.tip[hip].x, out.tip[hip].y);
        for i in 0..n {
            let o = rotate_point_cw((out.origin[i].x, out.origin[i].y), pivot, signed);
            let t = rotate_point_cw((out.tip[i].x, out.tip[i].y), pivot, signed);
            out.origin[i] = Point::new(o.0, o.1);
            out.tip[i] = Point::new(t.0, t.1);
        }
    }

    plant_on_baseline(out, root.y);
}

/// Lift the pose so the body does not sit below `baseline_y` (never lower it).
/// Sword / fist / guard are ignored so a low blade does not levitate the figure.
fn plant_on_baseline(out: &mut PoseScratch, baseline_y: i32) {
    let Some(lowest) = contact_lowest_y(out) else {
        return;
    };
    let dy = lowest - baseline_y;
    if dy <= 0 {
        return;
    }
    for i in 0..out.n {
        out.origin[i].y -= dy;
        out.tip[i].y -= dy;
    }
}

fn contact_lowest_y(out: &PoseScratch) -> Option<i32> {
    let n = out.n.min(library::FIST as usize);
    if n == 0 {
        return None;
    }
    let mut lowest = i32::MIN;
    for i in 0..n {
        lowest = lowest.max(out.origin[i].y).max(out.tip[i].y);
        if let Some(species) = out.species {
            if let BoneKind::Circle { diameter } = species.bones[i].kind {
                let r = (diameter as i32 + 1) / 2;
                lowest = lowest.max(out.tip[i].y + r);
            }
        }
    }
    Some(lowest)
}

fn joint_tip(origin: Point, angle_deg: i32, length: i32, dir: i32) -> Point {
    let (s, c) = sin_cos_deg_milli(angle_deg);
    Point::new(
        origin.x + dir * s * length / 1000,
        origin.y + c * length / 1000,
    )
}

/// Inclusive AABB of visible strokes (no dirty-tile padding).
pub fn hitbox(pose: &PoseScratch) -> Rectangle {
    match visible_aabb(pose) {
        Some((min_x, min_y, max_x, max_y)) => Rectangle::new(
            Point::new(min_x, min_y),
            Size::new((max_x - min_x).max(1) as u32, (max_y - min_y).max(1) as u32),
        ),
        None => fallback_rect(pose),
    }
}

/// Inclusive AABB of visible strokes, padded for body thickness.
pub fn dirty_rect(pose: &PoseScratch) -> Rectangle {
    match visible_aabb(pose) {
        Some((min_x, min_y, max_x, max_y)) => {
            const PAD: i32 = 4;
            let w = (max_x - min_x + PAD * 2).max(1) as u32;
            let h = (max_y - min_y + PAD * 2).max(1) as u32;
            Rectangle::new(Point::new(min_x - PAD, min_y - PAD), Size::new(w, h))
        }
        None => fallback_rect(pose),
    }
}

fn fallback_rect(pose: &PoseScratch) -> Rectangle {
    let p = if pose.n > 0 {
        pose.origin[0]
    } else {
        Point::new(0, 0)
    };
    Rectangle::new(Point::new(p.x - 8, p.y - 8), Size::new(16, 16))
}

fn visible_aabb(pose: &PoseScratch) -> Option<(i32, i32, i32, i32)> {
    let species = pose.species?;
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    let mut any = false;

    for i in 0..pose.n {
        if !pose.visible[i] {
            continue;
        }
        match species.bones[i].kind {
            BoneKind::Joint => {}
            BoneKind::Line => {
                include_point(
                    &mut any,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    pose.origin[i],
                );
                include_point(
                    &mut any,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    pose.tip[i],
                );
            }
            BoneKind::Circle { diameter } => {
                let r = (diameter as i32 + 1) / 2 + 1;
                let c = pose.tip[i];
                include_point(
                    &mut any,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    Point::new(c.x - r, c.y - r),
                );
                include_point(
                    &mut any,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    Point::new(c.x + r, c.y + r),
                );
            }
            BoneKind::Rect { width, height } => {
                let o = pose.origin[i];
                let hw = width as i32 / 2;
                let h = height as i32;
                include_point(
                    &mut any,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    Point::new(o.x - hw, o.y - h),
                );
                include_point(
                    &mut any,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    Point::new(o.x + hw, o.y),
                );
            }
        }
    }

    if !any {
        None
    } else {
        Some((min_x, min_y, max_x, max_y))
    }
}

fn include_point(
    any: &mut bool,
    min_x: &mut i32,
    min_y: &mut i32,
    max_x: &mut i32,
    max_y: &mut i32,
    p: Point,
) {
    *any = true;
    *min_x = (*min_x).min(p.x);
    *min_y = (*min_y).min(p.y);
    *max_x = (*max_x).max(p.x);
    *max_y = (*max_y).max(p.y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stickman::ir::{Interp, Key};

    #[test]
    fn lerp_midpoint() {
        let keys = [
            Key {
                t_ms: 0,
                value: 0,
                interp: Interp::Lerp,
            },
            Key {
                t_ms: 100,
                value: 40,
                interp: Interp::Lerp,
            },
        ];
        assert_eq!(sample_track(&keys, 50, 100, false), 20);
    }

    #[test]
    fn hold_does_not_interpolate() {
        let keys = [
            Key {
                t_ms: 0,
                value: 10,
                interp: Interp::Hold,
            },
            Key {
                t_ms: 100,
                value: 40,
                interp: Interp::Lerp,
            },
        ];
        assert_eq!(sample_track(&keys, 50, 100, false), 10);
        assert_eq!(sample_track(&keys, 100, 100, false), 40);
    }

    #[test]
    fn loop_wraps_lerp_to_first_key() {
        let keys = [
            Key {
                t_ms: 0,
                value: 0,
                interp: Interp::Lerp,
            },
            Key {
                t_ms: 80,
                value: 80,
                interp: Interp::Lerp,
            },
        ];
        // t=90 of 100: 80 → 0 over 20ms, 10ms in ⇒ 40.
        assert_eq!(sample_track(&keys, 90, 100, true), 40);
    }

    #[test]
    fn wrap_time_once_clamps_and_loop_mods() {
        let idle = library::clip(crate::stickman::ir::ClipId::Idle);
        assert_eq!(wrap_time(0, idle), 0);
        assert_eq!(wrap_time(99, idle), idle.duration_ms as u32);

        let walk = library::clip(crate::stickman::ir::ClipId::Walk);
        let d = walk.duration_ms as u32;
        assert_eq!(wrap_time(0, walk), 0);
        assert_eq!(wrap_time(d, walk), 0);
        assert_eq!(wrap_time(d + 10, walk), 10);
    }

    #[test]
    fn idle_head_sits_above_feet() {
        let mut actor = Actor::default();
        actor.play(crate::stickman::ir::ClipId::Idle);
        actor.x = 100;
        actor.y = 200;
        let mut pose = PoseScratch::new();
        sample(&actor, &mut pose);
        let head = pose.tip[4];
        assert_eq!(head.x, 100);
        // hip 28 + spine 18 + neck 6 + head 6 = 58.
        assert_eq!(head.y, 200 - 58);
    }

    #[test]
    fn facing_left_mirrors_x() {
        let mut actor = Actor::default();
        actor.play(crate::stickman::ir::ClipId::Crouch);
        actor.x = 100;
        actor.y = 200;
        actor.facing_left = false;
        let mut right = PoseScratch::new();
        sample(&actor, &mut right);
        actor.facing_left = true;
        let mut left = PoseScratch::new();
        sample(&actor, &mut left);
        for i in 0..right.n {
            let dx_r = right.tip[i].x - 100;
            let dx_l = left.tip[i].x - 100;
            assert_eq!(dx_l, -dx_r, "bone {i}");
            assert_eq!(left.tip[i].y, right.tip[i].y, "bone {i}");
        }
    }

    const ALL_CLIPS: [crate::stickman::ir::ClipId; 11] = [
        crate::stickman::ir::ClipId::Walk,
        crate::stickman::ir::ClipId::Idle,
        crate::stickman::ir::ClipId::Jump,
        crate::stickman::ir::ClipId::Crouch,
        crate::stickman::ir::ClipId::Beg,
        crate::stickman::ir::ClipId::SwordStance,
        crate::stickman::ir::ClipId::SwordStab,
        crate::stickman::ir::ClipId::SwordCrouchStance,
        crate::stickman::ir::ClipId::SwordCrouchStab,
        crate::stickman::ir::ClipId::Knockback,
        crate::stickman::ir::ClipId::Tumble,
    ];

    fn sample_at(clip: crate::stickman::ir::ClipId, time_ms: u32) -> PoseScratch {
        let mut actor = Actor::default();
        actor.play(clip);
        actor.x = 100;
        actor.y = 200;
        actor.time_ms = time_ms;
        let mut pose = PoseScratch::new();
        sample(&actor, &mut pose);
        pose
    }

    #[test]
    fn clips_do_not_dip_below_baseline() {
        for clip_id in ALL_CLIPS {
            let clip = library::clip(clip_id);
            let step = 50u32.min(clip.duration_ms.max(1) as u32);
            let mut t = 0u32;
            while t <= clip.duration_ms as u32 {
                let pose = sample_at(clip_id, t);
                let lowest = contact_lowest_y(&pose).expect("contact");
                assert!(
                    lowest <= 200,
                    "{clip_id:?} t={t}: contact y {lowest} below baseline 200"
                );
                t += step;
            }
        }
    }

    #[test]
    fn crouched_sword_plants_on_baseline() {
        let pose = sample_at(crate::stickman::ir::ClipId::SwordCrouchStance, 0);
        let lowest = contact_lowest_y(&pose).expect("contact");
        assert_eq!(lowest, 200);
        let shin_a = pose.tip[library::SHIN_A as usize].y;
        let shin_b = pose.tip[library::SHIN_B as usize].y;
        assert!(shin_a.max(shin_b) <= 200);
    }

    #[test]
    fn box_sits_on_baseline_at_half_stickman_height() {
        let mut actor = Actor::default();
        actor.play(crate::stickman::ir::ClipId::BoxIdle);
        actor.x = 100;
        actor.y = 200;
        let mut pose = PoseScratch::new();
        sample(&actor, &mut pose);
        let lowest = contact_lowest_y(&pose).expect("contact");
        assert_eq!(lowest, 200);
        assert_eq!(pose.origin[0].y, 200);
        let crate::stickman::ir::BoneKind::Rect { width, height } =
            pose.species.unwrap().bones[0].kind
        else {
            panic!("box species should be a rect bone");
        };
        assert_eq!(height, library::BOX_HEIGHT);
        assert_eq!(width, library::BOX_WIDTH);
        assert_eq!(
            height,
            (crate::stickman::geometry::STANDING_HEIGHT / 2) as u32
        );
    }

    #[test]
    fn box_hitbox_is_unpadded_on_baseline() {
        let mut actor = Actor::default();
        actor.play(crate::stickman::ir::ClipId::BoxIdle);
        actor.x = 100;
        actor.y = 200;
        let mut pose = PoseScratch::new();
        sample(&actor, &mut pose);
        let box_hit = hitbox(&pose);
        assert_eq!(box_hit.top_left, Point::new(84, 168));
        assert_eq!(
            box_hit.size,
            Size::new(library::BOX_WIDTH, library::BOX_HEIGHT)
        );
        assert_eq!(box_hit.top_left.y + box_hit.size.height as i32, 200);
    }
}
