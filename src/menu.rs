//! Left-edge touch menu. The strip is reserved UI, not part of the room.

use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    Circle, ContainsPoint, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
};

/// Full-height reserved strip on the left of the display.
pub const MENU_WIDTH: u32 = 56;
/// Equal stacked buttons in [`MENU_WIDTH`].
pub const MENU_BUTTON_COUNT: usize = 4;
/// Room starts just to the right of the menu.
pub const ROOM_LEFT: i32 = MENU_WIDTH as i32;

const PANEL: Rgb565 = Rgb565::BLACK;
const FACE: Rgb565 = Rgb565::new(10, 20, 10);
const EDGE: Rgb565 = Rgb565::WHITE;
const GUTTER_LEFT: i32 = 3;
const GUTTER_RIGHT: i32 = 3;
const GUTTER_BETWEEN: i32 = 3;
/// Corner radius for menu buttons and the room face.
pub const CORNER: u32 = 5;

/// Left-strip buttons, top to bottom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuButton {
    Config,
    Box,
    Dog,
    Man,
}

impl MenuButton {
    pub const ALL: [Self; MENU_BUTTON_COUNT] = [Self::Config, Self::Box, Self::Dog, Self::Man];

    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Self::Config),
            1 => Some(Self::Box),
            2 => Some(Self::Dog),
            3 => Some(Self::Man),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Config => 0,
            Self::Box => 1,
            Self::Dog => 2,
            Self::Man => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Config => "CFG",
            Self::Box => "BOX",
            Self::Dog => "DOG",
            Self::Man => "MAN",
        }
    }
}

pub const fn room_width() -> u32 {
    DISPLAY_WIDTH - MENU_WIDTH
}

pub const fn button_height() -> u32 {
    DISPLAY_HEIGHT / MENU_BUTTON_COUNT as u32
}

pub fn menu_rect() -> Rectangle {
    Rectangle::new(Point::zero(), Size::new(MENU_WIDTH, DISPLAY_HEIGHT))
}

pub fn room_rect() -> Rectangle {
    Rectangle::new(
        Point::new(ROOM_LEFT, 0),
        Size::new(room_width(), DISPLAY_HEIGHT),
    )
}

/// Room interior, same corner radius as the menu buttons.
pub fn room_face() -> RoundedRectangle {
    RoundedRectangle::with_equal_corners(room_rect(), Size::new(CORNER, CORNER))
}

/// `true` if `(x, y)` is inside the rounded room (not the black corner wedges).
pub fn room_contains(x: i32, y: i32) -> bool {
    room_face().contains(Point::new(x, y))
}

/// Paint the black wedges outside [`room_face`] but inside [`room_rect`].
pub fn mask_room_corners<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let face = room_face();
    let r = CORNER as i32;
    let rect = room_rect();
    let x0 = rect.top_left.x;
    let y0 = rect.top_left.y;
    let x1 = x0 + rect.size.width as i32;
    let y1 = y0 + rect.size.height as i32;
    let origins = [(x0, y0), (x1 - r, y0), (x0, y1 - r), (x1 - r, y1 - r)];
    for (ox, oy) in origins {
        for y in oy..oy + r {
            for x in ox..ox + r {
                if !face.contains(Point::new(x, y)) {
                    Pixel(Point::new(x, y), PANEL).draw(display)?;
                }
            }
        }
    }
    Ok(())
}

/// Button `i` in display space, top to bottom. `None` if `i` is out of range.
pub fn button_rect(i: usize) -> Option<Rectangle> {
    if i >= MENU_BUTTON_COUNT {
        return None;
    }
    let h = button_height();
    Some(Rectangle::new(
        Point::new(0, i as i32 * h as i32),
        Size::new(MENU_WIDTH, h),
    ))
}

pub fn contains(x: u32, y: u32) -> bool {
    x < MENU_WIDTH && y < DISPLAY_HEIGHT
}

/// Which button was hit, if the point is inside the reserved strip.
pub fn hit_button(x: u32, y: u32) -> Option<MenuButton> {
    if !contains(x, y) {
        return None;
    }
    MenuButton::from_index((y / button_height()) as u8)
}

