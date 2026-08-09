//! Desktop simulation: windowed stickman with keyboard/mouse input.
//!
//! Input mapping:
//!   Spacebar          → GPIO21 button (cycle behavior)
//!   Left mouse click  → touch tap (cycle behavior)
//!   Escape / close    → quit
//!
//! Background: loads `assets/background.png` (preferred) or
//! `assets/background.rgb565` at runtime from the project tree.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use minifb::{Key, MouseButton, Scale, Window, WindowOptions};
use stickman::assets::{rgb888_to_rgb565_be, Rgb565Image};
use stickman::game::Game;
use stickman::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
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

fn asset_candidates(file_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    // Prefer the crate root so `make sim` works regardless of cwd quirks.
    paths.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(file_name));
    paths.push(Path::new("assets").join(file_name));
    paths
}

fn first_existing(file_name: &str) -> Option<PathBuf> {
    asset_candidates(file_name).into_iter().find(|p| p.is_file())
}

fn leak_pixels(pixels: Vec<u8>) -> &'static [u8] {
    Box::leak(pixels.into_boxed_slice())
}

fn load_background_png(path: &Path) -> Result<Rgb565Image<'static>, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("png header {}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("png frame {}: {e}", path.display()))?;
    let width = info.width as u16;
    let height = info.height as u16;
    let src = &buf[..info.buffer_size()];

    let mut pixels = Vec::with_capacity((width as usize) * (height as usize) * 2);
    match info.color_type {
        png::ColorType::Rgb => {
            for chunk in src.chunks_exact(3) {
                pixels.extend_from_slice(&rgb888_to_rgb565_be(chunk[0], chunk[1], chunk[2]));
            }
        }
        png::ColorType::Rgba => {
            for chunk in src.chunks_exact(4) {
                // Composite onto black (matches import-image RGB565 conversion).
                let a = chunk[3] as u16;
                let r = (chunk[0] as u16 * a / 255) as u8;
                let g = (chunk[1] as u16 * a / 255) as u8;
                let b = (chunk[2] as u16 * a / 255) as u8;
                pixels.extend_from_slice(&rgb888_to_rgb565_be(r, g, b));
            }
        }
        png::ColorType::Grayscale => {
            for &g in src {
                pixels.extend_from_slice(&rgb888_to_rgb565_be(g, g, g));
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for chunk in src.chunks_exact(2) {
                let a = chunk[1] as u16;
                let g = (chunk[0] as u16 * a / 255) as u8;
                pixels.extend_from_slice(&rgb888_to_rgb565_be(g, g, g));
            }
        }
        other => return Err(format!("unsupported PNG color type: {other:?}")),
    }

    Rgb565Image::from_pixels(width, height, leak_pixels(pixels))
        .ok_or_else(|| format!("invalid PNG dimensions in {}", path.display()))
}

fn load_background_rgb565(path: &Path) -> Result<Rgb565Image<'static>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let leaked = leak_pixels(bytes);
    Rgb565Image::from_sm65(leaked)
        .ok_or_else(|| format!("invalid SM65 image: {}", path.display()))
}

fn load_sim_background() -> Option<Rgb565Image<'static>> {
    if let Some(path) = first_existing("background.png") {
        match load_background_png(&path) {
            Ok(img) => {
                println!(
                    "Background: loaded {} ({}x{})",
                    path.display(),
                    img.width,
                    img.height
                );
                return Some(img);
            }
            Err(e) => println!("Background: failed to load PNG ({e})"),
        }
    }

    if let Some(path) = first_existing("background.rgb565") {
        match load_background_rgb565(&path) {
            Ok(img) => {
                println!(
                    "Background: loaded {} ({}x{})",
                    path.display(),
                    img.width,
                    img.height
                );
                return Some(img);
            }
            Err(e) => println!("Background: failed to load RGB565 ({e})"),
        }
    }

    None
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
    match load_sim_background() {
        Some(img) => game.set_background(img),
        None => {
            if game.has_background_image() {
                println!("Background: using embedded assets/background.rgb565");
            } else {
                println!(
                    "Background: none found. Import one with:\n  make import IMAGE=your.png NAME=background"
                );
            }
        }
    }

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
        // Incremental draw: erase previous pose + dirty floor restore (no full clear).
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
