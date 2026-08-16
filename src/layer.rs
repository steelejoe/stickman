//! Three depth layers for 2.5D compositing (back → front).
//!
//! | Layer | Role |
//! |-------|------|
//! | 0 [`LayerId::Background`] | Backdrop image + ground line |
//! | 1 [`LayerId::Middle`] | Middleground; actor may draw here |
//! | 2 [`LayerId::Foreground`] | Foreground overlays; actor may draw here |
//!
//! Frame presents composite layer 0 + the figure in [`crate::dirty`].
//! [`crate::stickman::ir::Actor::layer`] is reserved for when middle/foreground
//! images exist.

/// Depth layer index (0 = farthest back, 2 = nearest).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LayerId {
    Background = 0,
    #[default]
    Middle = 1,
    Foreground = 2,
}

impl LayerId {
    pub const COUNT: usize = 3;
}