/// Visible face of button `i`: flush top / bottom at the strip edges,
/// with 3px left/right gutters and a 3px gap between buttons.
fn button_face_rect(i: usize) -> Option<Rectangle> {
    if i >= MENU_BUTTON_COUNT {
        return None;
    }
    let h = button_height() as i32;
    let bottom = if i + 1 == MENU_BUTTON_COUNT {
        0
    } else {
        GUTTER_BETWEEN
    };
    Some(Rectangle::new(
        Point::new(GUTTER_LEFT, i as i32 * h),
        Size::new(
            (MENU_WIDTH as i32 - GUTTER_LEFT - GUTTER_RIGHT) as u32,
            (h - bottom) as u32,
        ),
    ))
}

fn face_center(face: Rectangle) -> Point {
    Point::new(
        face.top_left.x + face.size.width as i32 / 2,
        face.top_left.y + face.size.height as i32 / 2,
    )
}

pub fn draw<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.fill_solid(&menu_rect(), PANEL)?;
    let face = PrimitiveStyleBuilder::new()
        .fill_color(FACE)
        .stroke_color(EDGE)
        .stroke_width(1)
        .build();
    for i in 0..MENU_BUTTON_COUNT {
        let body = button_face_rect(i).expect("button");
        RoundedRectangle::with_equal_corners(body, Size::new(CORNER, CORNER))
            .into_styled(face)
            .draw(display)?;
        draw_icon(display, body, MenuButton::ALL[i])?;
    }
    Ok(())
}

fn stroke() -> PrimitiveStyle<Rgb565> {
    PrimitiveStyle::with_stroke(EDGE, 2)
}

fn polar(c: Point, r: i32, deg: i32) -> Point {
    Point::new(
        c.x + r * cos_milli(deg) / 1000,
        c.y + r * sin_milli(deg) / 1000,
    )
}

/// `sin` in milli-units. 15° table, folded to a full turn.
fn sin_milli(deg: i32) -> i32 {
    const Q: [i32; 7] = [0, 259, 500, 707, 866, 966, 1000];
    let d = deg.rem_euclid(360);
    let (d, sign) = if d > 180 { (360 - d, -1) } else { (d, 1) };
    let d = if d > 90 { 180 - d } else { d };
    let i = (d / 15) as usize;
    let f = d % 15;
    let a = Q[i];
    let b = Q[(i + 1).min(6)];
    sign * (a + (b - a) * f / 15)
}

fn cos_milli(deg: i32) -> i32 {
    sin_milli(deg + 90)
}

fn draw_icon<D>(display: &mut D, face: Rectangle, button: MenuButton) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let c = face_center(face);
    match button {
        MenuButton::Config => draw_gear(display, c),
        MenuButton::Box => draw_spiral(display, c),
        MenuButton::Dog => draw_dog_face(display, face),
        MenuButton::Man => draw_stick_figure(display, c),
    }
}

fn draw_gear<D>(display: &mut D, c: Point) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    Circle::with_center(c, 20)
        .into_styled(stroke())
        .draw(display)?;
    Circle::with_center(c, 8)
        .into_styled(stroke())
        .draw(display)?;
    for i in 0..8 {
        let deg = i * 45;
        Line::new(polar(c, 10, deg), polar(c, 17, deg))
            .into_styled(PrimitiveStyle::with_stroke(EDGE, 3))
            .draw(display)?;
    }
    Ok(())
}

fn draw_spiral<D>(display: &mut D, c: Point) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    const STEPS: i32 = 36;
    let mut prev = polar(c, 2, 0);
    for i in 1..=STEPS {
        let r = 2 + i * 15 / STEPS;
        let p = polar(c, r, i * 20);
        Line::new(prev, p).into_styled(stroke()).draw(display)?;
        prev = p;
    }
    Ok(())
}

