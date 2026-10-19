//! Geometry for the vector objects: arrow heads, callout bodies, step badges, shapes.

use linkshot_core::enums::*;
use linkshot_core::geometry::{Pt, Rect};
use tiny_skia::{Path, PathBuilder};

pub fn pt(p: Pt) -> (f32, f32) { (p.x as f32, p.y as f32) }

pub fn rect_path(r: Rect) -> Option<Path> {
    let (x, y, w, h) = (r.min.x as f32, r.min.y as f32, r.width() as f32, r.height() as f32);
    if w <= 0.0 || h <= 0.0 { return None; }
    PathBuilder::from_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?).into()
}

pub fn rounded_rect_path(r: Rect, radius: f64) -> Option<Path> {
    let rad = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0) as f32;
    if rad <= 0.0 { return rect_path(r); }
    let (l, t, rr, b) = (r.min.x as f32, r.min.y as f32, r.max.x as f32, r.max.y as f32);
    let k = 0.5522848 * rad;
    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(rr - rad, t);
    pb.cubic_to(rr - rad + k, t, rr, t + rad - k, rr, t + rad);
    pb.line_to(rr, b - rad);
    pb.cubic_to(rr, b - rad + k, rr - rad + k, b, rr - rad, b);
    pb.line_to(l + rad, b);
    pb.cubic_to(l + rad - k, b, l, b - rad + k, l, b - rad);
    pb.line_to(l, t + rad);
    pb.cubic_to(l, t + rad - k, l + rad - k, t, l + rad, t);
    pb.close();
    pb.finish()
}

pub fn ellipse_path(r: Rect) -> Option<Path> {
    let (x, y, w, h) = (r.min.x as f32, r.min.y as f32, r.width() as f32, r.height() as f32);
    if w <= 0.0 || h <= 0.0 { return None; }
    PathBuilder::from_oval(tiny_skia::Rect::from_xywh(x, y, w, h)?)
}

pub fn polygon_path(points: &[Pt], close: bool) -> Option<Path> {
    if points.len() < 2 { return None; }
    let mut pb = PathBuilder::new();
    let (x, y) = pt(points[0]);
    pb.move_to(x, y);
    for p in &points[1..] { let (x, y) = pt(*p); pb.line_to(x, y); }
    if close { pb.close(); }
    pb.finish()
}

/// Shape outline/fill path for the Shape tool.
pub fn shape_path(shape: ShapeShape, r: Rect, corner_ratio: f64, points: &[Pt]) -> Option<Path> {
    match shape {
        ShapeShape::Rectangle => rect_path(r),
        ShapeShape::RoundedRectangle => rounded_rect_path(r, (r.width().min(r.height()) / 2.0) * corner_ratio.clamp(0.0, 1.0).max(0.2)),
        ShapeShape::Ellipse => ellipse_path(r),
        ShapeShape::Polygon => polygon_path(points, true),
    }
}

/// The centre line of an arrow/line: straight or quadratic bezier through a control point.
pub fn line_path(a: Pt, b: Pt, control: Option<Pt>) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (ax, ay) = pt(a);
    pb.move_to(ax, ay);
    match control {
        Some(c) => { let (cx, cy) = pt(c); let (bx, by) = pt(b); pb.quad_to(cx, cy, bx, by); }
        None => { let (bx, by) = pt(b); pb.line_to(bx, by); }
    }
    pb.finish()
}

/// Direction unit vector at an end of the line (pointing outward from the line).
pub fn end_direction(a: Pt, b: Pt, control: Option<Pt>, at_end: bool) -> Pt {
    let (from, to) = match (control, at_end) {
        (Some(c), true) => (c, b),
        (Some(c), false) => (c, a),
        (None, true) => (a, b),
        (None, false) => (b, a),
    };
    let d = to - from;
    let len = (d.x * d.x + d.y * d.y).sqrt();
    if len < 1e-6 { Pt::new(1.0, 0.0) } else { d * (1.0 / len) }
}

