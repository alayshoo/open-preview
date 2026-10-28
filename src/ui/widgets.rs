//! Custom-painted controls.

use egui::epaint::{Mesh, PathShape, RectShape, Shadow};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use super::icons::{self, Icon};
use super::theme;
use crate::player::{Chapter, format_time};

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

/// Glyphs for the title bar's window controls, drawn on a 12×12 grid at a
/// 1.2px stroke, the way Vault's title bar draws them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowGlyph {
    Pin { on: bool },
    Minimize,
    Maximize,
    Restore,
    ExitFullscreen,
    Close,
}

/// A 44px, full-height window control: transparent at rest, `BG4` and
/// brighter on hover. Close gets a stronger neutral rather than a colour.
pub fn window_control(ui: &mut Ui, rect: Rect, glyph: WindowGlyph, id: Id, tooltip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.15);
    let p = ui.painter();
    if hover > 0.0 {
        let bg = if glyph == WindowGlyph::Close { theme::COLOR5 } else { theme::BG4 };
        p.rect_filled(rect, 0.0, bg.gamma_multiply(hover));
    }
    let on = matches!(glyph, WindowGlyph::Pin { on: true });
    let color = if on { theme::COLOR1 } else { lerp_color(theme::COLOR2, theme::COLOR1, hover) };
    paint_window_glyph(p, glyph, Rect::from_center_size(rect.center(), vec2(12.0, 12.0)), color);
    if tooltip.is_empty() { resp } else { resp.on_hover_text_at_pointer(tooltip) }
}

fn paint_window_glyph(p: &Painter, glyph: WindowGlyph, r: Rect, color: Color32) {
    let at = |x: f32, y: f32| pos2(r.left() + x, r.top() + y);
    let stroke = Stroke::new(1.2, color);
    let line = |pts: Vec<Pos2>| p.add(Shape::Path(PathShape::line(pts, stroke)));
    match glyph {
        WindowGlyph::Minimize => {
            line(vec![at(1.5, 6.0), at(10.5, 6.0)]);
        }
        WindowGlyph::Maximize => {
            p.rect_stroke(Rect::from_min_max(at(1.4, 1.4), at(10.6, 10.6)), 1.6, stroke, StrokeKind::Middle);
        }
        WindowGlyph::Restore => {
            p.rect_stroke(Rect::from_min_max(at(1.2, 3.6), at(8.4, 10.8)), 1.4, stroke, StrokeKind::Middle);
            // The window behind: up, across and down, with rounded corners.
            let mut pts = vec![at(3.9, 3.6)];
            pts.extend(corner(at(5.3, 2.6), 1.4, 180.0));
            pts.extend(corner(at(9.4, 2.6), 1.4, 270.0));
            pts.extend(corner(at(9.4, 6.7), 1.4, 0.0));
            pts.push(at(8.4, 8.1));
            line(pts);
        }
        WindowGlyph::ExitFullscreen => {
            line(vec![at(4.5, 1.5), at(4.5, 4.5), at(1.5, 4.5)]);
            line(vec![at(7.5, 1.5), at(7.5, 4.5), at(10.5, 4.5)]);
            line(vec![at(1.5, 7.5), at(4.5, 7.5), at(4.5, 10.5)]);
            line(vec![at(10.5, 7.5), at(7.5, 7.5), at(7.5, 10.5)]);
        }
        WindowGlyph::Close => {
            line(vec![at(2.2, 2.2), at(9.8, 9.8)]);
            line(vec![at(9.8, 2.2), at(2.2, 9.8)]);
        }
        WindowGlyph::Pin { on } => {
            // A thumbtack: cap, neck flaring into the head, and the needle.
            if on {
                p.rect_filled(Rect::from_min_max(at(4.9, 1.4), at(7.1, 4.9)), 0.0, color);
                p.add(Shape::convex_polygon(vec![at(4.9, 4.8), at(3.0, 7.0), at(9.0, 7.0), at(7.1, 4.8)], color, Stroke::NONE));
            }
            line(vec![at(3.9, 1.4), at(8.1, 1.4)]);
            p.add(Shape::Path(PathShape::closed_line(
                vec![at(4.9, 1.4), at(4.9, 4.8), at(3.0, 7.0), at(9.0, 7.0), at(7.1, 4.8), at(7.1, 1.4)],
                stroke,
            )));
            line(vec![at(6.0, 7.0), at(6.0, 10.6)]);
        }
    }
}

