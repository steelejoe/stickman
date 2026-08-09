//! Physical button inputs (GPIO0 Boot, GPIO21).

use embedded_hal::digital::InputPin;

/// Debounce threshold: consecutive reads
const DEBOUNCE_COUNT: u8 = 3;

/// Button input with simple debouncing.
pub struct Button<P> {
    pin: P,
    count: u8,
    prev_pressed: bool,
}

impl<P> Button<P>
where
    P: InputPin,
{
    pub fn new(pin: P) -> Self {
        Self {
            pin,
            count: 0,
            prev_pressed: false,
        }
    }

    /// Returns true on rising edge (button just pressed).
    pub fn poll_pressed(&mut self) -> bool {
        let low = self.pin.is_low().unwrap_or(true);
        let pressed = low;
        if pressed {
            if self.count < DEBOUNCE_COUNT {
                self.count += 1;
            }
            if self.count >= DEBOUNCE_COUNT && !self.prev_pressed {
                self.prev_pressed = true;
                return true;
            }
        } else {
            self.count = 0;
            self.prev_pressed = false;
        }
        false
    }
}
