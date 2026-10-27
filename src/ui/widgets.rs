//! Custom-painted controls.

use egui::epaint::{Mesh, RectShape, Shadow};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use super::icons::{self, Icon};
use super::theme;
use crate::player::format_time;

/// Fills `rect` with black fading vertically from `top` to `bottom` alpha.
pub fn vertical_gradient(p: &Painter, rect: Rect, top: f32, bottom: f32) {
    // A few stops with an ease curve looks much smoother than a linear ramp.
    const STEPS: usize = 8;
    let mut mesh = Mesh::default();
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32;
        let eased = t * t * (3.0 - 2.0 * t);
        let a = top + (bottom - top) * eased;
        let color = Color32::from_black_alpha((a.clamp(0.0, 1.0) * 255.0) as u8);
        let y = rect.top() + rect.height() * t;
        mesh.colored_vertex(pos2(rect.left(), y), color);
        mesh.colored_vertex(pos2(rect.right(), y), color);
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// Round, borderless icon button with a soft hover halo.
pub fn icon_button(ui: &mut Ui, rect: Rect, icon: Icon, id: Id, tooltip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.12);
    let p = ui.painter();
    if hover > 0.0 {
        p.circle_filled(rect.center(), rect.height() * 0.5, Color32::from_white_alpha((28.0 * hover) as u8));
    }
    let size = rect.height() * 0.58;
    let icon_rect = Rect::from_center_size(rect.center(), vec2(size, size));
    let pressed = resp.is_pointer_button_down_on();
    let icon_rect = if pressed { icon_rect.shrink(1.0) } else { icon_rect };
    icons::paint(p, icon, icon_rect, theme::TEXT);
    if !tooltip.is_empty() {
        resp.on_hover_text_at_pointer(tooltip)
    } else {
        resp
    }
}

/// Windows-style caption button (minimize / maximize / close).
pub fn caption_button(ui: &mut Ui, rect: Rect, icon: Icon, id: Id, danger: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.1);
    let p = ui.painter();
    if hover > 0.0 {
        let bg = if danger { theme::CLOSE_RED } else { Color32::from_white_alpha(26) };
        p.rect_filled(rect, 0.0, bg.gamma_multiply(hover));
    }
    icons::paint(p, icon, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), theme::TEXT);
    resp
}

/// Pill-shaped text label, used for timestamps above the seek bar and OSD.
pub fn chip(p: &Painter, anchor: Pos2, align: Align2, text: &str, font: FontId, alpha: f32) -> Rect {
    let galley = p.layout_no_wrap(text.to_string(), font, theme::TEXT.gamma_multiply(alpha));
    let size = galley.size() + vec2(20.0, 10.0);
    let rect = align.anchor_size(anchor, size);
    p.add(Shadow { offset: [0, 2], blur: 12, spread: 0, color: Color32::from_black_alpha((90.0 * alpha) as u8) }.as_shape(rect, CornerRadius::same(255)));
    p.add(RectShape::filled(rect, CornerRadius::same(255), theme::PANEL.gamma_multiply(alpha)));
    p.galley(rect.center() - galley.size() / 2.0, galley, theme::TEXT);
    rect
}

pub enum SeekEvent {
    None,
    /// Scrubbing in progress (cheap keyframe seek).
    Scrub(f64),
    /// Released; seek exactly here.
    Commit(f64),
}

pub struct SeekBar<'a> {
    pub position: f64,
    pub duration: f64,
    pub buffered: f64,
    pub chapters: &'a [f64],
}