/// Points along a quarter circle of `radius` around `center`, starting at
/// `from_deg` (0° = east, 90° = south) and turning clockwise on screen.
fn corner(center: Pos2, radius: f32, from_deg: f32) -> Vec<Pos2> {
    (0..=6)
        .map(|i| {
            let a = (from_deg + 15.0 * i as f32).to_radians();
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// A borderless square button: `COLOR2` at rest, `BG3` and `COLOR1` on hover.
/// Vault's player-row and title-bar lead buttons.
pub fn flat_button(ui: &mut Ui, rect: Rect, icon: Icon, id: Id, tooltip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.15);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, 7.0, theme::BG3.gamma_multiply(hover));
    }
    let color = lerp_color(theme::COLOR2, theme::COLOR1, hover);
    icons::paint(p, icon, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), color);
    if tooltip.is_empty() { resp } else { resp.on_hover_text_at_pointer(tooltip) }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

/// Pill-shaped text label, used for timestamps above the seek bar and OSD.
pub fn chip(p: &Painter, anchor: Pos2, align: Align2, text: &str, font: FontId, alpha: f32) -> Rect {
    let galley = p.layout_no_wrap(text.to_string(), font, theme::TEXT.gamma_multiply(alpha));
    let size = galley.size() + vec2(20.0, 10.0);
    let rect = align.anchor_size(anchor, size);
    let r = CornerRadius::same(7);
    p.add(Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha((128.0 * alpha) as u8) }.as_shape(rect, r));
    p.add(RectShape::filled(rect, r, theme::PANEL.gamma_multiply(alpha)));
    p.rect_stroke(rect, r, Stroke::new(1.0, theme::BG5.gamma_multiply(alpha)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, theme::TEXT);
    rect
}

pub enum SeekEvent {
    None,
    /// The pointer is held on the bar (cheap keyframe seek).
    Scrub(f64),
    /// Released; seek exactly here.
    Commit(f64),
}

/// Space kept clear either side of the head, and either side of a chapter cut.
const HEAD_GAP: f32 = 12.0;
const CHAPTER_GAP: f32 = 3.0;

/// Vault's timeline: one track cut into pieces by chapters and by the head,
/// played pieces shaded from `COLOR4` to `COLOR1` along the track, and a pill
/// for a head that grows as you take hold of it.
pub struct SeekBar<'a> {
    pub position: f64,
    pub duration: f64,
    /// End of what's been buffered ahead of the position, in seconds.
    pub buffered: f64,
    pub chapters: &'a [Chapter],
}

