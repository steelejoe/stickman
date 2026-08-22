//! Packed image assets (SM65 RGB565).
//!
//! - Device: optional `assets/background.rgb565` embedded at build time
//! - Sim: load `assets/background.png` (preferred) or `.rgb565` at runtime

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::image::{Image, ImageRawBE};
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

/// Big-endian RGB565 image (pixel payload only — no SM65 header).
#[derive(Clone, Copy, Debug)]
pub struct Rgb565Image<'a> {
    pub width: u16,
    pub height: u16,
    data: &'a [u8],
}

impl<'a> Rgb565Image<'a> {
    /// Wrap raw BE RGB565 pixels.
    pub fn from_pixels(width: u16, height: u16, data: &'a [u8]) -> Option<Self> {
        let need = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(2)?;
        if width == 0 || height == 0 || data.len() < need {
            return None;
        }
        Some(Self {
            width,
            height,
            data: &data[..need],
        })
    }

    /// Parse an SM65 blob: magic + width/height (LE) + BE RGB565 pixels.
    pub fn from_sm65(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < 8 || &bytes[..4] != b"SM65" {
            return None;
        }
        let width = u16::from_le_bytes([bytes[4], bytes[5]]);
        let height = u16::from_le_bytes([bytes[6], bytes[7]]);
        Self::from_pixels(width, height, &bytes[8..])
    }

    /// Sample one pixel in image coordinates, if in bounds.
    pub fn pixel(&self, x: i32, y: i32) -> Option<Rgb565> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        let idx = ((y as u32 * self.width as u32 + x as u32) * 2) as usize;
        let raw = u16::from_be_bytes([self.data[idx], self.data[idx + 1]]);
        Some(Rgb565::from(RawU16::new(raw)))
    }

    /// Draw the full image with its top-left at `pos`.
    pub fn draw<D>(&self, display: &mut D, pos: Point) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let raw = ImageRawBE::<Rgb565>::new(self.data, self.width as u32);
        Image::new(&raw, pos).draw(display)
    }

    /// Blit the intersection of `rect` (display space) via `fill_contiguous`.
    pub fn blit_rect<D>(&self, display: &mut D, rect: Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let img_bounds = Rectangle::new(
            Point::zero(),
            Size::new(self.width as u32, self.height as u32),
        );
        let area = rect.intersection(&img_bounds);
        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }

        let img_w = self.width as i32;
        let x0 = area.top_left.x;
        let y0 = area.top_left.y;
        let w = area.size.width;
        let h = area.size.height;
        let data = self.data;

        display.fill_contiguous(
            &area,
            (0..h).flat_map(|row| {
                let y = y0 + row as i32;
                (0..w).map(move |col| {
                    let x = x0 + col as i32;
                    let idx = ((y * img_w + x) as usize) * 2;
                    let raw = u16::from_be_bytes([data[idx], data[idx + 1]]);
                    Rgb565::from(RawU16::new(raw))
                })
            }),
        )
    }
}

/// Layer-0 backdrop embedded from `assets/background.rgb565` (device / fallback).
pub fn embedded_background() -> Option<Rgb565Image<'static>> {
    #[cfg(has_background)]
    {
        Rgb565Image::from_sm65(include_bytes!("../assets/background.rgb565"))
    }
    #[cfg(not(has_background))]
    {
        None
    }
}

/// Convert one RGB888 pixel to big-endian RGB565 bytes.
pub fn rgb888_to_rgb565_be(r: u8, g: u8, b: u8) -> [u8; 2] {
    let value = ((u16::from(r) & 0xF8) << 8) | ((u16::from(g) & 0xFC) << 3) | (u16::from(b) >> 3);
    value.to_be_bytes()
}