/// 1-bit dog face from the guide PNG (MSB first, 40×32, 5 bytes/row).
/// Sized to keep ≥2px between the bitmap and the dog button face.
const DOG_ICON_PAD: i32 = 2;
const DOG_ICON_W: u32 = 40;
const DOG_ICON_H: u32 = 32;
const DOG_ICON_ROW: usize = 5;
const DOG_ICON: &[u8] = &[
    0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xC0, 0x00, 0x00, 0x0F, 0x81, 0xF0, 0x00, 0x00,
    0xFE, 0x00, 0xFF, 0x80, 0x03, 0xFC, 0x00, 0x3F, 0xC0, 0x07, 0xF8, 0x00, 0x1F, 0xE0, 0x0F, 0x30,
    0x00, 0x1C, 0xF0, 0x1C, 0x30, 0x00, 0x0C, 0x38, 0x38, 0x30, 0x00, 0x0C, 0x1C, 0x30, 0x30, 0x00,
    0x0C, 0x0C, 0x60, 0x70, 0x00, 0x06, 0x06, 0x60, 0x60, 0x00, 0x06, 0x06, 0xE0, 0x67, 0x00, 0xE6,
    0x07, 0xC0, 0xEF, 0x81, 0xF7, 0x03, 0xC0, 0xEF, 0x83, 0xF7, 0x03, 0xC0, 0xC7, 0x81, 0xE3, 0x03,
    0xC0, 0xC3, 0x3C, 0xC3, 0x03, 0xC1, 0xC0, 0xFF, 0x03, 0x03, 0xE1, 0xC1, 0xFF, 0x03, 0x83, 0xE3,
    0x81, 0xFF, 0x83, 0xC7, 0x7F, 0x81, 0xFF, 0x83, 0xFE, 0x3F, 0x80, 0xFF, 0x03, 0x7C, 0x01, 0x80,
    0x7E, 0x03, 0x00, 0x01, 0xC0, 0x38, 0x03, 0x00, 0x00, 0xC1, 0x18, 0x03, 0x00, 0x00, 0xE3, 0xFF,
    0x87, 0x00, 0x00, 0x73, 0xFF, 0x8E, 0x00, 0x00, 0x78, 0xF7, 0x9E, 0x00, 0x00, 0x3E, 0x00, 0x7C,
    0x00, 0x00, 0x1F, 0x81, 0xF8, 0x00, 0x00, 0x07, 0xFF, 0xC0, 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00,
];

fn draw_dog_face<D>(display: &mut D, face: Rectangle) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let inner_w = face.size.width as i32 - DOG_ICON_PAD * 2;
    let inner_h = face.size.height as i32 - DOG_ICON_PAD * 2;
    let x0 = face.top_left.x + DOG_ICON_PAD + (inner_w - DOG_ICON_W as i32) / 2;
    let y0 = face.top_left.y + DOG_ICON_PAD + (inner_h - DOG_ICON_H as i32) / 2;
    draw_bitmap(
        display,
        x0,
        y0,
        DOG_ICON_W,
        DOG_ICON_H,
        DOG_ICON_ROW,
        DOG_ICON,
    )
}

fn draw_bitmap<D>(
    display: &mut D,
    x0: i32,
    y0: i32,
    width: u32,
    height: u32,
    row_bytes: usize,
    bits: &[u8],
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    for y in 0..height as i32 {
        let row = (y as usize) * row_bytes;
        for x in 0..width as i32 {
            let byte = bits[row + (x as usize) / 8];
            if byte & (0x80 >> (x % 8)) != 0 {
                Pixel(Point::new(x0 + x, y0 + y), EDGE).draw(display)?;
            }
        }
    }
    Ok(())
}

