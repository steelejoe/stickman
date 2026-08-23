//! Speech bubble: rounded body, sharp tail at the head, 6-pt text.

use crate::stickman::ir::{BoneKind, PoseScratch};
use crate::stickman::library;
use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle, Triangle,
};
use embedded_graphics::text::{Baseline, Text};

const WHITE: Rgb565 = Rgb565::WHITE;
const BLACK: Rgb565 = Rgb565::BLACK;
/// Soft gray for the drop shadow (no alpha on the panel).
const SHADOW: Rgb565 = Rgb565::new(8, 16, 8);
/// 6-pt bitmap cell used for measure and draw.
const FONT: &embedded_graphics::mono_font::MonoFont<'_> = &FONT_6X10;
const PAD_X: i32 = 7;
const PAD_Y: i32 = 6;
const STROKE: u32 = 2;
const TAIL_LEN: i32 = 13;
const TAIL_BASE: i32 = 14;
/// Horizontal gap from the head circle to the body.
const SIDE_GAP: i32 = 4;
/// Drop shadow offset (left and down), matching the reference balloon.
const SHADOW_DX: i32 = -2;
const SHADOW_DY: i32 = 3;
const TEXT_CAP: usize = 64;

/// Placed balloon + tail + text, ready to stroke into a dirty tile.
#[derive(Clone, Copy, Debug)]
pub struct Bubble {
    /// AABB including stroke, tail, and shadow (for dirty / hit tests).
    pub bounds: Rectangle,
    body: Rectangle,
    tail: [Point; 3],
    corner: u32,
    text_pos: Point,
    text: [u8; TEXT_CAP],
    text_len: u8,
}

impl Bubble {
    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.text[..self.text_len as usize]).unwrap_or("")
    }

    /// True when the balloon sits to the left of the head.
    pub fn is_left(&self) -> bool {
        self.tail[2].x >= self.body.top_left.x + self.body.size.width as i32 / 2
    }
}

/// Layout a balloon above a head circle, or the top of a crate.
pub fn for_pose(pose: &PoseScratch, text: &str, prefer_left: bool) -> Option<Bubble> {
    if text.is_empty() {
        return None;
    }
    let (anchor, radius) = speech_anchor(pose)?;
    Some(layout(anchor, radius, text, prefer_left))
}

fn speech_anchor(pose: &PoseScratch) -> Option<(Point, i32)> {
    let species = pose.species?;
    let head = library::HEAD as usize;
    if head < pose.n {
        if let BoneKind::Circle { diameter } = species.bones.get(head)?.kind {
            return Some((pose.tip[head], (diameter as i32 + 1) / 2));
        }
    }
    for i in 0..pose.n {
        if let BoneKind::Rect { width: _, height } = species.bones.get(i)?.kind {
            let origin = pose.origin[i];
            return Some((Point::new(origin.x, origin.y - height as i32), 4));
        }
    }
    None
}

