//! Left-edge touch menu. The strip is reserved UI, not part of the room.

use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
};
use embedded_graphics::text::{Baseline, Text};

/// Full-height reserved strip on the left of the display.
pub const MENU_WIDTH: u32 = 30;
/// Equal stacked buttons in [`MENU_WIDTH`].
pub const MENU_BUTTON_COUNT: usize = 4;
/// Room starts just to the right of the menu.
pub const ROOM_LEFT: i32 = MENU_WIDTH as i32;

const PANEL: Rgb565 = Rgb565::BLACK;
const FACE: Rgb565 = Rgb565::new(10, 20, 10);
const EDGE: Rgb565 = Rgb565::WHITE;
const GUTTER: i32 = 3;
const CORNER: u32 = 5;

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
    let label = MonoTextStyle::new(&FONT_6X10, EDGE);
    let h = button_height() as i32;
    for i in 0..MENU_BUTTON_COUNT {
        let y = i as i32 * h;
        let body = Rectangle::new(
            Point::new(GUTTER, y + GUTTER),
            Size::new(
                (MENU_WIDTH as i32 - GUTTER * 2) as u32,
                (h - GUTTER * 2) as u32,
            ),
        );
        RoundedRectangle::with_equal_corners(body, Size::new(CORNER, CORNER))
            .into_styled(face)
            .draw(display)?;
        draw_label(display, label, y, h, MenuButton::ALL[i])?;
    }
    Ok(())
}

fn draw_label<D>(
    display: &mut D,
    style: MonoTextStyle<'_, Rgb565>,
    cell_y: i32,
    cell_h: i32,
    button: MenuButton,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let text = button.label();
    let line_h = FONT_6X10.character_size.height as i32;
    let glyph_w = FONT_6X10.character_size.width as i32;
    let block_h = text.len() as i32 * line_h;
    let tx = (MENU_WIDTH as i32 - glyph_w) / 2;
    let mut ty = cell_y + (cell_h - block_h) / 2;
    let mut ch = [0u8; 1];
    for b in text.bytes() {
        ch[0] = b;
        let line = core::str::from_utf8(&ch).unwrap_or("?");
        Text::with_baseline(line, Point::new(tx, ty), style, Baseline::Top).draw(display)?;
        ty += line_h;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty::SliceDisplay;

    #[test]
    fn reserved_strip_is_thirty_by_full_height() {
        let m = menu_rect();
        assert_eq!(m.top_left, Point::zero());
        assert_eq!(m.size, Size::new(30, DISPLAY_HEIGHT));
        let room = room_rect();
        assert_eq!(room.top_left.x, 30);
        assert_eq!(room.size.width, DISPLAY_WIDTH - 30);
        assert_eq!(room.size.height, DISPLAY_HEIGHT);
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
        assert_eq!(hit_button(15, button_height() - 1), Some(MenuButton::Config));
        assert_eq!(hit_button(15, button_height()), Some(MenuButton::Box));
        assert_eq!(hit_button(29, button_height() * 3), Some(MenuButton::Man));
        assert_eq!(MenuButton::ALL.map(MenuButton::label), ["CFG", "BOX", "DOG", "MAN"]);
        assert_eq!(hit_button(30, 10), None);
        assert!(!contains(30, 0));
        assert!(contains(29, DISPLAY_HEIGHT - 1));
    }

    #[test]
    fn draw_fills_the_strip_with_four_faces() {
        const W: u32 = MENU_WIDTH;
        const H: u32 = DISPLAY_HEIGHT;
        let mut buf = [Rgb565::RED; (W * H) as usize];
        let mut display = SliceDisplay::new(&mut buf, W, H);
        draw(&mut display).unwrap();
        assert!(buf.iter().all(|c| *c != Rgb565::RED));
        assert_eq!(buf[0], PANEL);
        assert_eq!(buf[(GUTTER as u32 - 1) as usize], PANEL);
        let face = buf.iter().filter(|c| **c == FACE).count();
        let edge = buf.iter().filter(|c| **c == EDGE).count();
        assert!(face > 200, "button faces should fill, got {face}");
        assert!(edge > 40, "outlines and labels should paint, got {edge}");
    }
}
