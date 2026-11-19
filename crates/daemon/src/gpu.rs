//! EGL + OpenGL ES: one context shared by every overlay.
//!
//! Smooth mode draws the batch straight to the window. Pixel mode draws it into a small offscreen texture
//! (one texel per art pixel), then stretches that to the window with hard edges and a reduced palette.

use glow::HasContext;
use khronos_egl as egl;
use std::ffi::c_void;

type Egl = egl::DynamicInstance<egl::EGL1_4>;

const VERT: &str = "#version 100
attribute vec2 a_pos; attribute vec2 a_uv; attribute vec4 a_col;
uniform vec2 u_view;
varying vec2 v_uv; varying vec4 v_col;
void main() {
    gl_Position = vec4(a_pos.x / u_view.x * 2.0 - 1.0, 1.0 - a_pos.y / u_view.y * 2.0, 0.0, 1.0);
    v_uv = a_uv; v_col = a_col;
}";

const FRAG: &str = "#version 100
precision mediump float;
varying vec2 v_uv; varying vec4 v_col;
uniform float u_soft; uniform float u_opacity;
void main() {
    float d = length(v_uv);
    float a = u_soft > 0.5 ? 1.0 - smoothstep(0.6, 1.0, d) : step(d, 1.0);
    a *= v_col.a * u_opacity;
    gl_FragColor = vec4(v_col.rgb * a, a);
}";

const BLIT_VERT: &str = "#version 100
attribute vec2 a_pos;
void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }";

const BLIT_FRAG: &str = "#version 100
precision mediump float;
uniform sampler2D u_tex; uniform vec2 u_buf; uniform vec2 u_texsize; uniform float u_levels; uniform float u_opacity;
void main() {
    vec2 uv = vec2(gl_FragCoord.x / u_texsize.x, 1.0 - (u_buf.y - gl_FragCoord.y) / u_texsize.y);
    vec4 t = texture2D(u_tex, uv);
    if (t.a < 0.08) { gl_FragColor = vec4(0.0); return; }
    vec3 c = clamp(t.rgb / t.a, 0.0, 1.0);
    c = floor(c * u_levels + 0.5) / u_levels;
    // Pixel art keeps a few steps of transparency (a quarter, half, three quarters, solid): soft rims stay see-through.
    float a = max(floor(t.a * 4.0 + 0.5) / 4.0, 0.25) * u_opacity;
    gl_FragColor = vec4(c * a, a);
}";

pub struct Fbo {
    tex: glow::Texture,
    fbo: glow::Framebuffer,
    pub w: i32,
    pub h: i32,
}

/// Per-overlay GL state.
pub struct Target {
    pub surface: egl::Surface,
    pub fbo: Option<Fbo>,
}

pub struct Opts {
    pub buf: (i32, i32),
    pub logical: (f32, f32),
    pub scale: f32,
    pub opacity: f32,
    /// `Some((art pixel size in logical px, colour levels))` for the pixel renderer.
    pub pixel: Option<(u32, u32)>,
    /// Rectangles (x, y, w, h in buffer pixels, y down) left empty, so a bar can be photographed bare.
    pub clear: Vec<[i32; 4]>,
}

struct Prog {
    p: glow::Program,
}

pub struct Gpu {
    egl: Egl,
    pub display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    gl: glow::Context,
    main: Prog,
    blit: Prog,
    vbo: glow::Buffer,
    quad: glow::Buffer,
    pub renderer: String,
}

