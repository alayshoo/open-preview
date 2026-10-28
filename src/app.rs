use std::sync::{Arc, Mutex};

use eframe::egui_glow;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Area, Color32, CornerRadius, CursorIcon, Event, FontId, Frame, Id, Key, Margin, Modifiers, MouseWheelUnit,
    Order, Pos2, Rect, ResizeDirection, ScrollArea, Sense, Shadow, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
    ViewportCommand, WindowLevel, pos2, vec2,
};
use egui_glow::glow::{self, HasContext};
use serde::{Deserialize, Serialize};

use crate::mpv::{Mpv, ProcAddress, RenderContext};
use crate::platform;
use crate::player::{Player, TrackKind, format_time, is_subtitle};
use crate::ui::icons::{self, Icon};
use crate::ui::theme;
use crate::ui::widgets::{self, SeekBar, SeekEvent, WindowGlyph};

/// Seconds of pointer inactivity before the controls fade out.
const HIDE_AFTER: f64 = 2.2;
/// Title bar window controls: Open Darkroom's 36×28 buttons, 2px apart, kept
/// the same distance from the window's top and side edges.
const CONTROL_W: f32 = 36.0;
const CONTROL_H: f32 = 28.0;
const CONTROL_GAP: f32 = 2.0;
const CHROME_INSET: f32 = 8.0;
const TOP_BAR_H: f32 = CONTROL_H + 2.0 * CHROME_INSET;
const EDGE_GRIP: f32 = 6.0;
const SPEEDS: &[f64] = &[0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0];

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    volume: f64,
    muted: bool,
    sub_scale: f64,
    show_remaining: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { volume: 80.0, muted: false, sub_scale: 1.0, show_remaining: false }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Subtitles,
    Audio,
    Speed,
}

#[derive(Default)]
struct Video {
    ctx: Option<RenderContext>,
    error: Option<String>,
}

pub struct OpenPreviewApp {
    player: Player,
    video: Arc<Mutex<Video>>,
    get_proc: Option<Arc<ProcAddress>>,
    settings: Settings,

    last_activity: f64,
    top_rect: Rect,
    bottom_rect: Rect,
    menu: Option<(Menu, Rect)>,
    scrubbing: Option<f64>,
    last_scrub_sent: f64,
    toast: Option<(String, f64)>,
    flash: Option<(Icon, f64)>,
    /// Always on top. The window is already the picture-in-picture; pinning
    /// keeps it above other apps when it loses focus.
    pinned: bool,
    show_help: bool,
    /// Last `sub-pos` sent to mpv; subtitles ease up above the control bar.
    sub_pos: f64,
    /// Whether we're the default video player (Windows only); re-checked when
    /// the window regains focus, e.g. on the way back from Settings.
    is_default_player: Option<bool>,
    was_focused: bool,
    /// Size of the open menu last frame; see `show_menu`.
    menu_size: Option<Vec2>,
    press_on_edge: bool,
    wheel_accum: f32,
    window_title: String,
    /// Files from the command line, opened once the video output exists.
    pending_files: Vec<String>,
}

impl OpenPreviewApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mpv: Mpv, files: Vec<String>) -> Self {
        theme::install(&cc.egui_ctx);
        platform::init(cc);

        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();

        let mut mpv = mpv;
        let ctx = cc.egui_ctx.clone();
        mpv.set_wakeup_callback(move || ctx.request_repaint());
        mpv.set_double("volume", settings.volume);
        mpv.set_flag("mute", settings.muted);
        mpv.set_double("sub-scale", settings.sub_scale);

        let player = Player::new(Arc::new(mpv));

