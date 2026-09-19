//! Shared live-config types (device website + desktop tests).
//!
//! USB `wifi.json` and `POST /api/config` both parse through this module.

use core::fmt::{self, Write as _};
use embedded_graphics::pixelcolor::Rgb565;
use heapless::String;
use serde::Deserialize;

/// Max Wi-Fi SSID length (802.11).
pub const SSID_MAX: usize = 32;
/// Max WPA2 passphrase length.
pub const PASS_MAX: usize = 64;
/// SoftAP SSID when station join fails or credentials are missing.
pub const SOFTAP_SSID: &str = "stickman-setup";
/// SoftAP IPv4 (also the page URL on fallback).
pub const SOFTAP_IP: &str = "192.168.4.1";

/// Station credentials from USB `wifi.json` or flash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WifiCreds {
    pub ssid: String<SSID_MAX>,
    pub password: String<PASS_MAX>,
}

impl WifiCreds {
    pub fn new(ssid: &str, password: &str) -> Option<Self> {
        let mut s = String::new();
        let mut p = String::new();
        s.push_str(ssid).ok()?;
        p.push_str(password).ok()?;
        if s.is_empty() {
            return None;
        }
        Some(Self {
            ssid: s,
            password: p,
        })
    }
}

#[derive(Deserialize)]
struct WifiFile<'a> {
    ssid: &'a str,
    #[serde(default)]
    password: &'a str,
}

/// Parse the USB `wifi.json` body. 2.4 GHz only — 5 GHz APs will not join.
pub fn parse_wifi_json(bytes: &[u8]) -> Result<WifiCreds, ParseError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;
    let text = text.trim_start_matches(['\u{feff}', '\0']).trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    let (file, _): (WifiFile<'_>, _) =
        serde_json_core::from_str(text).map_err(|_| ParseError::InvalidJson)?;
    WifiCreds::new(file.ssid.trim(), file.password).ok_or(ParseError::MissingSsid)
}

/// Command from the website to the game core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigCmd {
    /// Solid home-room backdrop (RGB888, converted to RGB565).
    SetBackdropColor { r: u8, g: u8, b: u8 },
    /// Drop the color override and restore the embedded/imported image.
    ClearBackdropColor,
}

impl ConfigCmd {
    pub fn to_rgb565(self) -> Option<Rgb565> {
        match self {
            Self::SetBackdropColor { r, g, b } => Some(rgb888_to_565(r, g, b)),
            Self::ClearBackdropColor => None,
        }
    }
}

/// Website / serial status snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetStatus {
    pub mode: NetMode,
    pub ssid: String<SSID_MAX>,
    pub ip: String<16>,
    pub last_error: String<80>,
}

impl Default for NetStatus {
    fn default() -> Self {
        Self {
            mode: NetMode::Off,
            ssid: String::new(),
            ip: String::new(),
            last_error: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetMode {
    Off,
    Station,
    SoftAp,
}

impl NetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Station => "station",
            Self::SoftAp => "softap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    InvalidUtf8,
    InvalidJson,
    MissingSsid,
    BadColor,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty body",
            Self::InvalidUtf8 => "not utf-8",
            Self::InvalidJson => "invalid json",
            Self::MissingSsid => "missing ssid",
            Self::BadColor => "bad color",
        })
    }
}

#[derive(Deserialize)]
struct ConfigFile<'a> {
    #[serde(default)]
    backdrop: Option<&'a str>,
    #[serde(default)]
    r: Option<u8>,
    #[serde(default)]
    g: Option<u8>,
    #[serde(default)]
    b: Option<u8>,
    #[serde(default)]
    clear: Option<bool>,
}

/// Parse `POST /api/config` JSON.
pub fn parse_config_json(bytes: &[u8]) -> Result<ConfigCmd, ParseError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    let (file, _): (ConfigFile, _) =
        serde_json_core::from_str(text).map_err(|_| ParseError::InvalidJson)?;
    if file.clear == Some(true) {
        return Ok(ConfigCmd::ClearBackdropColor);
    }
    if let Some(hex) = file.backdrop {
        return parse_hex_color(hex);
    }
    match (file.r, file.g, file.b) {
        (Some(r), Some(g), Some(b)) => Ok(ConfigCmd::SetBackdropColor { r, g, b }),
        _ => Err(ParseError::BadColor),
    }
}

fn parse_hex_color(s: &str) -> Result<ConfigCmd, ParseError> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return Err(ParseError::BadColor);
    }
    let n = u32::from_str_radix(s, 16).map_err(|_| ParseError::BadColor)?;
    Ok(ConfigCmd::SetBackdropColor {
        r: ((n >> 16) & 0xff) as u8,
        g: ((n >> 8) & 0xff) as u8,
        b: (n & 0xff) as u8,
    })
}

pub fn rgb888_to_565(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

/// Format `r.g.b.a` into `out`. Returns false if it does not fit.
pub fn write_ipv4(out: &mut String<16>, oct: [u8; 4]) -> bool {
    out.clear();
    write!(out, "{}.{}.{}.{}", oct[0], oct[1], oct[2], oct[3]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wifi_json() {
        let creds = parse_wifi_json(br#"{"ssid":"HomeNet","password":"secret"}"#).unwrap();
        assert_eq!(creds.ssid.as_str(), "HomeNet");
        assert_eq!(creds.password.as_str(), "secret");
    }

    #[test]
    fn parses_open_network() {
        let creds = parse_wifi_json(br#"{"ssid":"Open"}"#).unwrap();
        assert_eq!(creds.ssid.as_str(), "Open");
        assert!(creds.password.is_empty());
    }

    #[test]
    fn rejects_empty_ssid() {
        assert!(parse_wifi_json(br#"{"ssid":""}"#).is_err());
    }

    #[test]
    fn parses_hex_backdrop() {
        let cmd = parse_config_json(br##"{"backdrop":"#ff8000"}"##).unwrap();
        assert_eq!(
            cmd,
            ConfigCmd::SetBackdropColor {
                r: 255,
                g: 128,
                b: 0
            }
        );
    }

    #[test]
    fn parses_rgb_backdrop() {
        let cmd = parse_config_json(br#"{"r":1,"g":2,"b":3}"#).unwrap();
        assert_eq!(cmd, ConfigCmd::SetBackdropColor { r: 1, g: 2, b: 3 });
    }

    #[test]
    fn parses_clear() {
        assert_eq!(
            parse_config_json(br#"{"clear":true}"#).unwrap(),
            ConfigCmd::ClearBackdropColor
        );
    }
}
