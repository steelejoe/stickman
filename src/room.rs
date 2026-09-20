//! Adjacent rooms: each has a backdrop and optional left/right neighbors.
//!
//! Up to [`MAX_ROOMS`] sit in a line. Home is always first; extra rooms are
//! added from the config page. A screen-edge hit with a neighbor moves the
//! stickman; a dead-end edge still bounces.

use crate::assets::Rgb565Image;
use crate::behavior::event::Rng32;
use crate::collision::CollisionKind;
use crate::config::ParseError;
use crate::menu::ROOM_LEFT;
use crate::DISPLAY_WIDTH;
use heapless::String;
use serde::Deserialize;

/// Maximum rooms the player can configure (Home plus two extras).
pub const MAX_ROOMS: usize = 3;
/// Array length for per-room state (always the maximum).
pub const ROOM_COUNT: usize = MAX_ROOMS;
/// Room backdrop width (display minus the menu strip).
pub const ROOM_BG_W: u16 = 480;
/// Room backdrop height.
pub const ROOM_BG_H: u16 = 240;
/// Packed SM65 size for a full room image (`SM65` header + pixels).
pub const SM65_ROOM_LEN: usize = 12 + (ROOM_BG_W as usize) * (ROOM_BG_H as usize) * 2;
/// Max characters in a room name.
pub const ROOM_NAME_MAX: usize = 24;

pub type RoomName = String<ROOM_NAME_MAX>;

/// Identifies a room. Indices stay stable as the active count grows from 1 to 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomId {
    /// Existing scene (imported / embedded backdrop, dog lives here).
    Home = 0,
    /// First extra room, to the right of [`Self::Home`].
    Two = 1,
    /// Second extra room, to the right of [`Self::Two`].
    Three = 2,
}

impl RoomId {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Home),
            1 => Some(Self::Two),
            2 => Some(Self::Three),
            _ => None,
        }
    }

    /// Room through this screen edge, if `active` rooms continue that way.
    pub fn neighbor(self, kind: CollisionKind, active: u8) -> Option<RoomId> {
        let n = active.clamp(1, MAX_ROOMS as u8);
        match (self, kind) {
            (Self::Home, CollisionKind::EdgeRight) if n >= 2 => Some(Self::Two),
            (Self::Two, CollisionKind::EdgeLeft) if n >= 2 => Some(Self::Home),
            (Self::Two, CollisionKind::EdgeRight) if n >= 3 => Some(Self::Three),
            (Self::Three, CollisionKind::EdgeLeft) if n >= 3 => Some(Self::Two),
            _ => None,
        }
    }
}

/// Default label for slot `i` (0 = Home).
pub fn default_room_name(i: usize) -> &'static str {
    match i {
        0 => "Home",
        1 => "Room 2",
        2 => "Room 3",
        _ => "Room",
    }
}

/// Names and how many rooms are actually linked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomsConfig {
    pub count: u8,
    pub names: [RoomName; MAX_ROOMS],
}

impl Default for RoomsConfig {
    fn default() -> Self {
        let mut names: [RoomName; MAX_ROOMS] = core::array::from_fn(|_| RoomName::new());
        for (i, name) in names.iter_mut().enumerate() {
            let _ = name.push_str(default_room_name(i));
        }
        Self { count: 1, names }
    }
}

impl RoomsConfig {
    pub fn clamped_count(&self) -> u8 {
        self.count.clamp(1, MAX_ROOMS as u8)
    }
}

/// Apply names, count, and per-room images on the game core.
#[derive(Clone, Debug)]
pub struct RoomsUpdate {
    pub count: u8,
    pub names: [RoomName; MAX_ROOMS],
    pub images: [Option<Rgb565Image<'static>>; MAX_ROOMS],
}

impl Default for RoomsUpdate {
    fn default() -> Self {
        let cfg = RoomsConfig::default();
        Self {
            count: cfg.count,
            names: cfg.names,
            images: [None; MAX_ROOMS],
        }
    }
}

/// Save room names/count or restore Home-only defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomsAction {
    Save(RoomsConfig),
    Reset,
}

#[derive(Deserialize)]
struct RoomsPost<'a> {
    #[serde(default)]
    count: Option<u8>,
    #[serde(borrow)]
    #[serde(default)]
    n0: Option<&'a str>,
    #[serde(borrow)]
    #[serde(default)]
    n1: Option<&'a str>,
    #[serde(borrow)]
    #[serde(default)]
    n2: Option<&'a str>,
    #[serde(default)]
    reset: Option<bool>,
}

