//! CST816x capacitive touch controller via I2C.
//!
//! LilyGo T-Display-S3 AMOLED Touch (1.91"): SDA=GPIO3, SCL=GPIO2, IRQ=GPIO21.
//! Tap zones: left third faces left, right third faces right, center cycles behavior.

use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use embedded_hal::i2c::I2c;

/// CST816 I2C address
const CST816_ADDR: u8 = 0x15;

/// Register addresses
const REG_POINTS: u8 = 0x02;
/// Write `0xFF` here to keep the chip from entering auto-sleep (no RST pin on this board).
const REG_AUTOSLEEP: u8 = 0xFE;

/// Touch point data in display coordinates (landscape 536×240).
#[derive(Clone, Copy, Debug, Default)]
pub struct TouchPoint {
    pub x: u16,
    pub y: u16,
    pub pressed: bool,
}

/// Minimal CST816 touch driver with edge detection for taps.
pub struct Cst816Touch<I2C> {
    i2c: I2C,
    prev_pressed: bool,
}

impl<I2C> Cst816Touch<I2C>
where
    I2C: I2c,
{
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            prev_pressed: false,
        }
    }

    /// Disable auto-sleep so I2C polls keep working without a hardware reset pin.
    pub fn disable_auto_sleep(&mut self) -> Result<(), I2C::Error> {
        self.i2c.write(CST816_ADDR, &[REG_AUTOSLEEP, 0xFF])
    }

    /// Read current touch point, if any.
    /// Reads registers 0x02-0x06: points, XH, XL, YH, YL.
    ///
    /// On this board CST816 reports landscape-aligned points with X along the
    /// long edge (~0..535) and Y along the short edge (~0..239). Mapped to
    /// display space for LandscapeFlipped (536×240).
    pub fn read(&mut self) -> Result<TouchPoint, I2C::Error> {
        let mut buf = [0u8; 5];
        self.i2c.write_read(CST816_ADDR, &[REG_POINTS], &mut buf)?;
        let points = buf[0] & 0x0F;
        let xh = buf[1];
        let xl = buf[2];
        let yh = buf[3];
        let yl = buf[4];

        let raw_x = ((xh as u16 & 0x0F) << 8) | xl as u16;
        let raw_y = ((yh as u16 & 0x0F) << 8) | yl as u16;
        let (x, y) = map_to_display(raw_x, raw_y);

        Ok(TouchPoint {
            x,
            y,
            pressed: points > 0,
        })
    }

    /// Rising-edge tap with display coordinates, if any.
    pub fn poll_tap(&mut self) -> Option<TouchPoint> {
        let point = self.read().ok()?;
        let edge = point.pressed && !self.prev_pressed;
        self.prev_pressed = point.pressed;
        if edge {
            Some(point)
        } else {
            None
        }
    }
}

/// Map CST816 coords to landscape display pixels (536×240).
fn map_to_display(raw_x: u16, raw_y: u16) -> (u16, u16) {
    // X is already in panel width units (~0..535). Invert for LandscapeFlipped.
    // (Older code scaled as if max were 239, which crushed the center/left into x=0.)
    let x = (DISPLAY_WIDTH as u16 - 1).saturating_sub(raw_x.min(DISPLAY_WIDTH as u16 - 1));
    let y = raw_y.min(DISPLAY_HEIGHT as u16 - 1);
    (x, y)
}
