//! Our own vector glyphs for tools and buttons, drawn with the egui painter (no bitmap assets).

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};
use linkshot_core::enums::ToolMode;

fn stroke(c: Color32, w: f32) -> Stroke { Stroke::new(w, c) }

/// Draws the glyph for a tool centred in `rect` (square recommended).
pub fn tool_icon(p: &Painter, rect: Rect, tool: ToolMode, color: Color32) {
    let r = rect.shrink(rect.width() * 0.15);
    let c = r.center();
    let s = r.width();
    let w = (s / 10.0).max(1.5);
    match tool {
        ToolMode::Arrow => {
            let a = Pos2::new(r.left(), r.bottom());
            let b = Pos2::new(r.right(), r.top());
            p.line_segment([a, b], stroke(color, w * 1.2));
            let d = (b - a).normalized();
            let n = Vec2::new(-d.y, d.x);
            let head = s * 0.38;
            let base = b - d * head;
            let _ = p.add(egui::Shape::convex_polygon(vec![b, base + n * head * 0.45, base - n * head * 0.45], color, Stroke::NONE));
        }
        ToolMode::Line => { p.line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.top())], stroke(color, w * 1.2)); }
        ToolMode::Text => {
            let _ = p.text(c, egui::Align2::CENTER_CENTER, "T", egui::FontId::proportional(s * 1.05), color);
        }
        ToolMode::Callouts => {
            let body = Rect::from_min_max(Pos2::new(r.left(), r.top()), Pos2::new(r.right(), r.top() + s * 0.68));
            p.rect_stroke(body, s * 0.12, stroke(color, w), egui::StrokeKind::Inside);
            let _ = p.add(egui::Shape::convex_polygon(vec![Pos2::new(r.left() + s * 0.25, body.bottom() - 1.0), Pos2::new(r.left() + s * 0.5, body.bottom() - 1.0), Pos2::new(r.left() + s * 0.2, r.bottom())], color, Stroke::NONE));
        }
        ToolMode::Shape => { p.rect_stroke(r.shrink(w), 0.0, stroke(color, w), egui::StrokeKind::Inside); }
        ToolMode::Stamp => {
            // five-point star
            let pts: Vec<Pos2> = (0..10).map(|i| { let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0; let rr = if i % 2 == 0 { s * 0.5 } else { s * 0.22 }; c + Vec2::angled(a) * rr }).collect();
            let _ = p.add(egui::Shape::closed_line(pts, stroke(color, w)));
        }
        ToolMode::Fill => {
            let bucket = Rect::from_center_size(c + Vec2::new(-s * 0.05, s * 0.05), Vec2::splat(s * 0.6));
            p.rect_stroke(bucket.rotate_bb(egui::emath::Rot2::from_angle(0.6)), 0.0, stroke(color, w), egui::StrokeKind::Inside);
            p.circle_filled(Pos2::new(r.right() - s * 0.12, r.bottom() - s * 0.15), s * 0.11, color);
        }
        ToolMode::VectorSelection => {
            // mouse pointer
            let a = Pos2::new(r.left() + s * 0.25, r.top());
            let pts = vec![a, Pos2::new(a.x, a.y + s * 0.8), Pos2::new(a.x + s * 0.22, a.y + s * 0.6), Pos2::new(a.x + s * 0.55, a.y + s * 0.6)];
            let _ = p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        ToolMode::Selection => {
            let rr = r.shrink(w);
            let d = s * 0.18;
            for (a, b) in [(rr.left_top(), rr.right_top()), (rr.right_top(), rr.right_bottom()), (rr.right_bottom(), rr.left_bottom()), (rr.left_bottom(), rr.left_top())] {
                let dir = (b - a).normalized();
                let len = (b - a).length();
                let mut t = 0.0;
                while t < len { p.line_segment([a + dir * t, a + dir * (t + d * 0.6).min(len)], stroke(color, w)); t += d; }
            }
        }
        ToolMode::Blur => {
            for (i, y) in [0.2, 0.5, 0.8].iter().enumerate() {
                for x in [0.2, 0.5, 0.8] { p.rect_filled(Rect::from_center_size(Pos2::new(r.left() + s * x, r.top() + s * y), Vec2::splat(s * 0.22)), 0.0, color.gamma_multiply(if (i + (x * 10.0) as usize) % 2 == 0 { 1.0 } else { 0.5 })); }
            }
        }
        ToolMode::SimplifiedUI => {
            p.rect_filled(Rect::from_min_size(r.left_top(), Vec2::new(s, s * 0.22)), 2.0, color);
            p.rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.top() + s * 0.39), Vec2::new(s * 0.7, s * 0.22)), 2.0, color.gamma_multiply(0.6));
            p.rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.top() + s * 0.78), Vec2::new(s * 0.85, s * 0.22)), 2.0, color.gamma_multiply(0.6));
        }
        ToolMode::MagicWand => {
            p.line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right() - s * 0.25, r.top() + s * 0.25)], stroke(color, w * 1.3));
            p.circle_filled(Pos2::new(r.right() - s * 0.15, r.top() + s * 0.15), s * 0.12, color);
        }
        ToolMode::Crop => {
            p.line_segment([Pos2::new(r.left() + s * 0.25, r.top()), Pos2::new(r.left() + s * 0.25, r.bottom() - s * 0.25)], stroke(color, w));
            p.line_segment([Pos2::new(r.left() + s * 0.25, r.bottom() - s * 0.25), Pos2::new(r.right(), r.bottom() - s * 0.25)], stroke(color, w));
            p.line_segment([Pos2::new(r.left(), r.top() + s * 0.25), Pos2::new(r.right() - s * 0.25, r.top() + s * 0.25)], stroke(color, w));
            p.line_segment([Pos2::new(r.right() - s * 0.25, r.top() + s * 0.25), Pos2::new(r.right() - s * 0.25, r.bottom())], stroke(color, w));
        }
        ToolMode::CutOut => {
            p.rect_stroke(Rect::from_min_max(r.left_top(), Pos2::new(r.right(), r.top() + s * 0.35)), 0.0, stroke(color, w), egui::StrokeKind::Inside);
            p.rect_stroke(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - s * 0.35), r.right_bottom()), 0.0, stroke(color, w), egui::StrokeKind::Inside);
        }
        ToolMode::Pen => {
            p.line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right() - s * 0.15, r.top() + s * 0.15)], stroke(color, w * 2.0));
            p.circle_filled(Pos2::new(r.left() + s * 0.1, r.bottom() - s * 0.1), w, color);
        }
        ToolMode::Highlight => {
            p.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top() + s * 0.3), Pos2::new(r.right(), r.top() + s * 0.7)), 0.0, color.gamma_multiply(0.45));
            p.rect_stroke(Rect::from_min_max(Pos2::new(r.left(), r.top() + s * 0.3), Pos2::new(r.right(), r.top() + s * 0.7)), 0.0, stroke(color, w * 0.7), egui::StrokeKind::Inside);
        }
        ToolMode::Step => {
            p.circle_stroke(c, s * 0.48, stroke(color, w));
            p.text(c, egui::Align2::CENTER_CENTER, "1", egui::FontId::proportional(s * 0.6), color);
        }
        ToolMode::Eraser => {
            let rr = Rect::from_center_size(c, Vec2::new(s * 0.9, s * 0.5));
            p.rect_stroke(rr.rotate_bb(egui::emath::Rot2::from_angle(-0.6)), 2.0, stroke(color, w), egui::StrokeKind::Inside);
        }
        ToolMode::Magnify => {
            p.circle_stroke(c + Vec2::new(-s * 0.08, -s * 0.08), s * 0.32, stroke(color, w));
            p.line_segment([c + Vec2::new(s * 0.15, s * 0.15), Pos2::new(r.right(), r.bottom())], stroke(color, w * 1.5));
        }
        ToolMode::Favorites => {
            let pts: Vec<Pos2> = (0..10).map(|i| { let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0; let rr = if i % 2 == 0 { s * 0.5 } else { s * 0.22 }; c + Vec2::angled(a) * rr }).collect();
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        _ => {}
    }
}