fn draw_stick_figure<D>(display: &mut D, c: Point) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    Circle::with_center(Point::new(c.x, c.y - 12), 8)
        .into_styled(stroke())
        .draw(display)?;
    // Torso, arms, legs — 2px strokes like the in-game figure.
    Line::new(Point::new(c.x, c.y - 8), Point::new(c.x, c.y + 4))
        .into_styled(stroke())
        .draw(display)?;
    Line::new(Point::new(c.x - 9, c.y - 2), Point::new(c.x + 9, c.y - 2))
        .into_styled(stroke())
        .draw(display)?;
    Line::new(Point::new(c.x, c.y + 4), Point::new(c.x - 7, c.y + 16))
        .into_styled(stroke())
        .draw(display)?;
    Line::new(Point::new(c.x, c.y + 4), Point::new(c.x + 7, c.y + 16))
        .into_styled(stroke())
        .draw(display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty::SliceDisplay;

    #[test]
    fn reserved_strip_is_full_height() {
        let m = menu_rect();
        assert_eq!(m.top_left, Point::zero());
        assert_eq!(m.size, Size::new(MENU_WIDTH, DISPLAY_HEIGHT));
        let room = room_rect();
        assert_eq!(room.top_left.x, MENU_WIDTH as i32);
        assert_eq!(room.size.width, DISPLAY_WIDTH - MENU_WIDTH);
        assert_eq!(room.size.height, DISPLAY_HEIGHT);
        assert!(!room_contains(ROOM_LEFT, 0));
        assert!(!room_contains(DISPLAY_WIDTH as i32 - 1, 0));
        assert!(!room_contains(ROOM_LEFT, DISPLAY_HEIGHT as i32 - 1));
        assert!(!room_contains(
            DISPLAY_WIDTH as i32 - 1,
            DISPLAY_HEIGHT as i32 - 1
        ));
        assert!(room_contains(ROOM_LEFT + CORNER as i32, CORNER as i32));
    }

    #[test]
    fn four_buttons_are_equal_and_stack() {
        assert_eq!(button_height() * MENU_BUTTON_COUNT as u32, DISPLAY_HEIGHT);
        let mut y = 0i32;
        for i in 0..MENU_BUTTON_COUNT {
            let r = button_rect(i).expect("button");
            assert_eq!(r.top_left, Point::new(0, y));
            assert_eq!(r.size, Size::new(MENU_WIDTH, button_height()));
            y += button_height() as i32;
        }
        assert!(button_rect(MENU_BUTTON_COUNT).is_none());
    }

    #[test]
    fn hit_test_maps_each_quarter() {
        assert_eq!(hit_button(0, 0), Some(MenuButton::Config));
        assert_eq!(
            hit_button(15, button_height() - 1),
            Some(MenuButton::Config)
        );
        assert_eq!(hit_button(15, button_height()), Some(MenuButton::Box));
        assert_eq!(
            hit_button(MENU_WIDTH - 1, button_height() * 3),
            Some(MenuButton::Man)
        );
        assert_eq!(
            MenuButton::ALL.map(MenuButton::label),
            ["CFG", "BOX", "DOG", "MAN"]
        );
        assert_eq!(hit_button(MENU_WIDTH, 10), None);
        assert!(!contains(MENU_WIDTH, 0));
        assert!(contains(MENU_WIDTH - 1, DISPLAY_HEIGHT - 1));
    }

    #[test]
    fn draw_fills_the_strip_with_four_faces() {
        const W: u32 = MENU_WIDTH;
        const H: u32 = DISPLAY_HEIGHT;
        let mut buf = [Rgb565::RED; (W * H) as usize];
        let mut display = SliceDisplay::new(&mut buf, W, H);
        draw(&mut display).unwrap();
        assert!(buf.iter().all(|c| *c != Rgb565::RED));
        let at = |x: i32, y: i32| buf[(y as u32 * W + x as u32) as usize];
        // Left and right gutters stay black; the strip is flush on the top and bottom.
        assert_eq!(at(0, button_height() as i32 / 2), PANEL);
        assert_eq!(at(GUTTER_LEFT - 1, button_height() as i32 / 2), PANEL);
        assert_eq!(at(MENU_WIDTH as i32 - 1, H as i32 / 2), PANEL);
        assert_eq!(at(MENU_WIDTH as i32 - GUTTER_RIGHT, H as i32 / 2), PANEL);
        assert_ne!(at(MENU_WIDTH as i32 / 2, 0), PANEL);
        assert_ne!(at(MENU_WIDTH as i32 / 2, H as i32 - 1), PANEL);
        assert_ne!(at(GUTTER_LEFT, button_height() as i32 / 2), PANEL);
        let face = buf.iter().filter(|c| **c == FACE).count();
        let edge = buf.iter().filter(|c| **c == EDGE).count();
        assert!(face > 200, "button faces should fill, got {face}");
        assert!(edge > 40, "outlines and icons should paint, got {edge}");
    }

    #[test]
    fn dog_icon_keeps_padding_inside_the_face() {
        let face = button_face_rect(MenuButton::Dog.index()).expect("dog");
        assert!(DOG_ICON_W as i32 + DOG_ICON_PAD * 2 <= face.size.width as i32);
        assert!(DOG_ICON_H as i32 + DOG_ICON_PAD * 2 <= face.size.height as i32);
    }
}