impl SeekBar<'_> {
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> SeekEvent {
        let resp = ui.interact(rect, id, Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        let active = resp.hovered() || resp.dragged();
        let grow = ctx.animate_bool_with_time(id.with("grow"), active, 0.12);
        let dur = self.duration.max(0.001);

        let track_h = 4.0 + 2.0 * grow;
        let track = Rect::from_center_size(rect.center(), vec2(rect.width(), track_h));
        let frac_at = |x: f32| ((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
        let x_at = |f: f64| track.left() + track.width() * (f / dur).clamp(0.0, 1.0) as f32;

        // While dragging, the knob follows the pointer instead of the player.
        let drag_pos = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.drag_stopped() || resp.clicked());
        let shown_pos = drag_pos.map(|p| frac_at(p.x) * dur).unwrap_or(self.position);

        let p = ui.painter();
        let r = CornerRadius::same(255);
        p.rect_filled(track, r, Color32::from_white_alpha(56));
        if self.buffered > self.position {
            let buf = Rect::from_min_max(track.min, pos2(x_at(self.buffered), track.max.y));
            p.rect_filled(buf, r, Color32::from_white_alpha(70));
        }
        if let Some(hover) = resp.hover_pos() {
            let hx = hover.x.clamp(track.left(), track.right());
            p.rect_filled(Rect::from_min_max(track.min, pos2(hx, track.max.y)), r, Color32::from_white_alpha(40));
        }
        let px = x_at(shown_pos);
        p.rect_filled(Rect::from_min_max(track.min, pos2(px, track.max.y)), r, theme::ACCENT);

        for &c in self.chapters {
            if c > 0.5 {
                let x = x_at(c);
                p.rect_filled(Rect::from_center_size(pos2(x, track.center().y), vec2(2.0, track_h + 0.5)), 0.0, Color32::from_black_alpha(160));
            }
        }

        let knob_r = 7.0 * grow;
        if knob_r > 0.5 {
            p.circle_filled(pos2(px, track.center().y), knob_r + 1.0, Color32::from_black_alpha(60));
            p.circle_filled(pos2(px, track.center().y), knob_r, Color32::WHITE);
        }

        if let Some(hover) = resp.hover_pos().or(drag_pos) {
            let t = frac_at(hover.x) * dur;
            let x = hover.x.clamp(rect.left() + 30.0, rect.right() - 30.0);
            chip(ui.painter(), pos2(x, rect.top() - 6.0), Align2::CENTER_BOTTOM, &format_time(t), FontId::proportional(13.0), 1.0);
        }
        if active {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }

        match drag_pos {
            Some(p) if resp.drag_stopped() || resp.clicked() => SeekEvent::Commit(frac_at(p.x) * dur),
            Some(p) if resp.dragged() => SeekEvent::Scrub(frac_at(p.x) * dur),
            _ => SeekEvent::None,
        }
    }
}

/// Horizontal slider over 0..=`max`. Returns the new value when changed.
pub fn slider(ui: &mut Ui, rect: Rect, id: Id, value: f64, max: f64) -> Option<f64> {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let grow = ui.ctx().animate_bool_with_time(id.with("grow"), resp.hovered() || resp.dragged(), 0.12);
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 10.0, 4.0));
    let frac = (value / max).clamp(0.0, 1.0) as f32;
    let p = ui.painter();
    let r = CornerRadius::same(255);
    p.rect_filled(track, r, Color32::from_white_alpha(56));
    let x = track.left() + track.width() * frac;
    p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), r, Color32::WHITE);
    let knob = 3.0 + 3.5 * grow;
    p.circle_filled(pos2(x, track.center().y), knob, Color32::WHITE);

    let mut out = None;
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let f = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
        out = Some((f * max).round());
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            out = Some((value + scroll.signum() as f64 * 5.0).clamp(0.0, max));
        }
    }
    out
}

/// Plain clickable text, e.g. the "1.25×" speed button.
pub fn text_button(ui: &mut Ui, rect: Rect, text: &str, id: Id) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.12);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(255), Color32::from_white_alpha((28.0 * hover) as u8));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::new(13.5, theme::semibold()), theme::TEXT);
    resp
}

/// A row inside a popover menu.
pub fn menu_row(ui: &mut Ui, label: &str, detail: &str, checked: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 8.0, Color32::from_white_alpha(20));
    }
    if checked {
        icons::paint(p, Icon::Check, Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(16.0, 16.0)), theme::ACCENT);
    }
    let font = if checked { FontId::new(14.0, theme::semibold()) } else { FontId::proportional(14.0) };
    p.text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, label, font, theme::TEXT);
    if !detail.is_empty() {
        p.text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, detail, FontId::proportional(12.0), theme::TEXT_DIM);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

pub fn menu_header(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    ui.painter().text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, text.to_uppercase(), FontId::new(11.0, theme::semibold()), theme::TEXT_DIM);
}

pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().line_segment([pos2(rect.left() + 8.0, rect.center().y), pos2(rect.right() - 8.0, rect.center().y)], Stroke::new(1.0, Color32::from_white_alpha(22)));
}

/// "−  value  +" stepper row inside a menu. Returns -1, 0 or +1.
pub fn menu_stepper(ui: &mut Ui, label: &str, value: &str) -> i32 {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let p = ui.painter().clone();
    p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), theme::TEXT);
    let plus = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(28.0, 26.0));
    let minus = plus.translate(vec2(-82.0, 0.0));
    p.text(pos2((plus.left() + minus.right()) / 2.0, rect.center().y), Align2::CENTER_CENTER, value, FontId::proportional(13.0), theme::TEXT_DIM);
    let mut out = 0;
    for (r, sign, glyph) in [(minus, -1, "−"), (plus, 1, "+")] {
        let resp = ui.interact(r, Id::unique(("stepper", label, sign)), Sense::click());
        let bg = if resp.hovered() { 34 } else { 18 };
        p.rect_filled(r, 7.0, Color32::from_white_alpha(bg));
        p.rect_stroke(r, 7.0, Stroke::new(1.0, Color32::from_white_alpha(14)), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(16.0), theme::TEXT);
        if resp.clicked() {
            out = sign;
        }
    }
    out
}
