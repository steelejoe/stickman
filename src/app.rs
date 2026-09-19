//! Application state, main loop, and input handling (device build).

use crate::game::Game;
use crate::hardware::buttons::Button;
use crate::hardware::touch::Cst816Touch;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    peripherals::{GPIO0, GPIO2, GPIO3, GPIO5, GPIO6, GPIO7, GPIO17, GPIO18, GPIO38, GPIO47, GPIO48, I2C0, SPI2},
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
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

/// Display / input pins kept on core 0 (Wi-Fi/USB take the rest in `main`).
pub struct AppPins {
    pub gpio0: GPIO0<'static>,
    pub gpio2: GPIO2<'static>,
    pub gpio3: GPIO3<'static>,
    pub gpio5: GPIO5<'static>,
    pub gpio6: GPIO6<'static>,
    pub gpio7: GPIO7<'static>,
    pub gpio17: GPIO17<'static>,
    pub gpio18: GPIO18<'static>,
    pub gpio38: GPIO38<'static>,
    pub gpio47: GPIO47<'static>,
    pub gpio48: GPIO48<'static>,
    pub spi2: SPI2<'static>,
    pub i2c0: I2C0<'static>,
}

pub struct App {
    display: RM67162<'static, Output<'static>>,
    game: Game,
    button: Option<Button<Input<'static>>>,
    touch: Option<Cst816Touch<I2c<'static, esp_hal::Blocking>>>,
    /// GPIO38 must stay high on newer T-Display-S3 AMOLED boards (display power / LED rail).
    _display_enable: Output<'static>,
}

impl App {
    pub fn new(pins: AppPins) -> Self {
        let mut delay = Delay::new();

        // Newer board revisions require GPIO38 high before the panel will light.
        let display_enable = Output::new(pins.gpio38, Level::High, OutputConfig::default());

        let sclk = pins.gpio47;
        let rst = pins.gpio17;
        let cs = pins.gpio6;
        let d0 = pins.gpio18;
        let d1 = pins.gpio7;
        let d2 = pins.gpio48;
        let d3 = pins.gpio5;

        let cs = Output::new(cs, Level::High, OutputConfig::default());
        let mut rst = Output::new(rst, Level::High, OutputConfig::default());

        let spi = Spi::new(
            pins.spi2,
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
            pins.i2c0,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        ) {
            Ok(i2c) => {
                let i2c = i2c.with_sda(pins.gpio3).with_scl(pins.gpio2);
                let mut touch = Cst816Touch::new(i2c);
                match touch.disable_auto_sleep() {
                    Ok(()) => esp_println::println!("Touch: auto-sleep disabled"),
                    Err(_) => esp_println::println!(
                        "Touch: auto-sleep disable failed (will retry via polls)"
                    ),
                }
                match touch.read() {
                    Ok(_) => esp_println::println!(
                        "Touch: CST816 ready (tap an entity; empty space: random stickman)"
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
            pins.gpio0,
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

    pub async fn run(&mut self) -> ! {
        esp_println::println!("Stickman running!");
        esp_println::println!(
            "Tap an entity to roll its table; empty tap picks a random stickman behavior. BOOT cycles stickman behaviors. Looping clips roll a finished table after each cycle."
        );
        let mut last_tick = Instant::now();
        let frame_duration = Duration::from_millis(FRAME_MS);

        loop {
            // Measure delta from the previous frame start so draw time counts.
            let frame_start = Instant::now();
            let elapsed = (frame_start - last_tick).as_millis();
            last_tick = frame_start;
            let delta_ms = elapsed.min(MAX_DELTA_MS).max(1);

            if let Some(ref mut touch) = self.touch {
                if let Some(point) = touch.poll_tap() {
                    esp_println::println!("Touch: ({}, {})", point.x, point.y);
                    self.game.on_tap(point.x as u32, point.y as u32);
                }
            }
            // BOOT button → next behavior (no position).
            if let Some(ref mut btn) = self.button {
                if btn.poll_pressed() {
                    esp_println::println!("Button: cycle behavior");
                    self.game.on_cycle_input();
                }
            }

            while let Some(cmd) = crate::net::try_recv_config() {
                self.game.apply_config(cmd);
            }

            self.game.update(delta_ms);
            self.game.draw(&mut self.display).unwrap();

            // Yield to the core-0 RTOS/radio tasks instead of a busy-wait.
            let _ = frame_duration;
            embassy_time::Timer::after(embassy_time::Duration::from_millis(FRAME_MS)).await;
        }
    }
}
