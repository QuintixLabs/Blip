// ============================================================
// style.rs
// What a blip looks like and how it moves: it grows from the start
// size to the end size, slowing down as it goes, and fades out.
// ============================================================

use std::time::Duration;

use crate::app::settings::{parse_hex, Settings};

#[derive(Clone, Copy, PartialEq)]
pub enum Shape {
    Circle,
    Ring,
    Square,
    Hexagon,
    Diamond,
}

/// The settings a blip needs, ready to use.
#[derive(Clone, Copy)]
pub struct Style {
    pub shape: Shape,
    pub color: [u8; 3],
    /// In px at 100 % scaling; the screen's scaling is applied when drawing.
    pub start_size: f32,
    pub end_size: f32,
    /// 0–1.
    pub opacity: f32,
    pub duration: Duration,
    /// 0–1: how far through the blip the fade starts.
    pub fade_start: f32,
}

impl Style {
    pub fn from_settings(s: &Settings) -> Self {
        let shape = match s.shape.as_str() {
            "ring" => Shape::Ring,
            "square" => Shape::Square,
            "hexagon" => Shape::Hexagon,
            "diamond" => Shape::Diamond,
            _ => Shape::Circle,
        };
        Self {
            shape,
            color: parse_hex(&s.color).unwrap_or([0, 0, 0]),
            start_size: s.start_size as f32,
            end_size: s.end_size as f32,
            opacity: s.opacity as f32 / 100.0,
            duration: duration_for(s.speed),
            fade_start: s.fade_start as f32 / 100.0,
        }
    }

    /// The bigger of the two sizes: room a blip needs, in px at 100 %.
    pub fn largest(&self) -> f32 {
        self.start_size.max(self.end_size)
    }

    /// Size (px at 100 %) and opacity (0–1) at `elapsed`, or None once it's done.
    pub fn at(&self, elapsed: Duration) -> Option<(f32, f32)> {
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        if t >= 1.0 {
            return None;
        }
        // Fast at first, gently settling (ease-out cubic), like the original Blip.
        let eased = 1.0 - (1.0 - t).powi(3);
        let size = self.start_size + (self.end_size - self.start_size) * eased;
        let fade = if t > self.fade_start { 1.0 - (t - self.fade_start) / (1.0 - self.fade_start) } else { 1.0 };
        Some((size, self.opacity * fade.clamp(0.0, 1.0)))
    }
}

/// Speed 1–10 → how long a blip lasts. 6 (the default) is about a quarter
/// second, like the original Blip.
pub fn duration_for(speed: u32) -> Duration {
    Duration::from_millis(1500 / speed.clamp(1, 10) as u64)
}