impl SeekBar<'_> {
    /// `rect` is the 22px-tall control; the hit area extends 14px above it
    /// into the empty gradient and 4px below, short of the buttons.
    pub fn show(self, ui: &mut Ui, rect: Rect, id: Id) -> SeekEvent {
        let hit = Rect::from_min_max(rect.min - vec2(0.0, 14.0), rect.max + vec2(0.0, 4.0));
        let resp = ui.interact(hit, id, Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        let dur = self.duration.max(0.001);
        let (left, width) = (rect.left(), rect.width().max(1.0));
        let time_at = |x: f32| ((x - left) / width).clamp(0.0, 1.0) as f64 * dur;
        let x_at = |t: f64| left + width * (t / dur).clamp(0.0, 1.0) as f32;

        let dragging = resp.is_pointer_button_down_on();
        let hovering = resp.hovered() || dragging;
        let pointer = if dragging || resp.clicked() || resp.drag_stopped() {
            resp.interact_pointer_pos()
        } else {
            resp.hover_pos()
        };
        let pointer_t = pointer.map(|p| time_at(p.x));
        // While held, the head goes where the pointer is.
        let position = if dragging { pointer_t.unwrap_or(self.position) } else { self.position };
        let split_x = x_at(position);

        let travel = |key: &str, target: f32| ctx.animate_value_with_time(id.with(key), target, 0.2);
        let rails_h = travel("rails", if hovering { 6.0 } else { 4.0 });
        let (head_w, head_h) = match (dragging, hovering) {
            (true, _) => (12.0, 22.0),
            (false, true) => (6.0, 20.0),
            _ => (4.0, 16.0),
        };
        let (head_w, head_h) = (travel("head-w", head_w), travel("head-h", head_h));

        let p = ui.painter();
        let rails = Rect::from_center_size(rect.center(), vec2(width, rails_h));
        // The soft shadow that keeps a pale bar readable over a pale frame.
        p.add(Shadow { offset: [0, 0], blur: 12, spread: 0, color: Color32::from_black_alpha(0x40) }.as_shape(rails, CornerRadius::same(PIECE_RADIUS as u8)));

        let mut cuts: Vec<f64> = self.chapters.iter().map(|c| c.start).filter(|t| *t > 0.0 && *t < dur).collect();
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        let mut bounds = vec![0.0];
        bounds.extend(cuts);
        bounds.push(dur);

        let loaded_x = x_at(self.buffered);
        for pair in bounds.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let a = x_at(from) + if from > 0.0 { CHAPTER_GAP / 2.0 } else { 0.0 };
            let b = x_at(to) - if to < dur { CHAPTER_GAP / 2.0 } else { 0.0 };

            let played_end = b.min(split_x - HEAD_GAP);
            if played_end > a {
                played_piece(ui, Rect::from_x_y_ranges(a..=played_end, rails.y_range()), left, width);
            }
            let ahead = a.max(split_x + HEAD_GAP);
            if b > ahead {
                track_piece(p, Rect::from_x_y_ranges(ahead..=b, rails.y_range()), theme::COLOR3);
                // What's buffered: a hint, so barely there.
                let loaded_end = b.min(loaded_x);
                if self.buffered > position && loaded_end > ahead {
                    track_piece(p, Rect::from_x_y_ranges(ahead..=loaded_end, rails.y_range()), theme::COLOR2.gamma_multiply(0.45));
                }
            }
        }

        if let (Some(hp), false) = (resp.hover_pos(), dragging) {
            let x = hp.x.clamp(left, left + width);
            p.rect_filled(Rect::from_min_size(pos2(x, rails.top() - 2.0), vec2(1.0, 8.0)), 0.0, theme::COLOR2.gamma_multiply(0.7));
        }

        let head_x = left + 2.0 + (position / dur).clamp(0.0, 1.0) as f32 * (width - 4.0);
        let head = Rect::from_center_size(pos2(head_x, rect.center().y), vec2(head_w, head_h));
        let radius = CornerRadius::same((head_w / 2.0).min(5.0).round() as u8);
        p.add(Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(38) }.as_shape(head, radius));
        p.rect_filled(head, radius, theme::COLOR1);

        if let Some(t) = pointer_t.filter(|_| hovering) {
            let chapter = self.chapters.iter().rev().find(|c| c.start <= t).map(|c| c.title.as_str()).filter(|s| !s.is_empty());
            hover_card(p, rect, pointer.map_or(head_x, |p| p.x.clamp(left, left + width)), t, chapter);
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }

        if resp.clicked() || resp.drag_stopped() {
            pointer_t.map_or(SeekEvent::None, SeekEvent::Commit)
        } else if dragging {
            pointer_t.map_or(SeekEvent::None, SeekEvent::Scrub)
        } else {
            SeekEvent::None
        }
    }
}

/// Corner radius of every piece of track, played or not.
const PIECE_RADIUS: f32 = 2.0;