/// Geometry for a line end. `tip` is the line end point, `dir` points outward, `w` the stroke width,
/// `size` the "Start/End Size" (≈3 by default). Returns (fill path, how far to shorten the line).
pub fn end_cap(kind: ArrowLineEndType, tip: Pt, dir: Pt, w: f64, size: f64) -> (Option<Path>, f64) {
    let n = Pt::new(-dir.y, dir.x);
    let s = size.max(0.5);
    let head_len = w * (0.9 + s * 0.55);
    let half = w * (0.35 + s * 0.3);
    let tri = |back: f64, half: f64, concave: f64| -> Option<Path> {
        let base = tip - dir * back;
        let p1 = base + n * half;
        let p2 = base - n * half;
        let notch = tip - dir * (back - concave);
        let mut pb = PathBuilder::new();
        let (x, y) = pt(tip); pb.move_to(x, y);
        let (x, y) = pt(p1); pb.line_to(x, y);
        if concave > 0.0 { let (x, y) = pt(notch); pb.line_to(x, y); }
        let (x, y) = pt(p2); pb.line_to(x, y);
        pb.close();
        pb.finish()
    };
    match kind {
        ArrowLineEndType::Flat => (None, 0.0),
        ArrowLineEndType::Round => (ellipse_path(Rect::from_xywh(tip.x - w / 2.0, tip.y - w / 2.0, w, w)), 0.0),
        ArrowLineEndType::RoundAnchor => { let r = w * (0.6 + s * 0.2); (ellipse_path(Rect::from_xywh(tip.x - r, tip.y - r, 2.0 * r, 2.0 * r)), 0.0) }
        ArrowLineEndType::SquareAnchor => { let r = w * (0.55 + s * 0.2); (polygon_path(&[tip + dir * r + n * r, tip + dir * r - n * r, tip - dir * r - n * r, tip - dir * r + n * r], true), 0.0) }
        ArrowLineEndType::DiamondAnchor => { let r = w * (0.7 + s * 0.25); (polygon_path(&[tip + dir * r, tip + n * r, tip - dir * r, tip - n * r], true), 0.0) }
        ArrowLineEndType::TArrow => { let r = w * (0.6 + s * 0.3); let t = w * 0.6; (polygon_path(&[tip + n * r, tip + n * r - dir * t, tip - n * r - dir * t, tip - n * r], true), t) }
        ArrowLineEndType::EquilateralArrow => { let l = w * (1.0 + s * 0.6); (tri(l, l * 0.577, 0.0), l * 0.6) }
        ArrowLineEndType::BarbedArrow => (tri(head_len * 1.15, half, head_len * 0.35), head_len * 0.6),
        ArrowLineEndType::LineArrow => {
            // Open V drawn as two strokes: we approximate with a thin filled chevron.
            let back = tip - dir * head_len;
            let t = w * 0.5;
            let p1 = back + n * half; let p2 = back - n * half;
            let i1 = p1 + dir * t * 0.9 - n * t * 0.5; let i2 = p2 + dir * t * 0.9 + n * t * 0.5;
            let inner_tip = tip - dir * t * 1.2;
            (polygon_path(&[tip, p1, i1, inner_tip, i2, p2], true), 0.0)
        }
        ArrowLineEndType::RoundedArrow => (rounded_head(tip, dir, n, head_len, half), head_len * 0.55),
        ArrowLineEndType::TaperArrow => (tri(head_len * 1.3, half * 0.9, 0.0), head_len * 0.8),
    }
}

fn rounded_head(tip: Pt, dir: Pt, n: Pt, len: f64, half: f64) -> Option<Path> {
    let base = tip - dir * len;
    let p1 = base + n * half;
    let p2 = base - n * half;
    let r = half * 0.35;
    let mut pb = PathBuilder::new();
    let (x, y) = pt(tip); pb.move_to(x, y);
    let (x, y) = pt(p1); pb.line_to(x, y);
    // Rounded back edge: a shallow curve through a point slightly toward the tip.
    let mid = base + dir * (r * 0.8);
    let (cx, cy) = pt(mid);
    let (x2, y2) = pt(p2);
    pb.quad_to(cx, cy, x2, y2);
    pb.close();
    pb.finish()
}

