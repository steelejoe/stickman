//! Application state, main loop, and input handling (device build).

use crate::game::{tap_action_for_x, Game};
use crate::hardware::buttons::Button;
use crate::hardware::touch::Cst816Touch;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    peripherals::Peripherals,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::{Duration, Instant, Rate},
};
use t_display_s3_amoled::rm67162::{Orientation, RM67162};

/// Display dimensions (landscape: 536x240)
pub const DISPLAY_WIDTH: u32 = crate::DISPLAY_WIDTH;
pub const DISPLAY_HEIGHT: u32 = crate::DISPLAY_HEIGHT;

/// Target frame duration in milliseconds (~30 fps)
const FRAME_MS: u64 = 33;
const MAX_DELTA_MS: u64 = 100;

pub struct App {
    display: RM67162<'static, Output<'static>>,
    game: Game,
    button: Option<Button<Input<'static>>>,
    touch: Option<Cst816Touch<I2c<'static, esp_hal::Blocking>>>,
    /// GPIO38 must stay high on newer T-Display-S3 AMOLED boards (display power / LED rail).
    _display_enable: Output<'static>,
}

impl App {
    pub fn new(peripherals: Peripherals) -> Self {
        let mut delay = Delay::new();

        // Newer board revisions require GPIO38 high before the panel will light.
        let display_enable = Output::new(
            peripherals.GPIO38,
            Level::High,
            OutputConfig::default(),
        );

        let sclk = peripherals.GPIO47;
        let rst = peripherals.GPIO17;
        let cs = peripherals.GPIO6;
        let d0 = peripherals.GPIO18;
        let d1 = peripherals.GPIO7;
        let d2 = peripherals.GPIO48;
        let d3 = peripherals.GPIO5;

        let cs = Output::new(cs, Level::High, OutputConfig::default());
        let mut rst = Output::new(rst, Level::High, OutputConfig::default());

        let spi = Spi::new(
            peripherals.SPI2,
            SpiConfig::default()
                .with_frequency(Rate::from_mhz(75))
                .with_mode(Mode::_0),
        )
        .unwrap()
        .with_sck(sclk)
        .with_sio0(d0)
        .with_sio1(d1)
        .with_sio2(d2)
        .with_sio3(d3);

        let mut display = RM67162::new(spi, cs);
        esp_println::println!("Resetting display...");
        display.reset(&mut rst, &mut delay).unwrap();
        esp_println::println!("Init display...");
        display.init(&mut delay).unwrap();
        display
            .set_orientation(Orientation::LandscapeFlipped)
            .unwrap();

        // One full clear at boot (slow over QSPI). Per-frame clears are avoided in run().
        esp_println::println!("Clearing display (may take a few seconds)...");
        display.clear(Rgb565::BLACK).unwrap();
        esp_println::println!("Display ready");

        // CST816 on LilyGo 1.91" AMOLED Touch: SDA=GPIO3, SCL=GPIO2, IRQ=GPIO21.
        // Keep the driver even if the first read fails — the chip often starts asleep.
        let touch = match I2c::new(
            peripherals.I2C0,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        ) {
            Ok(i2c) => {
                let i2c = i2c.with_sda(peripherals.GPIO3).with_scl(peripherals.GPIO2);
                let mut touch = Cst816Touch::new(i2c);
                match touch.disable_auto_sleep() {
                    Ok(()) => esp_println::println!("Touch: auto-sleep disabled"),
                    Err(_) => esp_println::println!("Touch: auto-sleep disable failed (will retry via polls)"),
                }
                match touch.read() {
                    Ok(_) => esp_println::println!(
                        "Touch: CST816 ready (left/right: face, center: random behavior)"
                    ),
                    Err(_) => esp_println::println!(
                        "Touch: probe failed at boot; taps may still work after contact"
                    ),
                }
                Some(touch)
            }
            Err(_) => {
                esp_println::println!("Touch: I2C init failed");
                None
            }
        };

        // BOOT button (GPIO0) as a secondary cycle input.
        // GPIO21 is the CST816 IRQ — do not claim it as a GPIO button on touch boards.
        let button = Some(Button::new(Input::new(
            peripherals.GPIO0,
            InputConfig::default().with_pull(Pull::Up),
        )));

        Self {
            display,
            game: Game::new(),
            button,
            touch,
            _display_enable: display_enable,
        }
    }

    pub fn run(&mut self) -> ! {
        esp_println::println!("Stickman running!");
        esp_println::println!(
            "Tap left/right to face; center tap picks a random other behavior; BOOT cycles: walk → idle → jump → crouch → search → begging → sword → stab → crouch-sword → crouch-stab → knockback → tumble"
        );
        let mut last_tick = Instant::now();
        let frame_duration = Duration::from_millis(FRAME_MS);

        loop {
            // Measure delta from the previous frame start so draw time counts.
            let frame_start = Instant::now();
            let elapsed = (frame_start - last_tick).as_millis();
            last_tick = frame_start;
            let delta_ms = elapsed.min(MAX_DELTA_MS).max(1);

            // Positioned touch tap: left/right face, center picks a random other behavior.
            if let Some(ref mut touch) = self.touch {
                if let Some(point) = touch.poll_tap() {
                    let action = tap_action_for_x(point.x as u32, DISPLAY_WIDTH);
                    esp_println::println!(
                        "Touch: ({}, {}) -> {:?}",
                        point.x,
                        point.y,
                        action
                    );
                    self.game.on_tap(point.x as u32);
                }
            }
            // BOOT button → next behavior (no position).
            if let Some(ref mut btn) = self.button {
                if btn.poll_pressed() {
                    esp_println::println!("Button: cycle behavior");
                    self.game.on_cycle_input();
                }
            }

            self.game.update(delta_ms);
            self.game.draw(&mut self.display).unwrap();

            // Pace only when a frame finishes faster than the target.
            while frame_start.elapsed() < frame_duration {}
        }
    }
}
