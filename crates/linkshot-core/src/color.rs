//! `#AARRGGBB` colours as used throughout the JSON formats.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Argb {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Argb {
    pub const TRANSPARENT: Argb = Argb { a: 0, r: 0, g: 0, b: 0 };
    pub const BLACK: Argb = Argb::rgb(0, 0, 0);
    pub const WHITE: Argb = Argb::rgb(255, 255, 255);
    /// The "selection orange" used for the capture crosshair.
    pub const SELECTION_ORANGE: Argb = Argb::rgb(0xFB, 0xB9, 0x01);
    /// LinkShot product blue.
    pub const LINKSHOT_BLUE: Argb = Argb::rgb(0x2C, 0x7C, 0xB2);

    pub const fn new(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self { a, r, g, b }
    }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { a: 255, r, g, b }
    }
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
    pub fn is_transparent(self) -> bool {
        self.a == 0
    }
    pub fn to_u32(self) -> u32 {
        (self.a as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
    pub fn from_u32(v: u32) -> Self {
        Self::new((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
    /// Parses `#AARRGGBB`, `#RRGGBB`, `AARRGGBB` or `RRGGBB`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        match s.len() {
            8 => u32::from_str_radix(s, 16).ok().map(Self::from_u32),
            6 => u32::from_str_radix(s, 16).ok().map(|v| Self::from_u32(v | 0xFF00_0000)),
            _ => None,
        }
    }
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
    }
    pub fn to_rgb_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
    pub fn to_f32(self) -> [f32; 4] {
        [self.r as f32 / 255.0, self.g as f32 / 255.0, self.b as f32 / 255.0, self.a as f32 / 255.0]
    }
    /// Relative luminance (0..1) used for picking contrasting text colours.
    pub fn luminance(self) -> f32 {
        let [r, g, b, _] = self.to_f32();
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}

impl fmt::Debug for Argb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
impl fmt::Display for Argb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Argb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}
impl<'de> Deserialize<'de> for Argb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Argb::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("bad colour {s:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let c = Argb::parse("#FFFC4242").unwrap();
        assert_eq!(c, Argb::rgb(0xFC, 0x42, 0x42));
        assert_eq!(c.to_hex(), "#FFFC4242");
        assert_eq!(Argb::parse("#00000000").unwrap(), Argb::TRANSPARENT);
        assert_eq!(Argb::parse("#FC4242").unwrap().a, 255);
    }
}
