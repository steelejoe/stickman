//! Three depth layers for 2.5D compositing (back → front).
//!
//! | Layer | Role |
//! |-------|------|
//! | 0 [`LayerId::Background`] | Backdrop image + ground line |
//! | 1 [`LayerId::Middle`] | Middleground; stickman may draw here |
//! | 2 [`LayerId::Foreground`] | Foreground overlays; stickman may draw here |
//!
//! Layers are composited in order 0 → 1 → 2. An optional background image is
//! drawn once on layer 0; later frames restore only the dirty rectangle under
//! the previous stickman pose.

use crate::assets::Rgb565Image;
use crate::stickman::geometry::StickmanState;
use crate::stickman::render;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

/// Depth layer index (0 = farthest back, 2 = nearest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LayerId {
    Background = 0,
    Middle = 1,
    Foreground = 2,
}

impl LayerId {
    pub const COUNT: usize = 3;

    /// Layers the stickman is allowed to occupy.
    pub const fn is_stickman_layer(self) -> bool {
        matches!(self, Self::Middle | Self::Foreground)
    }

    /// Clamp to a valid stickman layer (background → middle).
    pub const fn clamp_stickman(self) -> Self {
        match self {
            Self::Background => Self::Middle,
            other => other,
        }
    }
}

impl Default for LayerId {
    fn default() -> Self {
        Self::Middle
    }
}

/// Draw static background content (layer 0).
///
/// Paints `background` when present, then the ground line.
pub fn draw_background<D>(
    display: &mut D,
    background: Option<&Rgb565Image<'_>>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    if let Some(img) = background {
        img.draw(display, Point::zero())?;
    }
    render::draw_floor(display)
}

/// Clear the previous stickman and restore layer-0 pixels under it.
///
/// With a background image, the dirty rectangle is blitted from the asset.
/// Otherwise the pose is erased in black. The floor span is always redrawn.
pub fn restore_background_under_stickman<D>(
    display: &mut D,
    prev: &StickmanState,
    background: Option<&Rgb565Image<'_>>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    if let Some(img) = background {
        img.blit_rect(display, render::stickman_dirty_rect(prev))?;
        // Full floor line is cheap (one scanline) and keeps ground continuous.
        render::draw_floor(display)?;
    } else {
        render::draw_stickman_colored(display, prev, Rgb565::BLACK)?;
        let (x0, x1) = render::stickman_floor_dirty_x(prev);
        render::draw_floor_span(display, x0, x1)?;
    }
    Ok(())
}

/// Draw the stickman onto its depth layer (1 or 2). Does not touch layer 0.
pub fn draw_stickman_layer<D, F>(
    display: &mut D,
    stickman: &StickmanState,
    mut draw_stickman: F,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
    F: FnMut(&mut D, &StickmanState) -> Result<(), D::Error>,
{
    let layer = stickman.layer.clamp_stickman();
    match layer {
        LayerId::Middle | LayerId::Foreground => draw_stickman(display, stickman),
        LayerId::Background => Ok(()),
    }
}

/// Full back-to-front compose (background + stickman).
pub fn compose<D, F>(
    display: &mut D,
    stickman: &StickmanState,
    background: Option<&Rgb565Image<'_>>,
    draw_stickman: F,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
    F: FnMut(&mut D, &StickmanState) -> Result<(), D::Error>,
{
    draw_background(display, background)?;
    draw_stickman_layer(display, stickman, draw_stickman)
}
