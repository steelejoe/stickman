//! Desktop simulation: windowed stickman with keyboard/mouse input.
//!
//! Input mapping:
//!   Spacebar          → GPIO21 button (cycle behavior)
//!   Left mouse click  → touch tap (cycle behavior)
//!   Escape / close    → quit

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use minifb::{Key, MouseButton, Scale, Window, WindowOptions};
use stickman::game::Game;
use stickman::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use std::time::{Duration, Instant};

const FRAME_MS: u64 = 33;
const MAX_DELTA_MS: u64 = 100;

/// In-memory RGB565 framebuffer that implements embedded-graphics DrawTarget.
struct SimDisplay {
    width: u32,
    height: u32,
    pixels: Vec<Rgb565>,
}

impl SimDisplay {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![Rgb565::BLACK; (width * height) as usize],
        }
    }

    fn to_u32_buffer(&self) -> Vec<u32> {
        self.pixels
            .iter()
            .map(|c| {
                let r = c.r() as u32;
                let g = c.g() as u32;
                let b = c.b() as u32;
                // Rgb565 components are 5/6/5; expand to 8-bit for the window.
                let r8 = (r << 3) | (r >> 2);
                let g8 = (g << 2) | (g >> 4);
                let b8 = (b << 3) | (b >> 2);
                (255 << 24) | (r8 << 16) | (g8 << 8) | b8
            })
            .collect()
    }
}

impl OriginDimensions for SimDisplay {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for SimDisplay {
    type Color = Rgb565;
    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let w = self.width as i32;
        let h = self.height as i32;
        for Pixel(coord, color) in pixels {
            let x = coord.x;
            let y = coord.y;
            if x >= 0 && x < w && y >= 0 && y < h {
                self.pixels[(y as u32 * self.width + x as u32) as usize] = color;
            }
        }
        Ok(())
    }
}

fn main() {
    let width = DISPLAY_WIDTH as usize;
    let height = DISPLAY_HEIGHT as usize;

    let mut window = Window::new(
        "Stickman Sim — Space/Click: cycle behavior · Esc: quit",
        width,
        height,
        WindowOptions {
            resize: false,
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("failed to open simulation window (is DISPLAY set?)");

    window.set_target_fps(30);

    let mut display = SimDisplay::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
    let mut game = Game::new();

    let mut last_frame = Instant::now();
    let mut prev_space = false;
    let mut prev_mouse = false;

    println!("Stickman simulation running.");
    println!("  Spacebar         → cycle behavior (GPIO21 button)");
    println!("  Left mouse click → cycle behavior (touch tap)");
    println!("  Escape / close   → quit");

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let elapsed = now.duration_since(last_frame);
        last_frame = now;
        let delta_ms = elapsed.as_millis().min(MAX_DELTA_MS as u128) as u64;

        let space = window.is_key_down(Key::Space);
        let mouse = window.get_mouse_down(MouseButton::Left);
        let cycle = (space && !prev_space) || (mouse && !prev_mouse);
        prev_space = space;
        prev_mouse = mouse;

        if cycle {
            game.on_cycle_input();
        }

        game.update(delta_ms);
        // Keep the last framebuffer when the pose is unchanged (idle / paused).
        if !game.is_frame_static() {
            display.clear(Rgb565::BLACK).expect("clear failed");
        }
        game.draw(&mut display).expect("draw failed");

        let buf = display.to_u32_buffer();
        window
            .update_with_buffer(&buf, width, height)
            .expect("failed to update window");

        // Pace roughly to FRAME_MS when the loop runs faster than the display.
        let frame_budget = Duration::from_millis(FRAME_MS);
        let spent = last_frame.elapsed();
        if spent < frame_budget {
            std::thread::sleep(frame_budget - spent);
        }
    }
}
