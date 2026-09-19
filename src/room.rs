//! Adjacent rooms: each has a backdrop and optional left/right neighbors.
//!
//! The first slice is a linear pair. Home is the existing scene; Blue is a
//! solid-color room to the right. A screen-edge hit with a neighbor moves the
//! stickman; a dead-end edge still bounces.

use crate::assets::{Backdrop, Rgb565Image};
use crate::behavior::event::Rng32;
use crate::collision::CollisionKind;
use crate::DISPLAY_WIDTH;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

/// How many rooms exist in this build.
pub const ROOM_COUNT: usize = 2;

/// Identifies a room. Add a variant and a [`NEIGHBORS`] row to grow the map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomId {
    /// Existing scene (imported / embedded backdrop, dog lives here).
    Home = 0,
    /// Solid blue room to the right of [`Self::Home`].
    Blue = 1,
}

/// `(left, right)` neighbor for each [`RoomId`] index. `None` is a hard wall.
const NEIGHBORS: [(Option<RoomId>, Option<RoomId>); ROOM_COUNT] =
    [(None, Some(RoomId::Blue)), (Some(RoomId::Home), None)];

impl RoomId {
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Room through this screen edge, if the map continues that way.
    pub fn neighbor(self, kind: CollisionKind) -> Option<RoomId> {
        let (left, right) = NEIGHBORS[self.index()];
        match kind {
            CollisionKind::EdgeLeft => left,
            CollisionKind::EdgeRight => right,
            _ => None,
        }
    }

    /// Layer-0 fill for this room. Home uses the installed image, or black.
    pub fn backdrop(self, home_image: Option<Rgb565Image<'static>>) -> Backdrop {
        match self {
            Self::Home => match home_image {
                Some(img) => Backdrop::Image(img),
                None => Backdrop::Color(Rgb565::BLACK),
            },
            Self::Blue => Backdrop::Color(Rgb565::BLUE),
        }
    }
}

/// Random X on the floor, inset so a crate stays fully on-screen.
pub fn random_floor_x(rng: &mut Rng32, margin: i32) -> i32 {
    let lo = margin;
    let hi = DISPLAY_WIDTH as i32 - margin;
    let span = (hi - lo).max(1);
    lo + (rng.next_u32() as i32).rem_euclid(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_exits_right_into_blue() {
        assert_eq!(
            RoomId::Home.neighbor(CollisionKind::EdgeRight),
            Some(RoomId::Blue)
        );
        assert_eq!(RoomId::Home.neighbor(CollisionKind::EdgeLeft), None);
    }

    #[test]
    fn blue_exits_left_into_home() {
        assert_eq!(
            RoomId::Blue.neighbor(CollisionKind::EdgeLeft),
            Some(RoomId::Home)
        );
        assert_eq!(RoomId::Blue.neighbor(CollisionKind::EdgeRight), None);
    }

    #[test]
    fn blue_backdrop_is_solid_blue() {
        assert_eq!(RoomId::Blue.backdrop(None), Backdrop::Color(Rgb565::BLUE));
    }

    #[test]
    fn random_floor_x_stays_inset() {
        let mut rng = Rng32::new(1);
        let margin = 32;
        for _ in 0..32 {
            let x = random_floor_x(&mut rng, margin);
            assert!(x >= margin);
            assert!(x <= DISPLAY_WIDTH as i32 - margin);
        }
    }
}