/// Undo / redo curved arrows.
pub fn undo_icon(p: &Painter, rect: Rect, color: Color32, redo: bool) {
    let r = rect.shrink(rect.width() * 0.2);
    let c = r.center();
    let rad = r.width() * 0.4;
    let w = (r.width() / 9.0).max(1.5);
    let pts: Vec<Pos2> = (0..20).map(|i| { let t = i as f32 / 19.0; let a = if redo { -std::f32::consts::PI * 1.1 + t * std::f32::consts::PI * 1.4 } else { -std::f32::consts::PI * 1.3 - t * std::f32::consts::PI * 1.4 }; c + Vec2::angled(a) * rad }).collect();
    let tip = *pts.last().unwrap();
    p.add(egui::Shape::line(pts, stroke(color, w)));
    let dir = Vec2::new(if redo { -1.0 } else { 1.0 }, 0.3).normalized();
    let n = Vec2::new(-dir.y, dir.x);
    let h = rad * 0.6;
    p.add(egui::Shape::convex_polygon(vec![tip + dir * h * 0.6, tip - dir * h * 0.4 + n * h * 0.5, tip - dir * h * 0.4 - n * h * 0.5], color, Stroke::NONE));
}

pub fn camera_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.12);
    let body = Rect::from_min_max(Pos2::new(r.left(), r.top() + r.height() * 0.25), r.right_bottom());
    p.rect_stroke(body, 2.0, stroke(color, 1.6), egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_size(Pos2::new(r.left() + r.width() * 0.3, r.top()), Vec2::new(r.width() * 0.4, r.height() * 0.28)), 1.0, color);
    p.circle_stroke(body.center(), r.width() * 0.2, stroke(color, 1.6));
}

pub fn video_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.15);
    let body = Rect::from_min_max(r.left_top(), Pos2::new(r.left() + r.width() * 0.65, r.bottom()));
    p.rect_filled(body, 2.0, color);
    p.add(egui::Shape::convex_polygon(vec![Pos2::new(body.right(), r.center().y - r.height() * 0.2), Pos2::new(r.right(), r.top()), Pos2::new(r.right(), r.bottom()), Pos2::new(body.right(), r.center().y + r.height() * 0.2)], color, Stroke::NONE));
}

