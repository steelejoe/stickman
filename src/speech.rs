//! Editable speech banks and talk chances (website + flash + game).

use crate::behavior::dialog::{BOX_LINES, DOG_LINES, STICKMAN_LINES};
use crate::behavior::event::Rng32;
use crate::config::ParseError;
use core::fmt::Write as _;
use heapless::String;
use serde::Deserialize;

/// Max characters in one spoken line (speech bubble cap).
pub const LINE_MAX: usize = 16;
/// Max lines per figure.
pub const LINES_MAX: usize = 40;

pub const DEFAULT_MAN_TALK: u8 = 6;
pub const DEFAULT_DOG_TALK: u8 = 8;
pub const DEFAULT_BOX_TALK: u8 = 1;

pub type Line = String<LINE_MAX>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhraseBank {
    pub talk_pct: u8,
    n: u8,
    lines: [Line; LINES_MAX],
}

impl PhraseBank {
    pub fn man() -> Self {
        Self::from_defaults(STICKMAN_LINES, DEFAULT_MAN_TALK)
    }

    pub fn dog() -> Self {
        Self::from_defaults(DOG_LINES, DEFAULT_DOG_TALK)
    }

    pub fn r#box() -> Self {
        Self::from_defaults(BOX_LINES, DEFAULT_BOX_TALK)
    }

    fn from_defaults(src: &[&str], talk_pct: u8) -> Self {
        let mut bank = Self {
            talk_pct: talk_pct.min(100),
            n: 0,
            lines: core::array::from_fn(|_| Line::new()),
        };
        bank.set_lines(src.iter().copied());
        bank
    }

    pub fn set_lines<'a, I>(&mut self, src: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        self.n = 0;
        for raw in src {
            if self.n as usize >= LINES_MAX {
                break;
            }
            let line = trim_line(raw);
            if line.is_empty() {
                continue;
            }
            let mut stored = Line::new();
            let _ = stored.push_str(line);
            self.lines[self.n as usize] = stored;
            self.n += 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines[..self.n as usize]
    }

    pub fn joined_lines(&self) -> alloc::string::String {
        let mut out = alloc::string::String::new();
        for (i, line) in self.lines().iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line.as_str());
        }
        out
    }

    pub fn pick(&self, rng: &mut Rng32, fallback: &[&str]) -> Line {
        if self.n > 0 {
            return self.lines[(rng.next_u32() as usize) % self.n as usize].clone();
        }
        if fallback.is_empty() {
            return Line::new();
        }
        let raw = fallback[(rng.next_u32() as usize) % fallback.len()];
        let mut out = Line::new();
        let _ = out.push_str(trim_line(raw));
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechConfig {
    pub man: PhraseBank,
    pub dog: PhraseBank,
    pub r#box: PhraseBank,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            man: PhraseBank::man(),
            dog: PhraseBank::dog(),
            r#box: PhraseBank::r#box(),
        }
    }
}

impl SpeechConfig {
    pub fn to_json(&self) -> alloc::string::String {
        let mut out = alloc::string::String::new();
        let _ = write!(
            out,
            "{{\"man\":{{\"talk\":{},\"lines\":\"{}\"}},\"dog\":{{\"talk\":{},\"lines\":\"{}\"}},\"box\":{{\"talk\":{},\"lines\":\"{}\"}}}}",
            self.man.talk_pct,
            json_escape(&self.man.joined_lines()),
            self.dog.talk_pct,
            json_escape(&self.dog.joined_lines()),
            self.r#box.talk_pct,
            json_escape(&self.r#box.joined_lines()),
        );
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpeechAction {
    Save(SpeechConfig),
    Reset,
}

#[derive(Deserialize)]
struct SpeechPost<'a> {
    #[serde(default)]
    reset: Option<bool>,
    #[serde(default, borrow)]
    man: Option<FigurePost<'a>>,
    #[serde(default, borrow)]
    dog: Option<FigurePost<'a>>,
    #[serde(default, borrow, rename = "box")]
    r#box: Option<FigurePost<'a>>,
}

#[derive(Deserialize)]
struct FigurePost<'a> {
    #[serde(default)]
    talk: Option<u8>,
    #[serde(default, borrow)]
    lines: Option<&'a str>,
}

pub fn parse_speech_json(bytes: &[u8]) -> Result<SpeechAction, ParseError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    let (file, _): (SpeechPost<'_>, _) =
        serde_json_core::from_str(text).map_err(|_| ParseError::InvalidJson)?;
    if file.reset == Some(true) {
        return Ok(SpeechAction::Reset);
    }
    let mut cfg = SpeechConfig::default();
    apply_figure(&mut cfg.man, file.man);
    apply_figure(&mut cfg.dog, file.dog);
    apply_figure(&mut cfg.r#box, file.r#box);
    Ok(SpeechAction::Save(cfg))
}

fn apply_figure(bank: &mut PhraseBank, post: Option<FigurePost<'_>>) {
    let Some(post) = post else {
        return;
    };
    if let Some(talk) = post.talk {
        bank.talk_pct = talk.min(100);
    }
    if let Some(lines) = post.lines {
        bank.set_lines(expand_lines(lines));
    }
}

fn expand_lines(s: &str) -> impl Iterator<Item = &str> {
    s.split('\n').flat_map(|part| part.split("\\n"))
}

fn trim_line(s: &str) -> &str {
    let s = s.trim();
    if s.len() <= LINE_MAX {
        return s;
    }
    let mut end = LINE_MAX;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].trim()
}

fn json_escape(s: &str) -> alloc::string::String {
    let mut out = alloc::string::String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_banks_match_dialog() {
        let cfg = SpeechConfig::default();
        assert_eq!(cfg.man.talk_pct, DEFAULT_MAN_TALK);
        assert_eq!(cfg.dog.talk_pct, DEFAULT_DOG_TALK);
        assert_eq!(cfg.r#box.talk_pct, DEFAULT_BOX_TALK);
        assert_eq!(cfg.man.lines().len(), STICKMAN_LINES.len());
        assert_eq!(cfg.r#box.lines()[0].as_str(), "sigh...");
    }

    #[test]
    fn parses_custom_banks() {
        let SpeechAction::Save(cfg) = parse_speech_json(
            br#"{"man":{"talk":20,"lines":"Hi!\nYo"},"dog":{"talk":0,"lines":"Woof!"},"box":{"talk":3,"lines":"thud"}}"#,
        )
        .unwrap() else {
            panic!("save");
        };
        assert_eq!(cfg.man.talk_pct, 20);
        assert_eq!(cfg.man.lines().len(), 2);
        assert_eq!(cfg.man.lines()[1].as_str(), "Yo");
        assert_eq!(cfg.dog.talk_pct, 0);
        assert_eq!(cfg.r#box.talk_pct, 3);
    }

    #[test]
    fn parses_reset() {
        assert_eq!(
            parse_speech_json(br#"{"reset":true}"#).unwrap(),
            SpeechAction::Reset
        );
    }

    #[test]
    fn round_trip_json() {
        let mut cfg = SpeechConfig::default();
        cfg.man.talk_pct = 12;
        cfg.man.set_lines(["Ouch!", "Hey!"]);
        let json = cfg.to_json();
        let SpeechAction::Save(parsed) = parse_speech_json(json.as_bytes()).unwrap() else {
            panic!("save");
        };
        assert_eq!(parsed.man.talk_pct, 12);
        assert_eq!(parsed.man.lines()[0].as_str(), "Ouch!");
        assert_eq!(parsed.man.lines()[1].as_str(), "Hey!");
        assert_eq!(parsed.dog.talk_pct, DEFAULT_DOG_TALK);
    }
}