        OpenPreviewApp {
            player,
            video: Arc::default(),
            get_proc: cc.get_proc_address.clone(),
            settings,
            last_activity: 0.0,
            top_rect: Rect::NOTHING,
            bottom_rect: Rect::NOTHING,
            menu: None,
            scrubbing: None,
            last_scrub_sent: 0.0,
            toast: None,
            flash: None,
            pinned: false,
            show_help: false,
            sub_pos: 100.0,
            menu_size: None,
            is_default_player: platform::is_default_video_player(),
            was_focused: true,
            press_on_edge: false,
            wheel_accum: 0.0,
            window_title: String::new(),
            pending_files: files,
        }
    }

    fn toast(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.toast = Some((text.into(), ctx.input(|i| i.time)));
    }

    // --- window management --------------------------------------------------

    fn is_maximized(ctx: &egui::Context) -> bool {
        ctx.input(|i| i.viewport().maximized.unwrap_or(false))
    }

    fn toggle_maximized(ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!Self::is_maximized(ctx)));
    }

    fn toggle_pin(&mut self, ctx: &egui::Context) {
        self.pinned = !self.pinned;
        let level = if self.pinned { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        self.toast(ctx, if self.pinned { "Pinned on top" } else { "Unpinned" });
    }

    /// Resizes the window to hug the video when a new video size arrives.
    fn fit_window_to_video(&mut self, ctx: &egui::Context) {
        let aspect = self.player.aspect();
        platform::set_aspect_lock(aspect);
        let Some(aspect) = aspect else { return };
        if Self::is_maximized(ctx) {
            return;
        }
        let info = ctx.input(|i| i.viewport().clone());
        let Some(inner) = info.inner_rect else { return };
        let monitor = info.monitor_size.unwrap_or(vec2(1920.0, 1080.0));
        // Keep the window's area, so 16:9 and 9:16 swap shapes instead of a
        // portrait video squeezing into the old width.
        let area = inner.width() * inner.height();
        let mut size = vec2((area * aspect).sqrt(), (area / aspect).sqrt());
        let max = monitor * 0.88;
        size *= (max.x / size.x).min(max.y / size.y).min(1.0);
        if (size - inner.size()).abs().max_elem() <= 1.0 {
            return;
        }
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        // Grow or shrink about the window's centre, so it stays on screen.
        if let Some(outer) = info.outer_rect {
            let frame = outer.size() - inner.size();
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(outer.center() - (size + frame) / 2.0));
        }
    }

    // --- actions ------------------------------------------------------------

    fn open_dialog(&mut self) {
        let files = rfd::FileDialog::new()
            .set_title("Open media")
            .add_filter(
                "Media",
                &[
                    "mkv", "mp4", "m4v", "avi", "mov", "webm", "wmv", "flv", "ts", "m2ts", "mts", "mpg", "mpeg",
                    "ogv", "3gp", "mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "wma", "m3u", "m3u8",
                ],
            )
            .add_filter("All files", &["*"])
            .pick_files();
        if let Some(files) = files {
            let paths: Vec<String> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            self.player.open_paths(&paths);
        }
    }

    fn add_subtitle_dialog(&mut self, ctx: &egui::Context) {
        let file = rfd::FileDialog::new()
            .set_title("Add subtitles")
            .add_filter("Subtitles", &["srt", "ass", "ssa", "vtt", "sub", "sup", "idx", "smi"])
            .pick_file();
        if let Some(f) = file {
            self.player.mpv.cmd(&["sub-add", &f.to_string_lossy(), "select"]);
            self.toast(ctx, "Subtitles added");
        }
    }

    fn toggle_pause(&mut self, ctx: &egui::Context) {
        if !self.player.has_media() {
            return;
        }
        let icon = if self.player.paused || self.player.eof { Icon::Play } else { Icon::Pause };
        self.player.toggle_pause();
        self.flash = Some((icon, ctx.input(|i| i.time)));
    }

    fn change_volume(&mut self, ctx: &egui::Context, delta: f64) {
        let v = (self.player.volume + delta).clamp(0.0, 150.0).round();
        self.player.set_volume(v);
        self.settings.volume = v;
        self.settings.muted = false;
        self.toast(ctx, format!("Volume {v:.0}%"));
    }

    fn toggle_mute(&mut self, ctx: &egui::Context) {
        let m = !self.player.muted;
        self.player.set_muted(m);
        self.settings.muted = m;
        self.toast(ctx, if m { "Muted" } else { "Unmuted" });
    }

    fn set_speed(&mut self, ctx: &egui::Context, s: f64) {
        self.player.set_speed(s);
        self.toast(ctx, format!("Speed {}", speed_label(self.player.speed)));
    }

    fn step_speed(&mut self, ctx: &egui::Context, dir: i32) {
        let cur = self.player.speed;
        let next = if dir > 0 {
            SPEEDS.iter().copied().find(|s| *s > cur + 1e-3).unwrap_or(cur)
        } else {
            SPEEDS.iter().rev().copied().find(|s| *s < cur - 1e-3).unwrap_or(cur)
        };
        self.set_speed(ctx, next);
    }

    /// Selects the next track of `kind` (wrapping through "off" for subtitles).
    fn cycle_track(&mut self, ctx: &egui::Context, kind: TrackKind) {
        let tracks: Vec<_> = self.player.tracks_of(kind).cloned().collect();
        if tracks.is_empty() {
            self.toast(ctx, if kind == TrackKind::Sub { "No subtitles" } else { "No audio tracks" });
            return;
        }
        let cur = tracks.iter().position(|t| t.selected);
        let next = match (cur, kind) {
            (None, _) => Some(0),
            (Some(i), _) if i + 1 < tracks.len() => Some(i + 1),
            (Some(_), TrackKind::Sub) => None,
            (Some(_), _) => Some(0),
        };
        let id = next.map(|i| tracks[i].id);
        self.player.select_track(kind, id);
        let name = next.map(|i| tracks[i].label.clone()).unwrap_or_else(|| "Off".into());
        let what = if kind == TrackKind::Sub { "Subtitles" } else { "Audio" };
        self.toast(ctx, format!("{what}: {name}"));
    }

    fn adjust_sub_delay(&mut self, ctx: &egui::Context, delta: f64) {
        let d = ((self.player.sub_delay + delta) * 10.0).round() / 10.0;
        self.player.set_sub_delay(d);
        self.toast(ctx, format!("Subtitle delay {d:+.1}s"));
    }

    // --- input --------------------------------------------------------------

    fn handle_input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                Event::Key { key, pressed: true, modifiers, .. } => self.on_key(ctx, key, modifiers),
                Event::Paste(text) => {
                    let text = text.trim().trim_matches('"').to_string();
                    if !text.is_empty() {
                        self.player.open_paths(&[text]);
                    }
                }
                Event::MouseWheel { unit, delta, .. } if self.menu.is_none() && self.player.has_media() => {
                    let steps = match unit {
                        MouseWheelUnit::Point => {
                            self.wheel_accum += delta.y;
                            let s = (self.wheel_accum / 40.0).trunc();
                            self.wheel_accum -= s * 40.0;
                            s
                        }
                        _ => delta.y.signum() * delta.y.abs().ceil(),
                    };
                    if steps != 0.0 {
                        self.change_volume(ctx, steps as f64 * 5.0);
                    }
                }
                _ => {}
            }
        }

        let dropped: Vec<String> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_string_lossy().into_owned()).filter(|p| !p.is_empty()).collect()
        });
        if !dropped.is_empty() {
            let only_subs = dropped.iter().all(|p| is_subtitle(p));
            self.player.open_paths(&dropped);
            if only_subs && self.player.has_media() {
                self.toast(ctx, "Subtitles added");
            }
        }
    }

    fn on_key(&mut self, ctx: &egui::Context, key: Key, m: Modifiers) {
        let p = &mut self.player;
        if m.command {
            match key {
                Key::O => self.open_dialog(),
                Key::W | Key::Q => ctx.send_viewport_cmd(ViewportCommand::Close),
                _ => {}
            }
            return;
        }
        match key {
            Key::Space | Key::K => self.toggle_pause(ctx),
            Key::ArrowLeft => p.seek_relative(if m.shift { -1.0 } else { -5.0 }),
            Key::ArrowRight => p.seek_relative(if m.shift { 1.0 } else { 5.0 }),
            Key::J => p.seek_relative(-10.0),
            Key::L => p.seek_relative(10.0),
            Key::ArrowUp => self.change_volume(ctx, 5.0),
            Key::ArrowDown => self.change_volume(ctx, -5.0),
            Key::M => self.toggle_mute(ctx),
            Key::F | Key::F11 => Self::toggle_maximized(ctx),
            Key::P => self.toggle_pin(ctx),
            Key::S => self.cycle_track(ctx, TrackKind::Sub),
            Key::A => self.cycle_track(ctx, TrackKind::Audio),
            Key::Z => self.adjust_sub_delay(ctx, -0.1),
            Key::X => self.adjust_sub_delay(ctx, 0.1),
            Key::OpenBracket => self.step_speed(ctx, -1),
            Key::CloseBracket => self.step_speed(ctx, 1),
            Key::Backspace => self.set_speed(ctx, 1.0),
            Key::Period => p.cycle_frame(true),
            Key::Comma => p.cycle_frame(false),
            Key::PageDown | Key::N => p.mpv.cmd(&["playlist-next"]),
            Key::PageUp => p.mpv.cmd(&["playlist-prev"]),
            Key::Home => p.seek_absolute(0.0, true),
            Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7
            | Key::Num8 | Key::Num9 => {
                let n = key.name().parse::<f64>().unwrap_or(0.0);
                if p.duration > 0.0 {
                    p.seek_absolute(p.duration * n / 10.0, true);
                }
            }
            Key::F1 | Key::Questionmark | Key::Slash => self.show_help = !self.show_help,
            Key::Escape => {
                if self.show_help {
                    self.show_help = false;
                } else if self.menu.is_some() {
                    self.menu = None;
                }
            }
            _ => {}
        }
    }

    // --- painting -----------------------------------------------------------

    fn paint_video(&self, ui: &Ui, rect: Rect) {
        let video = self.video.clone();
        let get_proc = self.get_proc.clone();
        let mpv = self.player.mpv.clone();
        let ctx = ui.ctx().clone();
        let callback = egui_glow::CallbackFn::new(move |info, painter| {
            let mut v = video.lock().unwrap();
            if v.ctx.is_none() && v.error.is_none() {
                // Created lazily here, where the GL context is guaranteed current.
                let result = match &get_proc {
                    Some(gp) => {
                        let gp = gp.clone();
                        let ctx = ctx.clone();
                        RenderContext::new(&mpv, Box::new(move |name| gp(name)), move || ctx.request_repaint())
                    }
                    None => Err("No OpenGL loader available".into()),
                };
                match result {
                    Ok(rc) => v.ctx = Some(rc),
                    Err(e) => {
                        eprintln!("{e}");
                        v.error = Some(e);
                    }
                }
            }
            if let Some(rc) = &v.ctx {
                let gl = painter.gl();
                let fbo = unsafe { gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) };
                let [w, h] = info.screen_size_px;
                // mpv's intermediate passes expect default GL state, not egui's.
                unsafe {
                    gl.disable(glow::SCISSOR_TEST);
                    gl.disable(glow::BLEND);
                    gl.bind_vertex_array(None);
                    gl.bind_buffer(glow::ARRAY_BUFFER, None);
                    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
                    gl.use_program(None);
                    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, None);
                }
                rc.wants_frame();
                rc.render(fbo, w as i32, h as i32, fbo == 0);
                // mpv leaves its own GL state behind; put back what egui relies on.
                unsafe {
                    let target = std::num::NonZeroU32::new(fbo as u32).map(glow::NativeFramebuffer);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, target);
                    gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
                    gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    gl.bind_vertex_array(None);
                    gl.bind_buffer(glow::ARRAY_BUFFER, None);
                }
            }
        });
        ui.painter().add(egui::PaintCallback { rect, callback: Arc::new(callback) });
    }

    fn paint_empty_state(&self, ui: &Ui, screen: Rect, hovered: bool) {
        let p = ui.painter();
        p.rect_filled(screen, 0.0, theme::BG1);

        let small = screen.height() < 260.0;
        let c = screen.center() - vec2(0.0, if small { 10.0 } else { 24.0 });
        let tile = Rect::from_center_size(c, Vec2::splat(if small { 52.0 } else { 72.0 }));
        let lift = ui.ctx().animate_bool_with_time(Id::unique("empty-hover"), hovered, 0.15);
        p.add(Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(140) }.as_shape(tile, CornerRadius::same(14)));
        p.rect_filled(tile, 14.0, theme::BG2);
        p.rect_stroke(tile, 14.0, Stroke::new(1.0, theme::BG5), StrokeKind::Inside);
        let icon_color = if lift > 0.5 { theme::COLOR1 } else { theme::COLOR2 };
        icons::paint(p, Icon::Play, tile.shrink(tile.width() * 0.3).translate(vec2(1.5, 0.0)), icon_color);

        let y = tile.bottom() + if small { 20.0 } else { 28.0 };
        p.text(pos2(c.x, y), Align2::CENTER_CENTER, "Drop a video to start", FontId::proportional(if small { 13.0 } else { 15.0 }), theme::COLOR2);
        p.text(pos2(c.x, y + 22.0), Align2::CENTER_CENTER, "or click anywhere to browse  ·  Ctrl+O", FontId::proportional(12.0), theme::COLOR3);
        let mut next_y = y + 46.0;
        if let Some(err) = &self.player.last_error {
            p.text(pos2(c.x, next_y), Align2::CENTER_CENTER, format!("Couldn't play that file: {err}"), FontId::proportional(12.0), theme::COLOR2);
            next_y += 24.0;
        }
        if self.is_default_player == Some(false) && !small {
            let galley = p.layout_no_wrap("Make Open Preview your default video player".into(), FontId::proportional(12.0), theme::COLOR3);
            let rect = Rect::from_center_size(pos2(c.x, next_y + 8.0), galley.size());
            let resp = ui.interact(rect.expand(4.0), Id::unique("make-default"), Sense::click());
            let color = if resp.hovered() { theme::COLOR1 } else { theme::COLOR3 };
            p.galley(rect.min, galley, color);
            if resp.hovered() {
                p.hline(rect.x_range(), rect.bottom() + 1.0, Stroke::new(1.0, color));
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() {
                platform::open_default_apps_settings();
            }
        }
    }

    /// The title on the left, the window controls on the right, over a
    /// black-to-clear gradient so the video still shows through. The empty
    /// screen has nothing under the bar to fade, so it has no gradient.
    fn top_bar(&mut self, ui: &mut Ui, ctx: &egui::Context, screen: Rect) {
        let width = screen.width();
        let maximized = Self::is_maximized(ctx);
        let p = ui.painter().clone();
        let bar = Rect::from_min_size(screen.min, vec2(width, TOP_BAR_H));
        self.top_rect = bar;
        if self.player.has_media() {
            widgets::vertical_gradient(&p, Rect::from_min_size(screen.min, vec2(width, 110.0)), 0.72, 0.0);
        }

        // Window controls, right to left: close, maximise, minimise, pin.
        let tiny = width < 360.0;
        let mut x = bar.right() - CHROME_INSET + CONTROL_GAP;
        let mut place = || {
            x -= CONTROL_W + CONTROL_GAP;
            Rect::from_min_size(pos2(x, bar.top() + CHROME_INSET), vec2(CONTROL_W, CONTROL_H))
        };
        if widgets::window_control(ui, place(), WindowGlyph::Close, Id::unique("close"), "").clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        if !tiny {
            let glyph = if maximized { WindowGlyph::Restore } else { WindowGlyph::Maximize };
            if widgets::window_control(ui, place(), glyph, Id::unique("max"), "").clicked() {
                Self::toggle_maximized(ctx);
            }
            if widgets::window_control(ui, place(), WindowGlyph::Minimize, Id::unique("min"), "").clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
            }
        }
        let pin_tip = if self.pinned { "Unpin (P)" } else { "Keep on top (P)" };
        let pin = place();
        if widgets::window_control(ui, pin, WindowGlyph::Pin { on: self.pinned }, Id::unique("pin"), pin_tip).clicked() {
            self.toggle_pin(ctx);
        }

        // Leading, in line with the timeline's 16px sides, and ellipsed short
        // of the controls rather than running under them.
        let left = bar.left() + 16.0;
        let max_w = pin.left() - 12.0 - left;
        if max_w > 40.0 {
            let title = if self.player.has_media() && !self.player.title.is_empty() { self.player.title.as_str() } else { "Open Preview" };
            // Vault's dim title sits on a solid bar; over video it needs the
            // next step up the ramp to stay legible.
            let color = if self.player.has_media() { theme::COLOR2 } else { theme::COLOR3 };
            let mut job = LayoutJob::simple_singleline(title.to_string(), FontId::proportional(13.0), color);
            job.wrap = TextWrapping { max_width: max_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
            let galley = p.layout_job(job);
            p.galley(pos2(left, pin.center().y - galley.size().y / 2.0), galley, color);
        }
    }

    fn bottom_bar(&mut self, ui: &mut Ui, ctx: &egui::Context, screen: Rect) {
        let width = screen.width();
        let compact = width < 620.0;
        let tiny = width < 400.0;
        let p = ui.painter().clone();
        widgets::vertical_gradient(&p, Rect::from_min_max(pos2(screen.left(), screen.bottom() - 160.0), screen.max), 0.0, 0.86);

        // Vault's controls block: 16px sides, a 22px timeline, 4px, then the row.
        let row_y = screen.bottom() - 30.0;
        let seek_bottom = row_y - 18.0 - 4.0;
        let seek_rect = Rect::from_min_max(pos2(screen.left() + 16.0, seek_bottom - 22.0), pos2(screen.right() - 16.0, seek_bottom));
        self.bottom_rect = Rect::from_min_max(pos2(screen.left(), seek_rect.top() - 22.0), screen.max);

        // Seek bar
        let player = &self.player;
        let seek = SeekBar {
            position: player.time_pos,
            duration: player.duration,
            chapters: &player.chapters,
        };
        if player.duration > 0.0 {
            let now = ctx.input(|i| i.time);
            match seek.show(ui, seek_rect, Id::unique("seek")) {
                SeekEvent::Scrub(t) => {
                    let moved = self.scrubbing.is_none_or(|prev| (prev - t).abs() > 0.01);
                    self.scrubbing = Some(t);
                    if moved && now - self.last_scrub_sent > 0.06 {
                        self.player.seek_absolute(t, false);
                        self.last_scrub_sent = now;
                    }
                }
                SeekEvent::Commit(t) => {
                    self.scrubbing = None;
                    self.player.seek_absolute(t, true);
                }
                SeekEvent::None => self.scrubbing = None,
            }
        }

        let btn = 36.0;
        let mut x = screen.left() + 10.0;
        let mut next = |w: f32| {
            let r = Rect::from_min_size(pos2(x, row_y - btn / 2.0), vec2(w, btn));
            x += w + 2.0;
            r
        };

        let play_icon = if self.player.eof {
            Icon::Replay
        } else if self.player.paused {
            Icon::Play
        } else {
            Icon::Pause
        };
        let r = next(40.0).expand(2.0);
        if widgets::icon_button(ui, r, play_icon, Id::unique("play"), "").clicked() {
            self.toggle_pause(ctx);
        }
        if !compact {
            if widgets::icon_button(ui, next(btn), Icon::Back10, Id::unique("back10"), "Back 10s (J)").clicked() {
                self.player.seek_relative(-10.0);
            }
            if widgets::icon_button(ui, next(btn), Icon::Forward10, Id::unique("fwd10"), "Forward 10s (L)").clicked() {
                self.player.seek_relative(10.0);
            }
        }
        if self.player.playlist_count > 1 && !tiny {
            if widgets::icon_button(ui, next(btn), Icon::Prev, Id::unique("prev"), "Previous (PgUp)").clicked() {
                self.player.mpv.cmd(&["playlist-prev"]);
            }
            if widgets::icon_button(ui, next(btn), Icon::Next, Id::unique("next"), "Next (PgDn)").clicked() {
                self.player.mpv.cmd(&["playlist-next"]);
            }
        }
        if !tiny {
            let vol_icon = match (self.player.muted, self.player.volume) {
                (true, _) => Icon::Muted,
                (_, v) if v <= 0.0 => Icon::Muted,
                (_, v) if v < 50.0 => Icon::Volume(1),
                _ => Icon::Volume(2),
            };
            if widgets::icon_button(ui, next(btn), vol_icon, Id::unique("mute"), "Mute (M)").clicked() {
                self.toggle_mute(ctx);
            }
            if !compact {
                let shown = if self.player.muted { 0.0 } else { self.player.volume };
                if let Some(v) = widgets::slider(ui, next(84.0), Id::unique("volume"), shown, 100.0_f64.max(shown)) {
                    self.player.set_volume(v);
                    self.settings.volume = v;
                    self.settings.muted = false;
                }
            }
        }

        // Time readout: click toggles elapsed/remaining.
        let pos = self.scrubbing.unwrap_or(self.player.time_pos);
        let cur = if self.settings.show_remaining && self.player.duration > 0.0 {
            format!("-{}", format_time(self.player.duration - pos))
        } else {
            format_time(pos)
        };
        let font = FontId::proportional(13.5);
        let start = pos2(x + 8.0, row_y);
        let r1 = p.text(start, Align2::LEFT_CENTER, &cur, font.clone(), theme::TEXT);
        let mut time_rect = r1;
        if self.player.duration > 0.0 && !tiny {
            let r2 = p.text(pos2(r1.right(), row_y), Align2::LEFT_CENTER, format!("  /  {}", format_time(self.player.duration)), font, theme::TEXT_DIM);
            time_rect = time_rect.union(r2);
        }
        let time_resp = ui.interact(time_rect.expand(4.0), Id::unique("time"), Sense::click());
        if time_resp.clicked() {
            self.settings.show_remaining = !self.settings.show_remaining;
        }
        let time_end = time_rect.right();

        // Right cluster, laid out right-to-left.
        let mut rx = screen.right() - 10.0;
        let mut prev = |w: f32| {
            rx -= w;
            let r = Rect::from_min_size(pos2(rx, row_y - btn / 2.0), vec2(w, btn));
            rx -= 2.0;
            r
        };
        let maximized = Self::is_maximized(ctx);
        let (max_icon, max_tip) = if maximized { (Icon::FullscreenExit, "Restore (F)") } else { (Icon::Fullscreen, "Maximize (F)") };
        if widgets::icon_button(ui, prev(btn), max_icon, Id::unique("maximize"), max_tip).clicked() {
            Self::toggle_maximized(ctx);
        }
        // `shape` is what the button draws, so the open-menu ring follows it: a
        // circle around an icon, a pill around the speed label.
        let menu_btn = |ui: &mut Ui, r: Rect, shape: Rect, menu: Menu, resp: egui::Response, current: &mut Option<(Menu, Rect)>| {
            if resp.clicked() {
                *current = match current {
                    Some((m, _)) if *m == menu => None,
                    _ => Some((menu, r)),
                };
            }
            if let Some((m, anchor)) = current
                && *m == menu {
                    *anchor = r;
                    let ring = shape.shrink(1.0);
                    ui.painter().rect_stroke(ring, ring.height() / 2.0, Stroke::new(1.0, theme::COLOR4), StrokeKind::Inside);
                }
        };
        if !compact {
            let r = prev(btn);
            if r.left() > time_end + 8.0 && widgets::icon_button(ui, r, Icon::Keyboard, Id::unique("help"), "Shortcuts (F1)").clicked() {
                self.show_help = !self.show_help;
            }
        }
        if !tiny {
            let r = prev(btn);
            if r.left() > time_end + 8.0 {
                let resp = widgets::icon_button(ui, r, Icon::Subtitles, Id::unique("subs"), "Subtitles (S)");
                menu_btn(ui, r, r, Menu::Subtitles, resp, &mut self.menu);
            }
        }
        if !compact && self.player.tracks_of(TrackKind::Audio).count() > 0 {
            let r = prev(btn);
            if r.left() > time_end + 8.0 {
                let resp = widgets::icon_button(ui, r, Icon::Audio, Id::unique("audio"), "Audio track (A)");
                menu_btn(ui, r, r, Menu::Audio, resp, &mut self.menu);
            }
        }
        if !compact {
            let r = prev(52.0);
            if r.left() > time_end + 8.0 {
                let pill = r.shrink2(vec2(2.0, 4.0));
                let resp = widgets::text_button(ui, pill, &speed_label(self.player.speed), Id::unique("speed"));
                menu_btn(ui, r, pill, Menu::Speed, resp, &mut self.menu);
            }
        }
    }

    fn show_menu(&mut self, ctx: &egui::Context, screen: Rect) {
        let Some((menu, anchor)) = self.menu else { return };
        let pos = pos2((anchor.right() + 40.0).min(screen.right() - 8.0), anchor.top() - 10.0);
        let max_h = (anchor.top() - screen.top() - 20.0).max(80.0);
        let mut close = false;
        let mut want_sub_file = false;

        // One area per menu: a bottom-anchored area is placed using the size it
        // had last frame, so sharing one between menus of different heights
        // placed each new menu by the previous one's size and cut it off.
        let area = Area::new(Id::unique(("menu-panel", menu as u8)))
            .order(Order::Foreground)
            .fixed_pos(pos)
            .pivot(Align2::RIGHT_BOTTOM)
            .constrain_to(screen.shrink(6.0))
            .fade_in(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL)
                    .corner_radius(10.0)
                    .stroke(Stroke::new(1.0, theme::BG5))
                    .shadow(Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(140) })
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        ui.set_width(250.0);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                            let player = &mut self.player;
                            match menu {
                                Menu::Subtitles => {
                                    widgets::menu_header(ui, "Subtitles");
                                    let any = player.tracks_of(TrackKind::Sub).any(|t| t.selected);
                                    if widgets::menu_row(ui, "Off", "", !any).clicked() {
                                        player.select_track(TrackKind::Sub, None);
                                    }
                                    let subs: Vec<_> = player.tracks_of(TrackKind::Sub).cloned().collect();
                                    for t in subs {
                                        if widgets::menu_row(ui, &t.label, &t.detail, t.selected).clicked() {
                                            player.select_track(TrackKind::Sub, Some(t.id));
                                        }
                                    }
                                    widgets::menu_separator(ui);
                                    let scale = format!("{:.0}%", player.sub_scale * 100.0);
                                    let d = widgets::menu_stepper(ui, "Size", &scale);
                                    if d != 0 {
                                        let s = ((player.sub_scale + d as f64 * 0.1) * 10.0).round() / 10.0;
                                        player.set_sub_scale(s);
                                        self.settings.sub_scale = s.clamp(0.3, 3.0);
                                    }
                                    let delay = format!("{:+.1}s", player.sub_delay);
                                    let d = widgets::menu_stepper(ui, "Delay", &delay);
                                    if d != 0 {
                                        player.set_sub_delay(((player.sub_delay + d as f64 * 0.1) * 10.0).round() / 10.0);
                                    }
                                    widgets::menu_separator(ui);
                                    if widgets::menu_row(ui, "Add subtitle file…", "", false).clicked() {
                                        want_sub_file = true;
                                    }
                                }
                                Menu::Audio => {
                                    widgets::menu_header(ui, "Audio track");
                                    let tracks: Vec<_> = player.tracks_of(TrackKind::Audio).cloned().collect();
                                    for t in tracks {
                                        if widgets::menu_row(ui, &t.label, &t.detail, t.selected).clicked() {
                                            player.select_track(TrackKind::Audio, Some(t.id));
                                            close = true;
                                        }
                                    }
                                }
                                Menu::Speed => {
                                    widgets::menu_header(ui, "Playback speed");
                                    for &s in SPEEDS {
                                        let label = if s == 1.0 { "Normal".to_string() } else { speed_label(s) };
                                        if widgets::menu_row(ui, &label, "", (player.speed - s).abs() < 1e-3).clicked() {
                                            player.set_speed(s);
                                            close = true;
                                        }
                                    }
                                }
                            }
                        });
                    });
            });

        let menu_rect = area.response.rect;
        // Whenever the content's size changes (a track list arriving, a
        // different menu), draw again so it's placed by its real size, even
        // while paused and nothing else is asking for frames.
        if self.menu_size != Some(menu_rect.size()) {
            self.menu_size = Some(menu_rect.size());
            ctx.request_repaint();
        }
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !menu_rect.contains(p) && !anchor.contains(p))
        });
        if close || clicked_outside {
            self.menu = None;
        }
        if want_sub_file {
            self.menu = None;
            self.add_subtitle_dialog(ctx);
        }
    }

    fn paint_osd(&mut self, ui: &Ui, screen: Rect, now: f64) {
        let p = ui.painter();
        if let Some((text, t0)) = &self.toast {
            let age = now - t0;
            const LIFE: f64 = 1.4;
            if age < LIFE {
                let alpha = (((LIFE - age) / 0.3).min(age / 0.08)).clamp(0.0, 1.0) as f32;
                let y = screen.top() + if screen.height() < 260.0 { 28.0 } else { 58.0 };
                widgets::chip(p, pos2(screen.center().x, y), Align2::CENTER_TOP, text, FontId::new(14.0, theme::semibold()), alpha);
                ui.ctx().request_repaint();
            } else {
                self.toast = None;
            }
        }
        if let Some((icon, t0)) = self.flash {
            let age = (now - t0) as f32;
            const LIFE: f32 = 0.55;
            if age < LIFE {
                let k = age / LIFE;
                let alpha = 1.0 - k;
                let radius = 36.0 * (1.0 + 0.3 * k);
                let c = screen.center();
                p.circle_filled(c, radius, Color32::from_black_alpha((120.0 * alpha) as u8));
                let ir = Rect::from_center_size(c, Vec2::splat(radius * 0.9));
                let ir = if icon == Icon::Play { ir.translate(vec2(radius * 0.06, 0.0)) } else { ir };
                icons::paint(p, icon, ir, Color32::WHITE.gamma_multiply(alpha));
                ui.ctx().request_repaint();
            } else {
                self.flash = None;
            }
        }
        if self.player.buffering || (self.player.loading && self.player.has_media()) {
            icons::spinner(p, screen.center(), 22.0, now, Color32::from_white_alpha(220));
            ui.ctx().request_repaint();
        }
    }

    fn paint_help(&mut self, ctx: &egui::Context, screen: Rect) {
        let shortcuts: &[(&str, &str)] = &[
            ("Space / K", "Play / pause"),
            ("← / →", "Seek 5s (Shift: 1s)"),
            ("J / L", "Seek 10s"),
            ("0 – 9", "Jump to 0% – 90%"),
            (", / .", "Previous / next frame"),
            ("↑ / ↓ / Wheel", "Volume"),
            ("M", "Mute"),
            ("S / A", "Cycle subtitles / audio"),
            ("Z / X", "Subtitle delay −/+ 0.1s"),
            ("[ / ] / Backspace", "Speed down / up / reset"),
            ("F / Double-click", "Maximize / restore"),
            ("P", "Keep on top"),
            ("PgUp / PgDn", "Previous / next in playlist"),
            ("Ctrl+O / Ctrl+V", "Open file / paste path or URL"),
        ];
        let mut open = true;
        Area::new(Id::unique("help-panel"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .constrain_to(screen.shrink(8.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL)
                    .corner_radius(10.0)
                    .stroke(Stroke::new(1.0, theme::BG5))
                    .shadow(Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(140) })
                    .inner_margin(Margin::symmetric(20, 16))
                    .show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Keyboard shortcuts").color(theme::COLOR1).size(15.0));
                        });
                        ui.add_space(8.0);
                        ScrollArea::vertical().max_height(screen.height() - 120.0).show(ui, |ui| {
                            for (k, v) in shortcuts {
                                ui.horizontal(|ui| {
                                    ui.add_sized([130.0, 22.0], egui::Label::new(egui::RichText::new(*k).color(theme::COLOR1).font(FontId::proportional(12.5))));
                                    ui.label(egui::RichText::new(*v).color(theme::COLOR2).size(12.5));
                                });
                            }
                        });
                    });
            });
        if ctx.input(|i| i.pointer.any_pressed()) {
            open = false;
        }
        self.show_help = open;
        let _ = screen;
    }
}

