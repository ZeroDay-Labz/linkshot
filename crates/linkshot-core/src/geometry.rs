//! Points, sizes and rectangles. Points are serialised as `"x.000000,y.000000"` strings.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const ZERO: Pt = Pt { x: 0.0, y: 0.0 };
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn min(self, o: Pt) -> Pt {
        Pt::new(self.x.min(o.x), self.y.min(o.y))
    }
    pub fn max(self, o: Pt) -> Pt {
        Pt::new(self.x.max(o.x), self.y.max(o.y))
    }
    pub fn distance(self, o: Pt) -> f64 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
    pub fn lerp(self, o: Pt, t: f64) -> Pt {
        Pt::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
    pub fn parse(s: &str) -> Option<Pt> {
        let (x, y) = s.split_once(',')?;
        Some(Pt::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
    }
}

impl std::ops::Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f64> for Pt {
    type Output = Pt;
    fn mul(self, k: f64) -> Pt {
        Pt::new(self.x * k, self.y * k)
    }
}

impl fmt::Debug for Pt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.3}, {:.3})", self.x, self.y)
    }
}
impl fmt::Display for Pt {
    /// Serialised format: six decimals, comma separated.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6},{:.6}", self.x, self.y)
    }
}
impl Serialize for Pt {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Pt {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Pt::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("bad point {s:?}")))
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}
impl Size {
    pub const fn new(w: f64, h: f64) -> Self {
        Self { w, h }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub min: Pt,
    pub max: Pt,
}

impl Rect {
    pub fn from_points(a: Pt, b: Pt) -> Rect {
        Rect { min: a.min(b), max: a.max(b) }
    }
    pub fn from_xywh(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { min: Pt::new(x, y), max: Pt::new(x + w, y + h) }
    }
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    pub fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }
    pub fn center(&self) -> Pt {
        self.min.lerp(self.max, 0.5)
    }
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    pub fn union(&self, o: Rect) -> Rect {
        Rect { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn translate(&self, d: Pt) -> Rect {
        Rect { min: self.min + d, max: self.max + d }
    }
    pub fn expand(&self, k: f64) -> Rect {
        Rect { min: self.min - Pt::new(k, k), max: self.max + Pt::new(k, k) }
    }
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }
    /// Snapped to whole pixels, covering the rect.
    pub fn round_out(&self) -> Rect {
        Rect {
            min: Pt::new(self.min.x.floor(), self.min.y.floor()),
            max: Pt::new(self.max.x.ceil(), self.max.y.ceil()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_format() {
        let p = Pt::parse("81.000000,23.000000").unwrap();
        assert_eq!(p, Pt::new(81.0, 23.0));
        assert_eq!(p.to_string(), "81.000000,23.000000");
        assert_eq!(serde_json::to_string(&p).unwrap(), "\"81.000000,23.000000\"");
    }
}
