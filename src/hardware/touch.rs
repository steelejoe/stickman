//! CST816x capacitive touch controller via I2C.
//!
//! Minimal blocking driver for detecting touch events (cycle behavior).
//! LilyGo T-Display-S3 AMOLED Touch (1.91"): SDA=GPIO3, SCL=GPIO2, IRQ=GPIO21.

use embedded_hal::i2c::I2c;

/// CST816 I2C address
const CST816_ADDR: u8 = 0x15;

/// Register addresses
const REG_POINTS: u8 = 0x02;

/// Touch point data
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

    /// Read current touch point, if any.
    /// Reads registers 0x02-0x06: points, XH, XL, YH, YL.
    pub fn read(&mut self) -> Result<TouchPoint, I2C::Error> {
        let mut buf = [0u8; 5];
        self.i2c.write_read(CST816_ADDR, &[REG_POINTS], &mut buf)?;
        let points = buf[0] & 0x0F;
        let xh = buf[1];
        let xl = buf[2];
        let yh = buf[3];
        let yl = buf[4];

        let x = ((xh as u16 & 0x0F) << 8) | xl as u16;
        let y = ((yh as u16 & 0x0F) << 8) | yl as u16;

        Ok(TouchPoint {
            x,
            y,
            pressed: points > 0,
        })
    }

    /// Returns true on the rising edge of a touch (finger just down).
    pub fn poll_pressed(&mut self) -> bool {
        let pressed = self.read().map(|p| p.pressed).unwrap_or(false);
        let edge = pressed && !self.prev_pressed;
        self.prev_pressed = pressed;
        edge
    }
}
