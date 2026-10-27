//! Vector icons drawn with egui shapes.

use std::f32::consts::{PI, TAU};

use egui::epaint::PathShape;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Play,
    Pause,
    Replay,
    Back10,
    Forward10,
    Prev,
    Next,
    Volume(u8),
    Muted,
    Subtitles,
    Audio,
    Pip,
    PipExit,
    Fullscreen,
    FullscreenExit,
    Close,
    Minimize,
    Maximize,
    Restore,
    Check,
    Keyboard,
}

/// Paints `icon` centered in `rect`. Icons are designed on a 24×24 grid.
pub fn paint(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let c = rect.center();
    let at = |x: f32, y: f32| pos2(c.x + (x - 12.0) * s, c.y + (y - 12.0) * s);
    let w = (1.8 * s).max(1.2);
    let stroke = Stroke::new(w, color);
    let fill = |pts: Vec<Pos2>| p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    let line = |pts: Vec<Pos2>| p.add(Shape::Path(PathShape::line(pts, stroke)));
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        let n = 24;
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };

    match icon {
        Icon::Play => {
            fill(vec![at(7.5, 4.5), at(19.5, 12.0), at(7.5, 19.5)]);
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(6.5, 4.5), at(10.0, 19.5)), 1.2 * s, color);
            p.rect_filled(Rect::from_min_max(at(14.0, 4.5), at(17.5, 19.5)), 1.2 * s, color);
        }
        Icon::Replay => {
            line(arc(12.0, 12.5, 7.0, -PI * 0.55, PI * 1.3));
            fill(vec![at(9.0, 1.8), at(13.2, 5.6), at(8.0, 7.6)]);
        }
        Icon::Back10 | Icon::Forward10 => {
            let fwd = icon == Icon::Forward10;
            let (a0, a1) = if fwd { (-PI * 0.5, PI * 1.25) } else { (-PI * 0.5, -PI * 2.25) };
            line(arc(12.0, 12.5, 8.0, a0, a1));
            let tip = at(12.0, 4.5);
            let d = if fwd { -1.0 } else { 1.0 };
            fill(vec![tip + vec2(d * 3.6 * s, -3.0 * s), tip + vec2(d * 3.6 * s, 3.0 * s), tip + vec2(-d * 1.0 * s, 0.0)]);
            p.text(at(12.0, 13.0), Align2::CENTER_CENTER, "10", FontId::proportional(7.5 * s), color);
        }
        Icon::Prev | Icon::Next => {
            let m = |x: f32| if icon == Icon::Next { x } else { 24.0 - x };
            fill(vec![at(m(6.0), 5.5), at(m(16.0), 12.0), at(m(6.0), 18.5)]);
            let x0 = m(16.5).min(m(19.0));
            p.rect_filled(Rect::from_min_max(at(x0, 5.5), at(x0 + 2.5, 18.5)), 0.8 * s, color);
        }
        Icon::Volume(_) | Icon::Muted => {
            // The speaker body isn't convex, so draw it as a box plus a cone.
            p.rect_filled(Rect::from_min_max(at(3.5, 9.0), at(8.0, 15.0)), 0.8 * s, color);
            fill(vec![at(7.0, 9.0), at(12.5, 4.5), at(12.5, 19.5), at(7.0, 15.0)]);
            match icon {
                Icon::Muted => {
                    line(vec![at(16.0, 9.0), at(22.0, 15.0)]);
                    line(vec![at(22.0, 9.0), at(16.0, 15.0)]);
                }
                Icon::Volume(level) => {
                    if level >= 1 {
                        line(arc(12.5, 12.0, 4.0, -PI * 0.3, PI * 0.3));
                    }
                    if level >= 2 {
                        line(arc(12.5, 12.0, 7.5, -PI * 0.32, PI * 0.32));
                    }
                }
                _ => {}
            }
        }
        Icon::Subtitles => {
            p.rect_stroke(Rect::from_min_max(at(2.5, 5.0), at(21.5, 19.0)), 2.5 * s, stroke, StrokeKind::Middle);
            line(vec![at(6.5, 12.5), at(11.0, 12.5)]);
            line(vec![at(13.5, 12.5), at(17.5, 12.5)]);
            line(vec![at(6.5, 15.5), at(9.0, 15.5)]);
            line(vec![at(11.5, 15.5), at(17.5, 15.5)]);
        }
        Icon::Audio => {
            for (x, h) in [(5.0, 5.0), (9.0, 12.0), (13.0, 16.0), (17.0, 9.0), (21.0, 4.0)] {
                line(vec![at(x - 1.5, 12.0 - h / 2.0), at(x - 1.5, 12.0 + h / 2.0)]);
            }
        }
        Icon::Pip | Icon::PipExit => {
            p.rect_stroke(Rect::from_min_max(at(2.5, 4.5), at(21.5, 19.5)), 2.5 * s, stroke, StrokeKind::Middle);
            let inner = if icon == Icon::Pip {
                Rect::from_min_max(at(12.0, 11.5), at(19.0, 17.0))
            } else {
                Rect::from_min_max(at(5.0, 7.0), at(12.0, 12.5))
            };
            p.rect_filled(inner, 1.2 * s, color);
        }
        Icon::Fullscreen | Icon::FullscreenExit => {
            let out = icon == Icon::Fullscreen;
            for (cx, cy) in [(4.0, 4.0), (20.0, 4.0), (4.0, 20.0), (20.0, 20.0)] {
                let dx = if cx < 12.0 { 1.0 } else { -1.0 };
                let dy = if cy < 12.0 { 1.0 } else { -1.0 };
                let (cx, cy) = if out { (cx, cy) } else { (cx + dx * 5.0, cy + dy * 5.0) };
                let sign = if out { 1.0 } else { -1.0 };
                line(vec![at(cx + dx * 5.0 * sign, cy), at(cx, cy), at(cx, cy + dy * 5.0 * sign)]);
            }
        }
        Icon::Close => {
            let st = Stroke::new(w * 0.7, color);
            p.line_segment([at(7.0, 7.0), at(17.0, 17.0)], st);
            p.line_segment([at(17.0, 7.0), at(7.0, 17.0)], st);
        }
        Icon::Minimize => {
            p.line_segment([at(7.0, 12.0), at(17.0, 12.0)], Stroke::new(w * 0.7, color));
        }
        Icon::Maximize => {
            let st = Stroke::new(w * 0.7, color);
            p.rect_stroke(Rect::from_min_max(at(7.0, 7.0), at(17.0, 17.0)), 1.5 * s, st, StrokeKind::Middle);
        }
        Icon::Restore => {
            let st = Stroke::new(w * 0.7, color);
            p.rect_stroke(Rect::from_min_max(at(7.0, 9.5), at(14.5, 17.0)), 1.2 * s, st, StrokeKind::Middle);
            p.add(Shape::Path(PathShape::line(vec![at(9.5, 7.0), at(17.0, 7.0), at(17.0, 14.5)], st)));
        }
        Icon::Check => {
            line(vec![at(5.0, 12.5), at(10.0, 17.5), at(19.5, 7.0)]);
        }
        Icon::Keyboard => {
            p.rect_stroke(Rect::from_min_max(at(2.5, 6.0), at(21.5, 18.0)), 2.0 * s, stroke, StrokeKind::Middle);
            for x in [6.5, 10.0, 13.5, 17.0] {
                p.circle_filled(at(x, 10.0), 0.9 * s, color);
            }
            line(vec![at(7.5, 14.5), at(16.5, 14.5)]);
        }
    }
}

/// An indeterminate spinner arc, animated by `time`.
pub fn spinner(p: &Painter, center: Pos2, radius: f32, time: f64, color: Color32) {
    let start = (time * 4.0) as f32 % TAU;
    let sweep = 1.2 + 0.8 * ((time * 2.0).sin() as f32);
    let pts: Vec<Pos2> = (0..=32)
        .map(|i| {
            let a = start + sweep * 2.0 * i as f32 / 32.0;
            center + Vec2::angled(a) * radius
        })
        .collect();
    p.add(Shape::Path(PathShape::line(pts, Stroke::new(radius * 0.16, color))));
}