pub fn layout(head: Point, head_r: i32, text: &str, prefer_left: bool) -> Bubble {
    let (text_w, text_h) = text_size(text);
    let body_w = (text_w + PAD_X * 2).max(20);
    let body_h = (text_h + PAD_Y * 2).max(16);
    let corner = corner_radius(body_w, body_h);

    let (body_x, body_y, left) = place_body(head, head_r, body_w, body_h, prefer_left);
    let body = Rectangle::new(
        Point::new(body_x, body_y),
        Size::new(body_w as u32, body_h as u32),
    );

    let tip = Point::new(head.x, head.y - head_r - 1);
    // Flush with the bottom edge; the body is drawn over any inward overlap.
    let base_y = body_y + body_h;
    let inset = corner as i32 + 3;
    let (x0, x1) = if left {
        let x1 = body_x + body_w - inset;
        (x1 - TAIL_BASE, x1)
    } else {
        let x0 = body_x + inset;
        (x0, x0 + TAIL_BASE)
    };
    let tail = [Point::new(x0, base_y - 1), Point::new(x1, base_y - 1), tip];

    let text_pos = Point::new(body_x + PAD_X, body_y + PAD_Y);

    let mut stored = [0u8; TEXT_CAP];
    let n = text.len().min(TEXT_CAP);
    stored[..n].copy_from_slice(&text.as_bytes()[..n]);

    let pad = STROKE as i32 + 1;
    let bounds = aabb_of(&[
        Point::new(body_x - pad, body_y - pad),
        Point::new(body_x + body_w + pad, body_y + body_h + pad),
        Point::new(body_x + SHADOW_DX - pad, body_y + SHADOW_DY - pad),
        Point::new(
            body_x + body_w + SHADOW_DX + pad,
            body_y + body_h + SHADOW_DY + pad,
        ),
        tail[0],
        tail[1],
        tail[2],
        Point::new(tail[2].x + SHADOW_DX, tail[2].y + SHADOW_DY),
    ]);

    Bubble {
        bounds,
        body,
        tail,
        corner,
        text_pos,
        text: stored,
        text_len: n as u8,
    }
}

pub fn draw<D>(display: &mut D, bubble: &Bubble) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let fill_stroke = PrimitiveStyleBuilder::new()
        .fill_color(WHITE)
        .stroke_color(BLACK)
        .stroke_width(STROKE)
        .build();
    let shadow = PrimitiveStyle::with_fill(SHADOW);
    let fill_white = PrimitiveStyle::with_fill(WHITE);
    let edge = PrimitiveStyle::with_stroke(BLACK, STROKE);
    let off = Point::new(SHADOW_DX, SHADOW_DY);

    rounded(bubble.body, bubble.corner)
        .translate(off)
        .into_styled(shadow)
        .draw(display)?;
    Triangle::new(
        bubble.tail[0] + off,
        bubble.tail[1] + off,
        bubble.tail[2] + off,
    )
    .into_styled(shadow)
    .draw(display)?;

    // Tail first, then the body on top so side strokes cannot enter the interior.
    Triangle::new(bubble.tail[0], bubble.tail[1], bubble.tail[2])
        .into_styled(fill_white)
        .draw(display)?;
    Line::new(bubble.tail[0], bubble.tail[2])
        .into_styled(edge)
        .draw(display)?;
    Line::new(bubble.tail[1], bubble.tail[2])
        .into_styled(edge)
        .draw(display)?;
    rounded(bubble.body, bubble.corner)
        .into_styled(fill_stroke)
        .draw(display)?;

    // Open the bottom stroke where the tail attaches so the outline is one piece.
    let join_l = bubble.tail[0].x.min(bubble.tail[1].x) + STROKE as i32;
    let join_r = bubble.tail[0].x.max(bubble.tail[1].x) - STROKE as i32;
    if join_r > join_l {
        let y = bubble.body.top_left.y + bubble.body.size.height as i32 - STROKE as i32;
        Rectangle::new(
            Point::new(join_l, y),
            Size::new((join_r - join_l) as u32, STROKE + 1),
        )
        .into_styled(fill_white)
        .draw(display)?;
    }

    let style = MonoTextStyle::new(FONT, BLACK);
    Text::with_baseline(bubble.text(), bubble.text_pos, style, Baseline::Top).draw(display)?;
    Ok(())
}

fn rounded(body: Rectangle, corner: u32) -> RoundedRectangle {
    RoundedRectangle::with_equal_corners(body, Size::new(corner, corner))
}

fn corner_radius(w: i32, h: i32) -> u32 {
    (w.min(h) / 2).clamp(6, 12) as u32
}

