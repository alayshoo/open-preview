//! HDR output. On an HDR screen, HDR video is drawn as HDR instead of being
//! tone-mapped to SDR: mpv renders it as absolute PQ / BT.2020 into a float
//! texture, egui draws the UI into a texture of its own, and `Compositor`
//! puts the two together as scRGB (linear, 1.0 = 80 nits), the format Windows
//! composes HDR desktops in. The platform side (`win`) gets that to the screen
//! through a DXGI swap chain; elsewhere there is none and everything stays SDR.
//!
//! SDR video keeps the ordinary path even on an HDR screen: Windows already
//! shows SDR content at the user's chosen brightness.

use eframe::glow::{self, HasContext};

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::{Presenter, display};

/// What the screen the window is on can show, when it's in HDR mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    /// Peak brightness, for mpv to tone-map brighter video down to.
    pub peak_nits: f32,
    /// Windows' "SDR content brightness": where the UI's white goes.
    pub sdr_white_nits: f32,
}

#[cfg(not(windows))]
pub fn display() -> Option<Display> {
    None
}

/// Stand-in where there's no HDR presentation; never created.
#[cfg(not(windows))]
pub struct Presenter;

#[cfg(not(windows))]
impl Presenter {
    pub fn new(_gl: &glow::Context, _get_proc: &crate::mpv::ProcAddress) -> Result<Self, String> {
        Err("HDR output is only implemented on Windows".into())
    }
    pub fn begin(&mut self, _gl: &glow::Context, _size: [u32; 2]) -> Result<glow::Framebuffer, String> {
        Err("no HDR output here".into())
    }
    pub fn present(&mut self, _gl: &glow::Context) -> Result<(), String> {
        Err("no HDR output here".into())
    }
    pub fn hide(&mut self) {}
    pub fn destroy(self, _gl: &glow::Context) {}
}

/// The mpv options for rendering to an HDR target, or back to SDR (`None`).
pub fn mpv_target(display: Option<Display>) -> [(&'static str, String); 3] {
    match display {
        Some(d) => [
            ("target-trc", "pq".into()),
            ("target-prim", "bt.2020".into()),
            ("target-peak", format!("{:.0}", d.peak_nits)),
        ],
        None => [("target-trc", "auto".into()), ("target-prim", "auto".into()), ("target-peak", "auto".into())],
    }
}

/// A colour buffer to render into and sample from.
pub struct Target {
    pub fbo: glow::Framebuffer,
    pub tex: glow::Texture,
    pub size: [u32; 2],
    /// GL internal format, as mpv wants to be told.
    pub format: u32,
}

impl Target {
    pub(crate) unsafe fn new(gl: &glow::Context, size: [u32; 2], format: u32) -> Result<Self, String> {
        unsafe {
            let tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            let (data_format, data_type) = match format {
                glow::RGBA16F => (glow::RGBA, glow::HALF_FLOAT),
                glow::RGBA32F => (glow::RGBA, glow::FLOAT),
                _ => (glow::RGBA, glow::UNSIGNED_BYTE),
            };
            let empty = glow::PixelUnpackData::Slice(None);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, format as i32, size[0] as i32, size[1] as i32, 0, data_format, data_type, empty);
            for (param, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let target = Target { fbo, tex, size, format };
            if status != glow::FRAMEBUFFER_COMPLETE {
                target.destroy(gl);
                return Err(format!("framebuffer incomplete ({status:#x})"));
            }
            Ok(target)
        }
    }

    pub(crate) unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.tex);
        }
    }
}