/// Callout body + tail as one outline path. The body is `r`; each tail ends at a point in `tails`.
pub fn callout_path(shape: CalloutShape, r: Rect, tails: &[Pt], tail_width: f64) -> Option<Path> {
    let body_is_ellipse = matches!(shape, CalloutShape::CTBalloon1 | CalloutShape::CTBalloon2 | CalloutShape::CTBalloon3 | CalloutShape::CTBalloon4
        | CalloutShape::CTBalloon5 | CalloutShape::CTBalloon6 | CalloutShape::CTBalloon7 | CalloutShape::CTBalloon8 | CalloutShape::CTBalloon9
        | CalloutShape::CTBalloon10 | CalloutShape::CTBalloon11 | CalloutShape::CTBalloon12 | CalloutShape::CTBasicSpeechBubble1
        | CalloutShape::CTBasicThoughtBubble1 | CalloutShape::CTBasicThoughtBubble2);
    let radius = match shape {
        CalloutShape::CTRoundedRectWithArrow | CalloutShape::CTBasicSpeechBubble2 => (r.width().min(r.height()) * 0.18).min(24.0),
        CalloutShape::CTBasicArrowText1 | CalloutShape::CTBasicArrowText2 => 0.0,
        _ => 0.0,
    };
    let center = r.center();
    let mut pb = PathBuilder::new();
    // Body
    let body = if body_is_ellipse { ellipse_path(r)? } else { rounded_rect_path(r, radius)? };
    pb.push_path(&body);
    // Tails: a triangle from two points on the body edge to the tail tip.
    for tail in tails {
        if r.contains(*tail) { continue; }
        let d = *tail - center;
        let len = (d.x * d.x + d.y * d.y).sqrt();
        if len < 1e-3 { continue; }
        let dir = d * (1.0 / len);
        let n = Pt::new(-dir.y, dir.x);
        // Anchor the tail base inside the body so the union is seamless.
        let base = center + dir * (len.min(r.width().min(r.height()) * 0.35));
        let hw = (tail_width.max(4.0) * 1.2).min(r.width().min(r.height()) * 0.45);
        let a = base + n * hw;
        let b = base - n * hw;
        let (x, y) = pt(a); pb.move_to(x, y);
        let (x, y) = pt(*tail); pb.line_to(x, y);
        let (x, y) = pt(b); pb.line_to(x, y);
        pb.close();
    }
    pb.finish()
}

/// Step badge shape. `r` is the badge box; pointed variants add a small tail downward-right.
pub fn step_path(style: StepStyle, r: Rect) -> Option<Path> {
    match style {
        StepStyle::Circle => ellipse_path(r),
        StepStyle::Square => rounded_rect_path(r, r.width() * 0.12),
        StepStyle::CirclePointed | StepStyle::SquarePointed => {
            let body = if style == StepStyle::CirclePointed { ellipse_path(r)? } else { rounded_rect_path(r, r.width() * 0.12)? };
            let mut pb = PathBuilder::new();
            pb.push_path(&body);
            let c = r.center();
            let rad = r.width() / 2.0;
            let tip = Pt::new(r.min.x - rad * 0.35, r.max.y + rad * 0.35);
            let a = c + Pt::new(-rad * 0.7, rad * 0.0);
            let b = c + Pt::new(0.0, rad * 0.7);
            let (x, y) = pt(a); pb.move_to(x, y);
            let (x, y) = pt(tip); pb.line_to(x, y);
            let (x, y) = pt(b); pb.line_to(x, y);
            pb.close();
            pb.finish()
        }
    }
}

pub fn dash_pattern(dash: DashType, w: f32) -> Option<Vec<f32>> {
    let w = w.max(1.0);
    match dash {
        DashType::Solid => None,
        DashType::Dash => Some(vec![w * 3.0, w * 2.0]),
        DashType::Dot => Some(vec![w, w * 1.5]),
        DashType::DashDot => Some(vec![w * 3.0, w * 1.5, w, w * 1.5]),
        DashType::DashDotDot => Some(vec![w * 3.0, w * 1.5, w, w * 1.5, w, w * 1.5]),
    }
}