impl eframe::App for OpenPreviewApp {
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let screen = root.max_rect();
        let now = ctx.input(|i| i.time);

        if !self.pending_files.is_empty() && self.video.lock().unwrap().ctx.is_some() {
            let files = std::mem::take(&mut self.pending_files);
            self.player.open_paths(&files);
        }
        self.player.process_events();
        if std::mem::take(&mut self.player.video_resized) {
            self.fit_window_to_video(&ctx);
        }
        let title = if self.player.has_media() && !self.player.title.is_empty() {
            format!("{} — Open Preview", self.player.title)
        } else {
            "Open Preview".to_string()
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        self.handle_input(&ctx);

        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.was_focused {
            self.is_default_player = platform::is_default_video_player();
        }
        self.was_focused = focused;

        let has_media = self.player.has_media();
        let maximized = Self::is_maximized(&ctx);

        // Activity tracking for the auto-hiding chrome.
        let (hover, moved, pressed, primary_pressed, any_key) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.delta() != Vec2::ZERO,
                i.pointer.any_pressed(),
                i.pointer.primary_pressed(),
                i.events.iter().any(|e| matches!(e, Event::Key { pressed: true, .. })),
            )
        });
        if moved || pressed || any_key {
            self.last_activity = now;
        }

        // Video first, so everything else draws over it. This also creates the
        // render context on the first frame, before any file is loaded.
        self.paint_video(root, screen);
        if !self.pending_files.is_empty() {
            ctx.request_repaint();
        }

        // Edge resizing for the borderless window.
        let edge = hover.filter(|_| !maximized).and_then(|p| edge_at(p, screen, EDGE_GRIP));
        if let Some(dir) = edge {
            ctx.set_cursor_icon(resize_cursor(dir));
            if primary_pressed {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        }
        if primary_pressed {
            self.press_on_edge = edge.is_some();
        }

        // The whole surface drags the window; controls registered later take priority.
        let bg = root.interact(screen, Id::unique("surface"), Sense::click_and_drag());
        if bg.drag_started() && !self.press_on_edge {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        // Like a native title bar: double-clicking maximises, and a single
        // click on the bar does nothing. Elsewhere a click plays or pauses, so
        // a double-click there undoes the first click's toggle.
        let on_bar = bg.interact_pointer_pos().is_some_and(|p| p.y < screen.top() + TOP_BAR_H);
        if bg.double_clicked() && !self.press_on_edge {
            Self::toggle_maximized(&ctx);
            if has_media && !on_bar && !self.show_help {
                self.player.toggle_pause();
                self.flash = None;
            }
        } else if bg.clicked() && !self.press_on_edge && !self.show_help && !on_bar {
            if has_media {
                self.toggle_pause(&ctx);
            } else {
                self.open_dialog();
            }
        }

        if !has_media {
            self.paint_empty_state(root, screen, hover.is_some());
        }

        // Decide whether the chrome is visible.
        let over_controls = hover.is_some_and(|p| self.top_rect.contains(p) || self.bottom_rect.contains(p));
        let idle = now - self.last_activity;
        let show = !has_media
            || self.menu.is_some()
            || self.scrubbing.is_some()
            || (hover.is_some() && (idle < HIDE_AFTER || over_controls || self.player.paused));
        let alpha = ctx.animate_bool_with_time(Id::unique("chrome"), show, if show { 0.15 } else { 0.4 });
        // Subtitles ease up clear of the timeline while it's shown.
        let sub_lift = ctx.animate_bool_with_time_and_easing(
            Id::unique("sub-lift"),
            show && has_media,
            if show { 0.2 } else { 0.4 },
            egui::emath::easing::cubic_out,
        ) as f64;
        let sub_pos = ((100.0 - sub_lift * (4500.0 / screen.height() as f64).min(25.0)) * 10.0).round() / 10.0;
        if sub_pos != self.sub_pos {
            self.sub_pos = sub_pos;
            self.player.mpv.set_double("sub-pos", sub_pos);
        }
        if show && has_media && hover.is_some() && !self.player.paused && !over_controls {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((HIDE_AFTER - idle).max(0.05)));
        }
        if !show && hover.is_some() && has_media && edge.is_none() {
            ctx.set_cursor_icon(CursorIcon::None);
        }

        if alpha > 0.01 {
            let mut chrome = root.new_child(UiBuilder::new().max_rect(screen).id_salt("chrome"));
            chrome.multiply_opacity(alpha);
            self.top_bar(&mut chrome, &ctx, screen);
            if has_media {
                self.bottom_bar(&mut chrome, &ctx, screen);
            } else {
                self.bottom_rect = Rect::NOTHING;
            }
        } else {
            self.top_rect = Rect::NOTHING;
            self.bottom_rect = Rect::NOTHING;
            self.menu = None;
        }
        if has_media {
            self.show_menu(&ctx, screen);
        } else {
            self.menu = None;
        }

        // Drag-and-drop hint.
        let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering_files {
            let p = root.painter();
            let r = screen.shrink(10.0);
            p.rect_filled(r, 14.0, theme::BG1.gamma_multiply(0.6));
            p.rect_stroke(r, 14.0, Stroke::new(1.5, theme::COLOR3), StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, "Drop to play", FontId::new(18.0, theme::semibold()), theme::TEXT);
        }

        self.paint_osd(root, screen, now);
        if self.show_help {
            self.paint_help(&ctx, screen);
        }

        // Audio-only files produce no video frames to drive repaints.
        if has_media && !self.player.paused && !self.player.has_video() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        self.player.save_position();
        // The render context must go before the GL context and the mpv core.
        self.video.lock().unwrap().ctx = None;
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// Shown when libmpv can't be loaded, so the user knows exactly what's missing.
pub struct FatalApp {
    pub message: String,
}

impl eframe::App for FatalApp {
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let screen = root.max_rect();
        let bg = root.interact(screen, Id::unique("surface"), Sense::click_and_drag());
        if bg.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        let p = root.painter();
        p.rect_filled(screen, 0.0, theme::BG);
        let c = screen.center();
        p.text(c - vec2(0.0, 40.0), Align2::CENTER_CENTER, "libmpv not found", FontId::new(20.0, theme::semibold()), theme::TEXT);
        p.text(
            c - vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            format!("Place {} next to the Open Preview executable.", crate::mpv::library_hint()),
            FontId::proportional(14.0),
            theme::TEXT_DIM,
        );
        let mut job = LayoutJob::simple(self.message.clone(), FontId::proportional(12.0), theme::TEXT_DIM, screen.width() - 60.0);
        job.halign = egui::Align::Center;
        let g = p.layout_job(job);
        p.galley(pos2(c.x, c.y + 20.0), g, theme::TEXT_DIM);
        let close = Rect::from_min_size(pos2(screen.right() - CHROME_INSET - CONTROL_W, screen.top() + CHROME_INSET), vec2(CONTROL_W, CONTROL_H));
        if widgets::window_control(root, close, WindowGlyph::Close, Id::unique("close"), "").clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

fn speed_label(s: f64) -> String {
    let t = format!("{s:.2}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    format!("{t}×")
}

fn edge_at(p: Pos2, r: Rect, m: f32) -> Option<ResizeDirection> {
    let l = p.x - r.left() < m;
    let rt = r.right() - p.x < m;
    let t = p.y - r.top() < m;
    let b = r.bottom() - p.y < m;
    Some(match (l, rt, t, b) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (_, _, _, true) => ResizeDirection::South,
        _ => return None,
    })
}

fn resize_cursor(d: ResizeDirection) -> CursorIcon {
    match d {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    }
}
