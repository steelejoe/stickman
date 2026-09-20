//! Packed image assets (SM65 RGB565).
//!
//! - Device: optional `assets/background.rgb565` embedded at build time
//! - Sim: load `assets/background.rgb565` (preferred) or `.png` at runtime
//! - Images are trimmed to the room and store a display-space origin so the
//!   menu strip and transparent padding are not kept in firmware.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::image::{Image, ImageRawBE};
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

/// Layer-0 fill: a full-screen color or a backdrop image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    Color(Rgb565),
    Image(Rgb565Image<'static>),
}

impl Backdrop {
    /// Sample one display-space pixel (image misses fall back to black).
    pub fn pixel(self, x: i32, y: i32) -> Rgb565 {
        match self {
            Self::Color(c) => c,
            Self::Image(img) => img.pixel(x, y).unwrap_or(Rgb565::BLACK),
        }
    }

    /// Paint the full target: solid fill, or the image at the origin.
    pub fn fill<D>(self, display: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        match self {
            Self::Color(c) => display.fill_solid(&display.bounding_box(), c),
            Self::Image(img) => img.draw(display, img.origin),
        }
    }
}

/// Big-endian RGB565 image (pixel payload only — no SM65 header).
///
/// [`Self::origin`] is the display-space top-left of this bitmap so a trimmed
/// room backdrop does not store the menu strip or empty padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb565Image<'a> {
    pub width: u16,
    pub height: u16,
    pub origin: Point,
    data: &'a [u8],
}

impl<'a> Rgb565Image<'a> {
    /// Wrap raw BE RGB565 pixels at display origin `(0, 0)`.
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
            origin: Point::zero(),
            data: &data[..need],
        })
    }

    /// Place this bitmap's top-left at `origin` in display space.
    pub fn with_origin(mut self, origin: Point) -> Self {
        self.origin = origin;
        self
    }

    /// Display-space rectangle covered by this bitmap.
    pub fn display_bounds(&self) -> Rectangle {
        Rectangle::new(
            self.origin,
            Size::new(self.width as u32, self.height as u32),
        )
    }

    /// Parse an SM65 blob.
    ///
    /// Layout: `SM65` + width/height (u16 LE). New files then store origin
    /// (i16 LE × 2) before the pixel payload; older 8-byte headers stay at `(0, 0)`.
    pub fn from_sm65(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < 8 || &bytes[..4] != b"SM65" {
            return None;
        }
        let width = u16::from_le_bytes([bytes[4], bytes[5]]);
        let height = u16::from_le_bytes([bytes[6], bytes[7]]);
        let pixel_bytes = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(2)?;
        if bytes.len() >= 12 + pixel_bytes {
            let origin = Point::new(
                i16::from_le_bytes([bytes[8], bytes[9]]) as i32,
                i16::from_le_bytes([bytes[10], bytes[11]]) as i32,
            );
            Self::from_pixels(width, height, &bytes[12..]).map(|img| img.with_origin(origin))
        } else if bytes.len() >= 8 + pixel_bytes {
            Self::from_pixels(width, height, &bytes[8..])
        } else {
            None
        }
    }

    /// Sample one display-space pixel, if it lands on this bitmap.
    pub fn pixel(&self, x: i32, y: i32) -> Option<Rgb565> {
        let ix = x - self.origin.x;
        let iy = y - self.origin.y;
        if ix < 0 || iy < 0 || ix >= self.width as i32 || iy >= self.height as i32 {
            return None;
        }
        let idx = ((iy as u32 * self.width as u32 + ix as u32) * 2) as usize;
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
        let area = rect.intersection(&self.display_bounds());
        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }

        let img_w = self.width as i32;
        let ox = self.origin.x;
        let oy = self.origin.y;
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
                    let idx = (((y - oy) * img_w + (x - ox)) as usize) * 2;
                    let raw = u16::from_be_bytes([data[idx], data[idx + 1]]);
                    Rgb565::from(RawU16::new(raw))
                })
            }),
        )
    }
}

/// Packed Home backdrop (`assets/background.rgb565`) when the file was present at build.
pub fn firmware_background_sm65() -> Option<&'static [u8]> {
    #[cfg(has_background)]
    {
        Some(include_bytes!("../assets/background.rgb565"))
    }
    #[cfg(not(has_background))]
    {
        None
    }
}

/// Layer-0 backdrop embedded from `assets/background.rgb565` (device / fallback).
pub fn embedded_background() -> Option<Rgb565Image<'static>> {
    firmware_background_sm65().and_then(Rgb565Image::from_sm65)
}

/// Convert one RGB888 pixel to big-endian RGB565 bytes.
pub fn rgb888_to_rgb565_be(r: u8, g: u8, b: u8) -> [u8; 2] {
    let value = ((u16::from(r) & 0xF8) << 8) | ((u16::from(g) & 0xFC) << 3) | (u16::from(b) >> 3);
    value.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty::SliceDisplay;
    use crate::menu::ROOM_LEFT;
    use embedded_graphics::prelude::RgbColor;

    fn sm65_pixel(origin: Point, px: [u8; 2]) -> [u8; 14] {
        let mut out = [0u8; 14];
        out[..4].copy_from_slice(b"SM65");
        out[4..6].copy_from_slice(&1u16.to_le_bytes());
        out[6..8].copy_from_slice(&1u16.to_le_bytes());
        out[8..10].copy_from_slice(&(origin.x as i16).to_le_bytes());
        out[10..12].copy_from_slice(&(origin.y as i16).to_le_bytes());
        out[12..].copy_from_slice(&px);
        out
    }

    #[test]
    fn sm65_origin_maps_display_pixels() {
        let blob = sm65_pixel(Point::new(ROOM_LEFT, 3), rgb888_to_rgb565_be(255, 0, 0));
        let img = Rgb565Image::from_sm65(&blob).expect("sm65");
        assert_eq!(img.origin, Point::new(ROOM_LEFT, 3));
        assert_eq!(
            img.pixel(ROOM_LEFT, 3).map(|c| (c.r(), c.g(), c.b())),
            Some((31, 0, 0))
        );
        assert!(img.pixel(0, 0).is_none());
    }

    #[test]
    fn legacy_sm65_stays_at_origin_zero() {
        let px = rgb888_to_rgb565_be(0, 255, 0);
        let mut blob = [0u8; 10];
        blob[..4].copy_from_slice(b"SM65");
        blob[4..6].copy_from_slice(&1u16.to_le_bytes());
        blob[6..8].copy_from_slice(&1u16.to_le_bytes());
        blob[8..].copy_from_slice(&px);
        let img = Rgb565Image::from_sm65(&blob).expect("legacy");
        assert_eq!(img.origin, Point::zero());
        assert!(img.pixel(0, 0).is_some());
    }

    #[test]
    fn blit_rect_only_writes_the_bitmap() {
        let px = rgb888_to_rgb565_be(255, 255, 255);
        let img = Rgb565Image::from_pixels(1, 1, &px)
            .unwrap()
            .with_origin(Point::new(2, 1));
        let mut buf = [Rgb565::BLACK; 12];
        let mut display = SliceDisplay::new(&mut buf, 4, 3);
        img.blit_rect(&mut display, Rectangle::new(Point::zero(), Size::new(4, 3)))
            .unwrap();
        assert_eq!(buf[1 * 4 + 2], Rgb565::WHITE);
        assert_eq!(buf.iter().filter(|c| **c == Rgb565::WHITE).count(), 1);
    }
}
