// ============================================================
// draw.rs
// Draws one frame of a blip with smooth edges, the same on every
// system. Each system's overlay then puts the picture on screen.
// ============================================================

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use super::style::{Shape, Style};

/// Clears `pixmap` and draws the shape centered in it, `size` px wide (real
/// screen pixels) at `opacity` 0–1. The result is premultiplied RGBA.
pub fn frame(pixmap: &mut Pixmap, style: &Style, size: f32, opacity: f32) {
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    if size < 0.5 || opacity <= 0.0 {
        return;
    }
    let cx = pixmap.width() as f32 / 2.0;
    let cy = pixmap.height() as f32 / 2.0;
    let r = size / 2.0;

    let mut paint = Paint::default();
    let [red, green, blue] = style.color;
    paint.set_color_rgba8(red, green, blue, (opacity * 255.0).round() as u8);
    paint.anti_alias = true;

    let path = match style.shape {
        Shape::Circle | Shape::Ring => PathBuilder::from_circle(cx, cy, r),
        Shape::Square => {
            // Slightly rounded corners read softer on screen than sharp ones.
            let rect = tiny_skia::Rect::from_xywh(cx - r, cy - r, size, size);
            rect.map(|rect| rounded_rect(rect, size * 0.14))
        }
        Shape::Hexagon => polygon(cx, cy, r, 6, 0.0),
        Shape::Diamond => polygon(cx, cy, r, 4, -90.0),
    };
    let Some(path) = path else { return };

    if style.shape == Shape::Ring {
        // An outline, thicker on bigger rings; drawn just inside the edge.
        let width = (size * 0.12).max(1.5);
        let stroke = Stroke { width, ..Stroke::default() };
        let inner = PathBuilder::from_circle(cx, cy, (r - width / 2.0).max(0.5));
        if let Some(inner) = inner {
            pixmap.stroke_path(&inner, &paint, &stroke, Transform::identity(), None);
        }
    } else {
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// A regular polygon with `sides` corners, the first at `start_deg`.
fn polygon(cx: f32, cy: f32, r: f32, sides: u32, start_deg: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for i in 0..sides {
        let angle = (start_deg + i as f32 * 360.0 / sides as f32).to_radians();
        let (x, y) = (cx + r * angle.cos(), cy + r * angle.sin());
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.close();
    pb.finish()
}

fn rounded_rect(rect: tiny_skia::Rect, radius: f32) -> tiny_skia::Path {
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let k = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let c = k * 0.4477; // control point distance for a circular arc (1 - 0.5523)
    let mut pb = PathBuilder::new();
    pb.move_to(l + k, t);
    pb.line_to(r - k, t);
    pb.cubic_to(r - c, t, r, t + c, r, t + k);
    pb.line_to(r, b - k);
    pb.cubic_to(r, b - c, r - c, b, r - k, b);
    pb.line_to(l + k, b);
    pb.cubic_to(l + c, b, l, b - c, l, b - k);
    pb.line_to(l, t + k);
    pb.cubic_to(l, t + c, l + c, t, l + k, t);
    pb.close();
    pb.finish().unwrap_or_else(|| PathBuilder::from_rect(rect))
}