const VERTEX: &str = r#"#version 140
uniform float flip;
out vec2 uv;
void main() {
    // One triangle over the whole target.
    vec2 p = vec2(float((gl_VertexID & 1) << 2) - 1.0, float((gl_VertexID & 2) << 1) - 1.0);
    uv = p * 0.5 + 0.5;
    gl_Position = vec4(p.x, p.y * flip, 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"#version 140
in vec2 uv;
out vec4 frag;
uniform sampler2D video; // mpv's output: PQ, BT.2020 primaries
uniform sampler2D ui;    // egui's: sRGB-encoded, premultiplied alpha
uniform float sdr_white; // nits

vec3 pq_to_nits(vec3 e) {
    const float m1 = 2610.0 / 16384.0, m2 = 2523.0 / 32.0;
    const float c1 = 3424.0 / 4096.0, c2 = 2413.0 / 128.0, c3 = 2392.0 / 128.0;
    vec3 p = pow(clamp(e, 0.0, 1.0), vec3(1.0 / m2));
    return 10000.0 * pow(max(p - c1, 0.0) / (c2 - c3 * p), vec3(1.0 / m1));
}

// The sRGB curve, extended past 1 and to negatives (colours outside BT.709).
vec3 encode(vec3 x) {
    vec3 a = abs(x);
    return sign(x) * mix(12.92 * a, 1.055 * pow(a, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, a));
}
vec3 decode(vec3 x) {
    vec3 a = abs(x);
    return sign(x) * mix(a / 12.92, pow((a + 0.055) / 1.055, vec3(2.4)), step(0.04045, a));
}

void main() {
    // BT.2020 to BT.709 primaries (columns), in linear light.
    const mat3 to_709 = mat3(1.6605, -0.1246, -0.0182, -0.5876, 1.1329, -0.1006, -0.0728, -0.0083, 1.1187);
    vec3 video = to_709 * pq_to_nits(texture(video, uv).rgb) / sdr_white;
    vec4 ui = texture(ui, uv);
    // egui blends in sRGB space over SDR video; blend the same way here, with
    // 1.0 as SDR white, so the controls look as they do in SDR.
    vec3 blended = encode(video) * (1.0 - ui.a) + ui.rgb;
    frag = vec4(decode(blended) * sdr_white / 80.0, 1.0);
}
"#;

/// Draws the video with the UI over it, as scRGB.
pub struct Compositor {
    program: glow::Program,
    vao: glow::VertexArray,
    flip: glow::UniformLocation,
    sdr_white: glow::UniformLocation,
    /// egui draws here: RGBA8, like the window's own framebuffer.
    pub ui: Target,
    /// mpv draws here, as PQ.
    pub video: Target,
}

impl Compositor {
    pub unsafe fn new(gl: &glow::Context, size: [u32; 2]) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, source) in [(glow::VERTEX_SHADER, VERTEX), (glow::FRAGMENT_SHADER, FRAGMENT)] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let log = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    gl.delete_program(program);
                    return Err(format!("HDR compositor shader: {log}"));
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.link_program(program);
            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("HDR compositor program: {log}"));
            }
            let uniform = |name: &str| gl.get_uniform_location(program, name).ok_or(format!("no uniform {name}"));
            let (flip, sdr_white) = (uniform("flip")?, uniform("sdr_white")?);
            gl.use_program(Some(program));
            gl.uniform_1_i32(Some(&uniform("video")?), 0);
            gl.uniform_1_i32(Some(&uniform("ui")?), 1);
            gl.use_program(None);
            let vao = gl.create_vertex_array()?;
            let ui = Target::new(gl, size, glow::RGBA8)?;
            let video = Target::new(gl, size, glow::RGBA16F)?;
            Ok(Compositor { program, vao, flip, sdr_white, ui, video })
        }
    }

    /// Reallocates both buffers when the window's size changed.
    pub unsafe fn resize(&mut self, gl: &glow::Context, size: [u32; 2]) -> Result<(), String> {
        if self.ui.size == size {
            return Ok(());
        }
        unsafe {
            let ui = Target::new(gl, size, glow::RGBA8)?;
            let video = Target::new(gl, size, glow::RGBA16F)?;
            self.ui.destroy(gl);
            self.video.destroy(gl);
            self.ui = ui;
            self.video = video;
        }
        Ok(())
    }

    /// Into `target`, which is `self.ui`'s size. `flip` turns the picture
    /// upside down, for a Direct3D texture, whose first row is the top.
    pub unsafe fn composite(&self, gl: &glow::Context, target: glow::Framebuffer, sdr_white_nits: f32, flip: bool) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target));
            gl.viewport(0, 0, self.ui.size[0] as i32, self.ui.size[1] as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.use_program(Some(self.program));
            gl.uniform_1_f32(Some(&self.flip), if flip { -1.0 } else { 1.0 });
            gl.uniform_1_f32(Some(&self.sdr_white), sdr_white_nits);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.ui.tex));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.video.tex));
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.use_program(None);
        }
    }

    pub unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            self.ui.destroy(gl);
            self.video.destroy(gl);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

/// Everything HDR frames need, made the first time one is drawn.
pub struct Output {
    compositor: Compositor,
    presenter: Presenter,
}

impl Output {
    pub fn new(gl: &glow::Context, get_proc: &crate::mpv::ProcAddress, size: [u32; 2]) -> Result<Self, String> {
        let presenter = Presenter::new(gl, get_proc)?;
        match unsafe { Compositor::new(gl, size) } {
            Ok(compositor) => Ok(Output { compositor, presenter }),
            Err(e) => {
                presenter.destroy(gl);
                Err(e)
            }
        }
    }

    /// Starts a frame of `size` pixels: from here on egui draws into the UI
    /// buffer rather than the window.
    pub unsafe fn begin(&mut self, gl: &glow::Context, size: [u32; 2]) -> Result<(), String> {
        unsafe {
            self.compositor.resize(gl, size)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.compositor.ui.fbo));
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        Ok(())
    }

    /// Where mpv draws the video: framebuffer, and its GL format.
    pub fn video_target(&self) -> (i32, i32) {
        (self.compositor.video.fbo.0.get() as i32, self.compositor.video.format as i32)
    }

    /// Puts the video and the UI together and shows them.
    pub unsafe fn present(&mut self, gl: &glow::Context, display: Display) -> Result<(), String> {
        let size = self.compositor.ui.size;
        let target = self.presenter.begin(gl, size)?;
        unsafe {
            // Direct3D's first row is the top; GL's is the bottom.
            self.compositor.composite(gl, target, display.sdr_white_nits, true);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        self.presenter.present(gl)
    }

    /// Lets the window's own SDR surface show again.
    pub fn hide(&mut self) {
        self.presenter.hide();
    }

    pub fn destroy(self, gl: &glow::Context) {
        self.presenter.destroy(gl);
        unsafe { self.compositor.destroy(gl) };
    }
}

/// HDR output unless `OPEN_PREVIEW_HDR=off`. `force` treats every screen as
/// a 1000-nit HDR one, to try the path where Windows reports none.
pub fn wanted() -> Option<Display> {
    match std::env::var("OPEN_PREVIEW_HDR").as_deref() {
        Ok("off") => None,
        Ok("force") => Some(display().unwrap_or(Display { peak_nits: 1000.0, sdr_white_nits: 203.0 })),
        _ => display(),
    }
}
