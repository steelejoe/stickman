//! Short spoken lines for speech-bubble behaviors.

use crate::behavior::event::Rng32;

/// Stickman talk bank. Short and a bit silly.
pub const STICKMAN_LINES: &[&str] = &[
    "Ouch!", "Hey!", "Oops", "Huh?", "Wait", "Boop", "Nope", "Yo!", "What", "Whoa", "Eep!", "Bonk",
    "Yeet", "Bruh", "Uh oh", "Whee!", "Hmm", "Hi!", "Ow", "Yikes", "Phew", "Nah", "Again?",
    "Watch it", "My foot!", "Beep", "Honk", "Gotcha", "Why??", "Help", "Snack?", "Bored", "Ta-da",
    "Shh", "Wow", "Gah", "Ack", "Zoom!", "Nice", "Maybe?",
];

/// Crate talk bank. Even shorter, and rarer in the tables.
pub const BOX_LINES: &[&str] = &[
    "sigh...", "(fart)", "Ouch!", "thud", "...", "oof", "creak", "heavy", "meh", "zip", "whee",
    "stuck?",
];

pub fn pick_line(rng: &mut Rng32, lines: &[&'static str]) -> &'static str {
    debug_assert!(!lines.is_empty());
    lines[rng.next_u32() as usize % lines.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stickman_bank_has_ouch_and_hey() {
        assert!(STICKMAN_LINES.contains(&"Ouch!"));
        assert!(STICKMAN_LINES.contains(&"Hey!"));
        assert!(STICKMAN_LINES.len() >= 8);
        for line in STICKMAN_LINES {
            assert!(!line.is_empty());
            assert!(line.len() <= 16, "{line}");
        }
    }

    #[test]
    fn box_bank_has_the_required_lines() {
        assert!(BOX_LINES.contains(&"sigh..."));
        assert!(BOX_LINES.contains(&"(fart)"));
        assert!(BOX_LINES.contains(&"Ouch!"));
        for line in BOX_LINES {
            assert!(!line.is_empty());
            assert!(line.len() <= 16, "{line}");
        }
    }

    #[test]
    fn pick_line_stays_in_the_bank() {
        let mut rng = Rng32::new(7);
        for _ in 0..64 {
            let line = pick_line(&mut rng, STICKMAN_LINES);
            assert!(STICKMAN_LINES.contains(&line));
        }
    }
}