/// A piece of track behind the head: a rounded rect shaded from `COLOR4` at
/// the start of the whole track to `COLOR1` at its end. The shading comes from
/// a gradient texture sampled over this piece's stretch of the track, so the
/// piece keeps the tessellator's rounded, anti-aliased corners.
fn played_piece(ui: &Ui, piece: Rect, track_left: f32, track_width: f32) {
    let u = |x: f32| ((x - track_left) / track_width).clamp(0.0, 1.0);
    let uv = Rect::from_min_max(pos2(u(piece.left()), 0.0), pos2(u(piece.right()), 1.0));
    let texture = played_fill_texture(ui.ctx());
    ui.painter().add(RectShape::filled(piece, PIECE_RADIUS, Color32::WHITE).with_texture(texture, uv));
}

fn track_piece(p: &Painter, piece: Rect, color: Color32) {
    p.rect_filled(piece, PIECE_RADIUS, color);
}

/// `COLOR4` → `COLOR1`, mixed in sRGB like the CSS `color-mix` Vault uses.
fn played_fill_texture(ctx: &egui::Context) -> egui::TextureId {
    let id = Id::unique("played-fill-texture");
    if let Some(handle) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return handle.id();
    }
    const N: usize = 256;
    let pixels = (0..N).map(|i| lerp_color(theme::COLOR4, theme::COLOR1, i as f32 / (N - 1) as f32)).collect();
    let image = egui::ColorImage::new([N, 1], pixels);
    let handle = ctx.load_texture("played-fill", image, egui::TextureOptions::LINEAR);
    let tex = handle.id();
    ctx.data_mut(|d| d.insert_temp(id, handle));
    tex
}