impl Gpu {
    pub fn new(wl_display: *mut c_void) -> Result<Gpu, String> {
        unsafe {
            let egl = Egl::load_required().map_err(|e| format!("libEGL: {e}"))?;
            let display = egl.get_display(wl_display as egl::NativeDisplayType).ok_or("no EGL display")?;
            egl.initialize(display).map_err(|e| format!("eglInitialize: {e}"))?;
            egl.bind_api(egl::OPENGL_ES_API).map_err(|e| format!("bind api: {e}"))?;
            let attribs = [
                egl::SURFACE_TYPE, egl::WINDOW_BIT,
                egl::RENDERABLE_TYPE, egl::OPENGL_ES2_BIT,
                egl::RED_SIZE, 8, egl::GREEN_SIZE, 8, egl::BLUE_SIZE, 8, egl::ALPHA_SIZE, 8,
                egl::NONE,
            ];
            let config = egl.choose_first_config(display, &attribs).map_err(|e| e.to_string())?.ok_or("no EGL config with alpha")?;
            let context = egl.create_context(display, config, None, &[egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE]).map_err(|e| format!("context: {e}"))?;
            egl.make_current(display, None, None, Some(context)).map_err(|e| format!("surfaceless make_current: {e}"))?;
            let gl = glow::Context::from_loader_function(|name| egl.get_proc_address(name).map_or(std::ptr::null(), |p| p as *const c_void));
            let renderer = gl.get_parameter_string(glow::RENDERER);
            let main = Self::program(&gl, VERT, FRAG)?;
            let blit = Self::program(&gl, BLIT_VERT, BLIT_FRAG)?;
            let vbo = gl.create_buffer()?;
            let quad = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
            let verts: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&verts), glow::STATIC_DRAW);
            Ok(Gpu { egl, display, config, context, gl, main, blit, vbo, quad, renderer })
        }
    }

    unsafe fn program(gl: &glow::Context, vs: &str, fs: &str) -> Result<Prog, String> {
        let p = gl.create_program()?;
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let s = gl.create_shader(kind)?;
            gl.shader_source(s, src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                return Err(format!("shader: {}", gl.get_shader_info_log(s)));
            }
            gl.attach_shader(p, s);
            gl.delete_shader(s);
        }
        gl.bind_attrib_location(p, 0, "a_pos");
        gl.bind_attrib_location(p, 1, "a_uv");
        gl.bind_attrib_location(p, 2, "a_col");
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            return Err(format!("link: {}", gl.get_program_info_log(p)));
        }
        Ok(Prog { p })
    }

    pub fn create_surface(&self, native_window: *mut c_void) -> Result<Target, String> {
        let surface = unsafe { self.egl.create_window_surface(self.display, self.config, native_window as egl::NativeWindowType, None) }.map_err(|e| format!("window surface: {e}"))?;
        Ok(Target { surface, fbo: None })
    }

    pub fn destroy(&self, t: Target) {
        self.make_current(Some(t.surface));
        if let Some(f) = t.fbo {
            unsafe {
                self.gl.delete_framebuffer(f.fbo);
                self.gl.delete_texture(f.tex);
            }
        }
        let _ = self.egl.make_current(self.display, None, None, Some(self.context));
        let _ = self.egl.destroy_surface(self.display, t.surface);
    }

    fn make_current(&self, s: Option<egl::Surface>) {
        let _ = self.egl.make_current(self.display, s, s, Some(self.context));
    }

    fn ensure_fbo(&self, t: &mut Target, w: i32, h: i32) {
        if t.fbo.as_ref().is_some_and(|f| f.w == w && f.h == h) {
            return;
        }
        unsafe {
            let gl = &self.gl;
            if let Some(f) = t.fbo.take() {
                gl.delete_framebuffer(f.fbo);
                gl.delete_texture(f.tex);
            }
            let tex = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
            for (k, v) in [(glow::TEXTURE_MIN_FILTER, glow::NEAREST), (glow::TEXTURE_MAG_FILTER, glow::NEAREST), (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE), (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE)] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            t.fbo = Some(Fbo { tex, fbo, w, h });
        }
    }

    /// Draws one frame into the window and presents it.
    pub fn draw(&self, t: &mut Target, batch: &[f32], o: &Opts) {
        self.make_current(Some(t.surface));
        let _ = self.egl.swap_interval(self.display, 0);
        unsafe {
            let gl = &self.gl;
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            match o.pixel {
                None => {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    gl.viewport(0, 0, o.buf.0, o.buf.1);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                    self.geometry(batch, o.logical, true, o.opacity);
                }
                Some((size, levels)) => {
                    let n_dev = size as f32 * o.scale;
                    let (pw, ph) = ((o.buf.0 as f32 / n_dev).ceil() as i32, (o.buf.1 as f32 / n_dev).ceil() as i32);
                    self.ensure_fbo(t, pw, ph);
                    let f = t.fbo.as_ref().unwrap();
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f.fbo));
                    gl.viewport(0, 0, pw, ph);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                    self.geometry(batch, (pw as f32 * size as f32, ph as f32 * size as f32), false, 1.0);
                    // Stretch it to the window, nearest neighbour.
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    gl.viewport(0, 0, o.buf.0, o.buf.1);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                    gl.use_program(Some(self.blit.p));
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(f.tex));
                    let loc = |n: &str| gl.get_uniform_location(self.blit.p, n);
                    gl.uniform_1_i32(loc("u_tex").as_ref(), 0);
                    gl.uniform_2_f32(loc("u_buf").as_ref(), o.buf.0 as f32, o.buf.1 as f32);
                    gl.uniform_2_f32(loc("u_texsize").as_ref(), pw as f32 * n_dev, ph as f32 * n_dev);
                    gl.uniform_1_f32(loc("u_levels").as_ref(), (levels.max(2) - 1) as f32);
                    gl.uniform_1_f32(loc("u_opacity").as_ref(), o.opacity);
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad));
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
                    gl.disable_vertex_attrib_array(1);
                    gl.disable_vertex_attrib_array(2);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
            }
            if !o.clear.is_empty() {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.enable(glow::SCISSOR_TEST);
                for [x, y, w, h] in &o.clear {
                    gl.scissor(*x, o.buf.1 - y - h, *w, *h);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }
                gl.disable(glow::SCISSOR_TEST);
            }
            gl.flush();
        }
        let _ = self.egl.swap_buffers(self.display, t.surface);
    }

    unsafe fn geometry(&self, batch: &[f32], view: (f32, f32), soft: bool, opacity: f32) {
        let gl = &self.gl;
        if batch.is_empty() {
            return;
        }
        gl.use_program(Some(self.main.p));
        let loc = |n: &str| gl.get_uniform_location(self.main.p, n);
        gl.uniform_2_f32(loc("u_view").as_ref(), view.0, view.1);
        gl.uniform_1_f32(loc("u_soft").as_ref(), if soft { 1.0 } else { 0.0 });
        gl.uniform_1_f32(loc("u_opacity").as_ref(), opacity);
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(batch), glow::STREAM_DRAW);
        for (i, (size, off)) in [(2, 0), (2, 8), (4, 16)].into_iter().enumerate() {
            gl.enable_vertex_attrib_array(i as u32);
            gl.vertex_attrib_pointer_f32(i as u32, size, glow::FLOAT, false, 32, off);
        }
        gl.draw_arrays(glow::TRIANGLES, 0, (batch.len() / 8) as i32);
    }
}

fn bytes(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