fn text_size(text: &str) -> (i32, i32) {
    let adv = (FONT.character_size.width + FONT.character_spacing) as i32;
    let line_h = FONT.character_size.height as i32;
    let mut lines = 1i32;
    let mut max_w = 0i32;
    let mut w = 0i32;
    for b in text.bytes() {
        if b == b'\n' {
            lines += 1;
            max_w = max_w.max(w);
            w = 0;
        } else {
            w += adv;
        }
    }
    max_w = max_w.max(w);
    (max_w, lines * line_h)
}

fn place_body(
    head: Point,
    head_r: i32,
    body_w: i32,
    body_h: i32,
    prefer_left: bool,
) -> (i32, i32, bool) {
    let y = (head.y - head_r - TAIL_LEN - body_h).max(1);
    let left_x = head.x - head_r - SIDE_GAP - body_w;
    let right_x = head.x + head_r + SIDE_GAP;
    let screen_w = DISPLAY_WIDTH as i32;
    let min_x = (1 - SHADOW_DX).max(1);
    let fits = |x: i32| x >= min_x && x + body_w < screen_w - 1;

    let mut left = prefer_left;
    let mut x = if left { left_x } else { right_x };
    if !fits(x) {
        left = !left;
        x = if left { left_x } else { right_x };
    }
    let max_x = (screen_w - 1 - body_w).max(min_x);
    x = x.clamp(min_x, max_x);
    let max_y = (DISPLAY_HEIGHT as i32 - 1 - body_h - SHADOW_DY).max(1);
    let y = y.clamp(1, max_y);
    (x, y, left)
}