pub fn all_in_one_icon(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() / 2.0;
    p.circle_stroke(c, r * 0.8, stroke(color, 1.6));
    p.circle_filled(c, r * 0.45, color);
}

pub fn scrolling_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.15);
    p.rect_stroke(Rect::from_min_max(r.left_top(), Pos2::new(r.right() - r.width() * 0.3, r.bottom())), 2.0, stroke(color, 1.6), egui::StrokeKind::Inside);
    let x = r.right() - r.width() * 0.12;
    p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], stroke(color, 1.6));
    p.add(egui::Shape::convex_polygon(vec![Pos2::new(x, r.bottom()), Pos2::new(x - 4.0, r.bottom() - 5.0), Pos2::new(x + 4.0, r.bottom() - 5.0)], color, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![Pos2::new(x, r.top()), Pos2::new(x - 4.0, r.top() + 5.0), Pos2::new(x + 4.0, r.top() + 5.0)], color, Stroke::NONE));
}

pub fn close_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.3);
    p.line_segment([r.left_top(), r.right_bottom()], stroke(color, 1.5));
    p.line_segment([r.right_top(), r.left_bottom()], stroke(color, 1.5));
}
pub fn minimize_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.3);
    p.line_segment([Pos2::new(r.left(), r.center().y), Pos2::new(r.right(), r.center().y)], stroke(color, 1.5));
}
pub fn maximize_icon(p: &Painter, rect: Rect, color: Color32) {
    p.rect_stroke(rect.shrink(rect.width() * 0.3), 0.0, stroke(color, 1.2), egui::StrokeKind::Inside);
}
pub fn gear_icon(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.32;
    p.circle_stroke(c, r, stroke(color, 1.6));
    for i in 0..8 { let a = i as f32 * std::f32::consts::FRAC_PI_4; p.line_segment([c + Vec2::angled(a) * r, c + Vec2::angled(a) * (r + rect.width() * 0.14)], stroke(color, 1.8)); }
}
pub fn chevron_down(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.25;
    p.line_segment([Pos2::new(c.x - s, c.y - s * 0.5), Pos2::new(c.x, c.y + s * 0.5)], stroke(color, 1.6));
    p.line_segment([Pos2::new(c.x, c.y + s * 0.5), Pos2::new(c.x + s, c.y - s * 0.5)], stroke(color, 1.6));
}
pub fn chevron_right(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.25;
    p.line_segment([Pos2::new(c.x - s * 0.5, c.y - s), Pos2::new(c.x + s * 0.5, c.y)], stroke(color, 1.6));
    p.line_segment([Pos2::new(c.x + s * 0.5, c.y), Pos2::new(c.x - s * 0.5, c.y + s)], stroke(color, 1.6));
}
pub fn pin_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.2);
    p.rect_filled(Rect::from_min_size(Pos2::new(r.center().x - r.width() * 0.25, r.top()), Vec2::new(r.width() * 0.5, r.height() * 0.45)), 1.0, color);
    p.line_segment([Pos2::new(r.center().x, r.top() + r.height() * 0.45), Pos2::new(r.center().x, r.bottom())], stroke(color, 1.6));
}
pub fn reset_icon(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let rad = rect.width() * 0.3;
    let pts: Vec<Pos2> = (0..24).map(|i| { let a = 0.6 + i as f32 / 23.0 * 5.2; c + Vec2::angled(a) * rad }).collect();
    p.add(egui::Shape::line(pts, stroke(color, 1.6)));
    let tip = c + Vec2::angled(0.6) * rad;
    p.add(egui::Shape::convex_polygon(vec![tip + Vec2::new(0.0, -4.0), tip + Vec2::new(-4.0, 3.0), tip + Vec2::new(4.0, 3.0)], color, Stroke::NONE));
}
pub fn library_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.15);
    for i in 0..3 { let x = r.left() + i as f32 * r.width() * 0.3; p.rect_filled(Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(r.width() * 0.2, r.height())), 1.0, color); }
    p.rect_filled(Rect::from_min_size(Pos2::new(r.left() + r.width() * 0.85, r.top() + r.height() * 0.1), Vec2::new(r.width() * 0.18, r.height() * 0.9)).rotate_bb(egui::emath::Rot2::from_angle(0.2)), 1.0, color);
}
pub fn plus_icon(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.3;
    p.line_segment([Pos2::new(c.x - s, c.y), Pos2::new(c.x + s, c.y)], stroke(color, 2.0));
    p.line_segment([Pos2::new(c.x, c.y - s), Pos2::new(c.x, c.y + s)], stroke(color, 2.0));
}
pub fn redo_selection_icon(p: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.2);
    p.rect_stroke(r, 0.0, Stroke::new(1.5_f32, color), egui::StrokeKind::Inside);
    for pt in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] { p.rect_filled(Rect::from_center_size(pt, Vec2::splat(4.0)), 0.0, color); }
}
