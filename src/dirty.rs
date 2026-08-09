//! Offscreen dirty-rectangle compositing to avoid device flicker.
//!
//! The RM67162 `draw_iter` path issues a QSPI address setup per pixel, so
//! erase-then-redraw is both slow and visibly flickery. Instead we compose the
//! union of the previous/new stickman bounds into a RAM buffer and push it with
//! a single [`DrawTarget::fill_contiguous`] (one window, streamed pixels).

use crate::assets::Rgb565Image;
use crate::stickman::geometry::{floor_y, StickmanState};
use crate::stickman::render;
use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Point, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

/// Max dirty tile stored in [`crate::game::Game`] (≈50 KiB).
/// Wide enough for sword-stab lunge unions without falling back to
/// erase-then-draw (which flashes on the AMOLED).
pub const DIRTY_MAX_W: u32 = 160;
pub const DIRTY_MAX_H: u32 = 160;
pub const DIRTY_BUF_LEN: usize = (DIRTY_MAX_W * DIRTY_MAX_H) as usize;

/// RAM draw target backed by a tightly packed RGB565 slice (row-major).
pub struct SliceDisplay<'a> {
    buf: &'a mut [Rgb565],
    width: u32,
    height: u32,
}

impl<'a> SliceDisplay<'a> {
    pub fn new(buf: &'a mut [Rgb565], width: u32, height: u32) -> Self {
        debug_assert_eq!(buf.len(), (width * height) as usize);
        Self { buf, width, height }
    }
}

impl OriginDimensions for SliceDisplay<'_> {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for SliceDisplay<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let w = self.width as i32;
        let h = self.height as i32;
        for Pixel(p, color) in pixels {
            if p.x >= 0 && p.x < w && p.y >= 0 && p.y < h {
                self.buf[(p.y as u32 * self.width + p.x as u32) as usize] = color;
            }
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut colors = colors.into_iter();
        let x0 = area.top_left.x.max(0) as u32;
        let y0 = area.top_left.y.max(0) as u32;
        let x1 = (area.top_left.x + area.size.width as i32)
            .min(self.width as i32)
            .max(0) as u32;
        let y1 = (area.top_left.y + area.size.height as i32)
            .min(self.height as i32)
            .max(0) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(c) = colors.next() {
                    self.buf[(y * self.width + x) as usize] = c;
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let x0 = area.top_left.x.max(0) as u32;
        let y0 = area.top_left.y.max(0) as u32;
        let x1 = (area.top_left.x + area.size.width as i32)
            .min(self.width as i32)
            .max(0) as u32;
        let y1 = (area.top_left.y + area.size.height as i32)
            .min(self.height as i32)
            .max(0) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                self.buf[(y * self.width + x) as usize] = color;
            }
        }
        Ok(())
    }
}

fn screen_bounds() -> Rectangle {
    Rectangle::new(Point::zero(), Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT))
}

fn clamp_to_screen(rect: Rectangle) -> Rectangle {
    rect.intersection(&screen_bounds())
}

fn union_rects(a: Rectangle, b: Rectangle) -> Rectangle {
    if a.size.width == 0 || a.size.height == 0 {
        return b;
    }
    if b.size.width == 0 || b.size.height == 0 {
        return a;
    }
    let x0 = a.top_left.x.min(b.top_left.x);
    let y0 = a.top_left.y.min(b.top_left.y);
    let x1 = (a.top_left.x + a.size.width as i32).max(b.top_left.x + b.size.width as i32);
    let y1 = (a.top_left.y + a.size.height as i32).max(b.top_left.y + b.size.height as i32);
    Rectangle::new(
        Point::new(x0, y0),
        Size::new((x1 - x0) as u32, (y1 - y0) as u32),
    )
}

fn fill_layer0(buf: &mut [Rgb565], width: u32, area: Rectangle, bg: Option<&Rgb565Image<'_>>) {
    let w = width as usize;
    let h = buf.len() / w;
    let fy = floor_y();
    for row in 0..h {
        let y = area.top_left.y + row as i32;
        for col in 0..w {
            let x = area.top_left.x + col as i32;
            let mut color = match bg {
                Some(img) => img.pixel(x, y).unwrap_or(Rgb565::BLACK),
                None => Rgb565::BLACK,
            };
            if y == fy {
                color = Rgb565::WHITE;
            }
            buf[row * w + col] = color;
        }
    }
}

/// Compose layer 0 (+ optional stickman) into `buf` for `area`, then `fill_contiguous`.
pub fn blit_composed_area<D>(
    display: &mut D,
    buf: &mut [Rgb565],
    area: Rectangle,
    stickman: &StickmanState,
    background: Option<&Rgb565Image<'_>>,
    draw_figure: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let area = clamp_to_screen(area);
    if area.size.width == 0 || area.size.height == 0 {
        return Ok(());
    }
    let w = area.size.width;
    let h = area.size.height;
    let need = (w * h) as usize;
    let tile = &mut buf[..need];

    fill_layer0(tile, w, area, background);

    if draw_figure {
        let mut slice = SliceDisplay::new(tile, w, h);
        let mut view = slice.translated(Point::new(-area.top_left.x, -area.top_left.y));
        let _ = render::draw_stickman(&mut view, stickman);
    }

    let tile = &buf[..need];
    display.fill_contiguous(&area, tile.iter().copied())
}

/// Update the stickman with a single contiguous display write (no erase hole).
pub fn present_stickman_frame<D>(
    display: &mut D,
    dirty_buf: &mut [Rgb565; DIRTY_BUF_LEN],
    prev: Option<&StickmanState>,
    current: &StickmanState,
    background: Option<&Rgb565Image<'_>>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let new_rect = clamp_to_screen(render::stickman_dirty_rect(current));
    let area = match prev {
        Some(p) => clamp_to_screen(union_rects(render::stickman_dirty_rect(p), new_rect)),
        None => new_rect,
    };

    if area.size.width == 0 || area.size.height == 0 {
        return Ok(());
    }

    if area.size.width <= DIRTY_MAX_W && area.size.height <= DIRTY_MAX_H {
        return blit_composed_area(display, dirty_buf, area, current, background, true);
    }

    // Oversized dirty region: restore previous tile, then present the new one.
    if let Some(p) = prev {
        let prev_rect = clamp_to_screen(render::stickman_dirty_rect(p));
        if prev_rect.size.width <= DIRTY_MAX_W && prev_rect.size.height <= DIRTY_MAX_H {
            blit_composed_area(display, dirty_buf, prev_rect, current, background, false)?;
        } else if let Some(img) = background {
            img.blit_rect(display, prev_rect)?;
        }
    }
    if new_rect.size.width <= DIRTY_MAX_W && new_rect.size.height <= DIRTY_MAX_H {
        blit_composed_area(display, dirty_buf, new_rect, current, background, true)?;
    }
    Ok(())
}