fn aabb_of(pts: &[Point]) -> Rectangle {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for p in pts {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    Rectangle::new(
        Point::new(min_x, min_y),
        Size::new((max_x - min_x).max(1) as u32, (max_y - min_y).max(1) as u32),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_text_makes_a_wider_body() {
        let head = Point::new(200, 160);
        let short = layout(head, 6, "Idle", false);
        let long = layout(head, 6, "SwordCrouchStance", false);
        assert!(long.body.size.width > short.body.size.width);
        assert_eq!(short.body.size.height, long.body.size.height);
        let (tw, th) = text_size("SwordCrouchStance");
        assert!(long.body.size.width as i32 >= tw + PAD_X * 2);
        assert!(long.body.size.height as i32 >= th + PAD_Y * 2);
    }

    #[test]
    fn three_lines_make_a_taller_body() {
        let head = Point::new(200, 160);
        let one = layout(head, 6, "Walking", false);
        let three = layout(head, 6, "Walking\nIdle\nJumping", false);
        assert!(three.body.size.height > one.body.size.height);
        assert!(three.body.size.width >= one.body.size.width);
        let (tw, th) = text_size("Walking\nIdle\nJumping");
        assert!(three.body.size.width as i32 >= tw + PAD_X * 2);
        assert!(three.body.size.height as i32 >= th + PAD_Y * 2);
    }

    #[test]
    fn body_sits_left_or_right_of_the_head() {
        let head = Point::new(200, 160);
        let left = layout(head, 6, "Walking\nIdle\nJumping", true);
        let right = layout(head, 6, "Walking\nIdle\nJumping", false);
        assert!(left.body.top_left.x + left.body.size.width as i32 <= head.x);
        assert!(right.body.top_left.x >= head.x);
        assert!(left.is_left());
        assert!(!right.is_left());
    }

    #[test]
    fn tail_tip_points_at_the_head() {
        let head = Point::new(200, 160);
        let bubble = layout(head, 6, "Idle", false);
        let tip = bubble.tail[2];
        assert_eq!(tip.x, head.x);
        assert_eq!(tip.y, head.y - 7);
        assert!(tip.y > bubble.body.top_left.y + bubble.body.size.height as i32 - 4);
    }

    #[test]
    fn near_the_left_edge_flips_to_the_right() {
        let head = Point::new(20, 160);
        let bubble = layout(head, 6, "SwordCrouchStance", true);
        assert!(!bubble.is_left());
        assert!(bubble.body.top_left.x >= 1);
    }

    #[test]
    fn bounds_cover_body_tail_and_shadow() {
        let head = Point::new(200, 160);
        let bubble = layout(head, 6, "Walking\nIdle", true);
        let b = bubble.bounds;
        let x1 = b.top_left.x + b.size.width as i32;
        let y1 = b.top_left.y + b.size.height as i32;
        assert!(b.top_left.x <= bubble.body.top_left.x + SHADOW_DX);
        assert!(b.top_left.y <= bubble.body.top_left.y);
        assert!(x1 >= bubble.tail[2].x);
        assert!(y1 >= bubble.tail[2].y + SHADOW_DY);
    }

    #[test]
    fn draw_paints_white_fill_and_black_outline() {
        use crate::dirty::SliceDisplay;
        let head = Point::new(28, 72);
        let bubble = layout(head, 6, "Idle\nWalk", false);
        const W: u32 = 100;
        const H: u32 = 80;
        let mut buf = [Rgb565::RED; (W * H) as usize];
        let mut display = SliceDisplay::new(&mut buf, W, H);
        draw(&mut display, &bubble).unwrap();
        let white = buf.iter().filter(|c| **c == WHITE).count();
        let black = buf.iter().filter(|c| **c == BLACK).count();
        assert!(white > 80, "balloon body should fill white, got {white}");
        assert!(
            black > 20,
            "outline and text should paint black, got {black}"
        );
    }

    #[test]
    fn tail_strokes_do_not_enter_the_body() {
        use crate::dirty::SliceDisplay;
        let head = Point::new(28, 72);
        let bubble = layout(head, 6, "Hi", false);
        const W: u32 = 100;
        const H: u32 = 80;
        let mut buf = [Rgb565::RED; (W * H) as usize];
        let mut display = SliceDisplay::new(&mut buf, W, H);
        draw(&mut display, &bubble).unwrap();

        let x0 = bubble.body.top_left.x + bubble.corner as i32 + 1;
        let x1 = bubble.body.top_left.x + bubble.body.size.width as i32 - bubble.corner as i32 - 1;
        let y0 = bubble.body.top_left.y + bubble.corner as i32 + 1;
        // Stay above the bottom stroke and the tail join.
        let y1 = bubble.body.top_left.y + bubble.body.size.height as i32 - STROKE as i32 - 2;
        let (tw, th) = text_size(bubble.text());
        let tx0 = bubble.text_pos.x - 1;
        let ty0 = bubble.text_pos.y - 1;
        let tx1 = bubble.text_pos.x + tw + 1;
        let ty1 = bubble.text_pos.y + th + 1;
        let mut interior_black = 0u32;
        for y in y0..y1 {
            for x in x0..x1 {
                if x >= tx0 && x < tx1 && y >= ty0 && y < ty1 {
                    continue;
                }
                if x >= 0 && y >= 0 && (x as u32) < W && (y as u32) < H {
                    let c = buf[(y as u32 * W + x as u32) as usize];
                    if c == BLACK {
                        interior_black += 1;
                    }
                }
            }
        }
        assert_eq!(
            interior_black, 0,
            "tail outline leaked into the interior ({interior_black} black px)"
        );
    }

    #[test]
    fn box_pose_anchors_the_bubble_above_the_lid() {
        use crate::stickman::eval;
        use crate::stickman::ir::{Actor, ClipId};
        let mut actor = Actor::default();
        actor.play(ClipId::BoxIdle);
        actor.x = 200;
        actor.y = 200;
        let mut pose = crate::stickman::ir::PoseScratch::new();
        eval::sample(&actor, &mut pose);
        let bubble = for_pose(&pose, "Ouch!", false).expect("crate top");
        let lid = 200 - crate::stickman::library::BOX_HEIGHT as i32;
        assert!(bubble.bounds.top_left.y < lid);
        assert_eq!(bubble.text(), "Ouch!");
    }
}