/// What's under the pointer, above the bar: the chapter's name over the
/// time, centred on the pointer and kept inside the track's width.
fn hover_card(p: &Painter, track: Rect, x: f32, t: f64, chapter: Option<&str>) {
    let chip = |text: &str, color: Color32| {
        let mut job = LayoutJob::simple_singleline(text.to_string(), FontId::proportional(11.0), color);
        job.wrap = TextWrapping { max_width: 180.0, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        p.layout_job(job)
    };
    let mut chips = vec![chip(&format_time(t), theme::COLOR2)];
    if let Some(name) = chapter {
        chips.push(chip(name, theme::COLOR1));
    }
    let pad = vec2(8.0, 2.0);
    let card_w = chips.iter().map(|g| g.size().x).fold(0.0, f32::max) + pad.x * 2.0;
    let card_left = (x - card_w / 2.0).clamp(track.left(), (track.right() - card_w).max(track.left()));
    let center_x = card_left + card_w / 2.0;

    // Bottom chip sits 4px above the control; each further one 4px above that.
    let mut bottom = track.top() - 4.0;
    for galley in chips {
        let size = galley.size() + pad * 2.0;
        let r = Rect::from_min_max(pos2(center_x - size.x / 2.0, bottom - size.y), pos2(center_x + size.x / 2.0, bottom));
        p.rect_filled(r, 7.0, theme::BG1.gamma_multiply(0.82));
        p.galley(r.min + pad, galley, theme::COLOR2);
        bottom = r.top() - 4.0;
    }
}

/// Vault's volume slider: the timeline's material at the size a volume needs.
/// Two pieces either side of a 12px gap, the played one shaded the same way,
/// and a head that grows on hover (5×18) and more while held (12×22).
///
/// A press within 20px of the head takes hold of it where it is; anywhere else
/// the value jumps to the pointer. Returns the new value while it changes.
pub fn slider(ui: &mut Ui, rect: Rect, id: Id, value: f64, max: f64) -> Option<f64> {
    const NEAR_HEAD: f32 = 20.0;
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let ctx = ui.ctx().clone();
    let control = Rect::from_center_size(rect.center(), vec2(rect.width(), 22.0));
    let (left, width) = (control.left(), control.width().max(5.0));
    let frac = (value / max).clamp(0.0, 1.0) as f32;
    let head_x = left + 2.0 + frac * (width - 4.0);

    let held = resp.is_pointer_button_down_on();
    let grab_id = id.with("grab");
    let mut out = None;
    if let Some(pointer) = resp.interact_pointer_pos().filter(|_| held) {
        if ctx.input(|i| i.pointer.primary_pressed()) {
            let offset = if (pointer.x - head_x).abs() <= NEAR_HEAD { pointer.x - head_x } else { 0.0 };
            ctx.data_mut(|d| d.insert_temp(grab_id, offset));
        }
        let offset: f32 = ctx.data(|d| d.get_temp(grab_id)).unwrap_or(0.0);
        let f = ((pointer.x - offset - left - 2.0) / (width - 4.0)).clamp(0.0, 1.0) as f64;
        let next = (f * max).round();
        if (next - value).abs() >= 1.0 {
            out = Some(next);
        }
    }

    let travel = |key: &str, target: f32| ctx.animate_value_with_time(id.with(key), target, 0.2);
    let (w, h) = match (held, resp.hovered()) {
        (true, _) => (12.0, 22.0),
        (false, true) => (5.0, 18.0),
        _ => (4.0, 16.0),
    };
    let (head_w, head_h) = (travel("head-w", w), travel("head-h", h));

    let p = ui.painter();
    let rails = Rect::from_center_size(control.center(), vec2(width, 4.0));
    p.add(Shadow { offset: [0, 0], blur: 12, spread: 0, color: Color32::from_black_alpha(0x40) }.as_shape(rails, CornerRadius::same(PIECE_RADIUS as u8)));
    let split = left + width * frac;
    let played_end = split - HEAD_GAP;
    if played_end > left {
        played_piece(ui, Rect::from_x_y_ranges(left..=played_end, rails.y_range()), left, width);
    }
    let ahead = split + HEAD_GAP;
    if control.right() > ahead {
        track_piece(p, Rect::from_x_y_ranges(ahead..=control.right(), rails.y_range()), theme::COLOR3);
    }
    let head = Rect::from_center_size(pos2(head_x, control.center().y), vec2(head_w, head_h));
    let radius = CornerRadius::same((head_w / 2.0).min(5.0).round() as u8);
    p.add(Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(38) }.as_shape(head, radius));
    p.rect_filled(head, radius, theme::COLOR1);
    if resp.hovered() || held {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
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
        p.rect_filled(rect, 7.0, theme::BG4);
    }
    if checked {
        icons::paint(p, Icon::Check, Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(15.0, 15.0)), theme::COLOR1);
    }
    let color = if checked || resp.hovered() { theme::COLOR1 } else { theme::COLOR2 };
    p.text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), color);
    if !detail.is_empty() {
        p.text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, detail, FontId::proportional(11.5), theme::COLOR3);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

pub fn menu_header(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    ui.painter().text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(12.0), theme::COLOR3);
}

pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().line_segment([pos2(rect.left() + 8.0, rect.center().y), pos2(rect.right() - 8.0, rect.center().y)], Stroke::new(1.0, theme::BG4));
}

/// "−  value  +" stepper row inside a menu. Returns -1, 0 or +1.
pub fn menu_stepper(ui: &mut Ui, label: &str, value: &str) -> i32 {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let p = ui.painter().clone();
    p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), theme::COLOR2);
    let plus = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(28.0, 26.0));
    let minus = plus.translate(vec2(-82.0, 0.0));
    p.text(pos2((plus.left() + minus.right()) / 2.0, rect.center().y), Align2::CENTER_CENTER, value, FontId::proportional(12.5), theme::COLOR2);
    let mut out = 0;
    for (r, sign, glyph) in [(minus, -1, "−"), (plus, 1, "+")] {
        let resp = ui.interact(r, Id::unique(("stepper", label, sign)), Sense::click());
        let (bg, fg) = if resp.hovered() { (theme::BG4, theme::COLOR1) } else { (theme::BG3, theme::COLOR2) };
        p.rect_filled(r, 7.0, bg);
        p.rect_stroke(r, 7.0, Stroke::new(1.0, theme::BG5), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), fg);
        if resp.clicked() {
            out = sign;
        }
    }
    out
}