/// Parse `POST /api/rooms` JSON.
pub fn parse_rooms_json(bytes: &[u8]) -> Result<RoomsAction, ParseError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;
    let text = text.trim_start_matches(['\u{feff}', '\0']).trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    let (file, _): (RoomsPost<'_>, _) =
        serde_json_core::from_str(text).map_err(|_| ParseError::InvalidJson)?;
    if file.reset == Some(true) {
        return Ok(RoomsAction::Reset);
    }
    let count = file.count.unwrap_or(1).clamp(1, MAX_ROOMS as u8);
    let raw = [file.n0, file.n1, file.n2];
    let mut names: [RoomName; MAX_ROOMS] = core::array::from_fn(|_| RoomName::new());
    for (i, name) in names.iter_mut().enumerate() {
        *name = sanitize_room_name(raw[i].unwrap_or(""), default_room_name(i));
    }
    Ok(RoomsAction::Save(RoomsConfig { count, names }))
}

fn sanitize_room_name(raw: &str, fallback: &str) -> RoomName {
    let mut out = RoomName::new();
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') {
            if out.push(c).is_err() {
                break;
            }
        }
    }
    if out.is_empty() {
        let _ = out.push_str(fallback);
    }
    out
}

/// Random X on the floor, inset so a crate stays fully on-screen.
pub fn random_floor_x(rng: &mut Rng32, margin: i32) -> i32 {
    let lo = ROOM_LEFT + margin;
    let hi = DISPLAY_WIDTH as i32 - margin;
    let span = (hi - lo).max(1);
    lo + (rng.next_u32() as i32).rem_euclid(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_room_has_no_exits() {
        assert_eq!(RoomId::Home.neighbor(CollisionKind::EdgeRight, 1), None);
        assert_eq!(RoomId::Home.neighbor(CollisionKind::EdgeLeft, 1), None);
    }

    #[test]
    fn two_rooms_link_home_and_two() {
        assert_eq!(
            RoomId::Home.neighbor(CollisionKind::EdgeRight, 2),
            Some(RoomId::Two)
        );
        assert_eq!(
            RoomId::Two.neighbor(CollisionKind::EdgeLeft, 2),
            Some(RoomId::Home)
        );
        assert_eq!(RoomId::Two.neighbor(CollisionKind::EdgeRight, 2), None);
    }

    #[test]
    fn three_rooms_are_a_line() {
        assert_eq!(
            RoomId::Two.neighbor(CollisionKind::EdgeRight, 3),
            Some(RoomId::Three)
        );
        assert_eq!(
            RoomId::Three.neighbor(CollisionKind::EdgeLeft, 3),
            Some(RoomId::Two)
        );
        assert_eq!(RoomId::Three.neighbor(CollisionKind::EdgeRight, 3), None);
        assert_eq!(RoomId::Home.neighbor(CollisionKind::EdgeLeft, 3), None);
    }

    #[test]
    fn parses_room_save() {
        let RoomsAction::Save(cfg) =
            parse_rooms_json(br#"{"count":2,"n0":"Home","n1":"Garden"}"#).unwrap()
        else {
            panic!("expected save");
        };
        assert_eq!(cfg.count, 2);
        assert_eq!(cfg.names[0].as_str(), "Home");
        assert_eq!(cfg.names[1].as_str(), "Garden");
        assert_eq!(cfg.names[2].as_str(), "Room 3");
    }

    #[test]
    fn parses_room_reset() {
        assert_eq!(
            parse_rooms_json(br#"{"reset":true}"#).unwrap(),
            RoomsAction::Reset
        );
    }

    #[test]
    fn sanitizes_room_names() {
        let RoomsAction::Save(cfg) = parse_rooms_json(br#"{"count":1,"n0":" <Home!> "}"#).unwrap()
        else {
            panic!("expected save");
        };
        assert_eq!(cfg.names[0].as_str(), "Home");
    }

    #[test]
    fn random_floor_x_stays_inset() {
        let mut rng = Rng32::new(1);
        let margin = 32;
        for _ in 0..32 {
            let x = random_floor_x(&mut rng, margin);
            assert!(x >= ROOM_LEFT + margin);
            assert!(x <= DISPLAY_WIDTH as i32 - margin);
        }
    }
}
