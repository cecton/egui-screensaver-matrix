//! Shared GL helpers used by every pass: shader compilation, offscreen
//! render targets, ping-pong buffer pairs, and the fullscreen-triangle
//! geometry every pass draws with.

use glow::HasContext as _;

pub unsafe fn compile_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(shader_type)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if gl.get_shader_compile_status(shader) {
            Ok(shader)
        } else {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            Err(log)
        }
    }
}

pub unsafe fn link_program(
    gl: &glow::Context,
    shaders: &[glow::Shader],
) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        for &shader in shaders {
            gl.attach_shader(program, shader);
        }
        gl.link_program(program);
        let result = if gl.get_program_link_status(program) {
            Ok(program)
        } else {
            Err(gl.get_program_info_log(program))
        };
        for &shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        result
    }
}

/// Compiles a vertex+fragment pair with the shared cross-platform GLSL
/// version header (desktop-core vs WebGL1/GLES2 vs WebGL2/GLES3), the same
/// technique `egui-screensaver-flame` uses.
pub unsafe fn compile_program(
    gl: &glow::Context,
    header: &str,
    vertex_src: &str,
    fragment_src: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let vertex = compile_shader(gl, glow::VERTEX_SHADER, &format!("{header}{vertex_src}"))?;
        let fragment = match compile_shader(
            gl,
            glow::FRAGMENT_SHADER,
            &format!("{header}{fragment_src}"),
        ) {
            Ok(fragment) => fragment,
            Err(err) => {
                gl.delete_shader(vertex);
                return Err(err);
            }
        };
        link_program(gl, &[vertex, fragment])
    }
}

/// Builds the `#version ...` + `#define NEW_SHADER_INTERFACE 0/1` header
/// every shader in this crate is compiled with.
pub fn shader_header(gl: &glow::Context) -> String {
    let shader_version = egui_glow::ShaderVersion::get(gl);
    let new_interface = shader_version.is_new_shader_interface() as i32;
    format!(
        "{}\n#define NEW_SHADER_INTERFACE {new_interface}\n",
        shader_version.version_declaration()
    )
}

fn f32_slice_as_bytes(floats: &[f32]) -> &[u8] {
    // SAFETY: `f32` has no padding/invalid bit patterns, and the resulting
    // slice's lifetime and length are derived directly from `floats`.
    unsafe {
        std::slice::from_raw_parts(floats.as_ptr().cast::<u8>(), std::mem::size_of_val(floats))
    }
}

/// A single triangle covering the whole clip-space square and beyond
/// (vertices at (-1,-1), (3,-1), (-1,3)); the parts outside `[-1,1]` are
/// clipped away, leaving a fullscreen quad with no extra draw call. Shared
/// by every fullscreen pass (compute passes, and — from a later stage —
/// bloom/effect passes).
pub struct FullscreenTriangle {
    pub vao: glow::VertexArray,
    vbo: glow::Buffer,
}

impl FullscreenTriangle {
    pub unsafe fn new(gl: &glow::Context, program: glow::Program) -> Result<Self, String> {
        unsafe {
            let vertices: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                f32_slice_as_bytes(&vertices),
                glow::STATIC_DRAW,
            );

            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            if let Some(a_pos) = gl.get_attrib_location(program, "a_pos") {
                gl.enable_vertex_attrib_array(a_pos);
                gl.vertex_attrib_pointer_f32(a_pos, 2, glow::FLOAT, false, 0, 0);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            Ok(Self { vao, vbo })
        }
    }

    pub unsafe fn draw(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
        }
    }
}

/// Internal pixel format for an offscreen [`RenderTarget`]. The rain
/// "compute" buffers need [`TargetFormat::Rgba16F`] (real fractional
/// values, not just 0..255) to hold brightness/age/decay state; falls back
/// to [`TargetFormat::Rgba8`] if the platform can't render to a half-float
/// target (see the doc comment on [`RenderTarget::new`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetFormat {
    Rgba16F,
    Rgba8,
}

/// Texture filtering for a [`RenderTarget`]. The rain "compute" buffers
/// need [`TargetFilter::Nearest`] (their texel values are exact indices/
/// booleans that must not be interpolated); the bloom pyramid and the
/// screen-resolution primary/combine targets need
/// [`TargetFilter::Linear`] so up/downsampling between differently-sized
/// pyramid levels looks smooth rather than blocky.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetFilter {
    Nearest,
    Linear,
}

/// A texture + framebuffer pair that can be rendered into and then sampled
/// from as a regular texture in a later pass.
pub struct RenderTarget {
    pub texture: glow::Texture,
    pub framebuffer: glow::Framebuffer,
    pub width: i32,
    pub height: i32,
    pub format: TargetFormat,
    pub filter: TargetFilter,
}

impl RenderTarget {
    /// Creates a render target, falling back from `Rgba16F` to `Rgba8` if
    /// the platform can't render to a half-float attachment: WebGL2 makes
    /// the `RGBA16F` texture *format* core, but *rendering to* it needs the
    /// separate `EXT_color_buffer_float`/half-float extension, which isn't
    /// guaranteed on every implementation. Desktop GL 3.0+ has no such gap.
    pub unsafe fn new(
        gl: &glow::Context,
        width: i32,
        height: i32,
        format: TargetFormat,
        filter: TargetFilter,
    ) -> Result<Self, String> {
        unsafe {
            match Self::new_exact(gl, width, height, format, filter) {
                Ok(target) => Ok(target),
                Err(err) if format == TargetFormat::Rgba16F => {
                    log::warn!(
                        "egui-screensaver-matrix: RGBA16F render target unsupported \
                         ({err}), falling back to RGBA8 (reduced precision)"
                    );
                    Self::new_exact(gl, width, height, TargetFormat::Rgba8, filter)
                }
                Err(err) => Err(err),
            }
        }
    }

    unsafe fn new_exact(
        gl: &glow::Context,
        width: i32,
        height: i32,
        format: TargetFormat,
        filter: TargetFilter,
    ) -> Result<Self, String> {
        unsafe {
            let gl_filter = match filter {
                TargetFilter::Nearest => glow::NEAREST as i32,
                TargetFilter::Linear => glow::LINEAR as i32,
            };
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, gl_filter);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, gl_filter);

            let (internal_format, data_type) = match format {
                TargetFormat::Rgba16F => (glow::RGBA16F as i32, glow::HALF_FLOAT),
                TargetFormat::Rgba8 => (glow::RGBA8 as i32, glow::UNSIGNED_BYTE),
            };
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal_format,
                width,
                height,
                0,
                glow::RGBA,
                data_type,
                glow::PixelUnpackData::Slice(None),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);

            let framebuffer = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            if status != glow::FRAMEBUFFER_COMPLETE {
                gl.delete_texture(texture);
                gl.delete_framebuffer(framebuffer);
                return Err(format!("framebuffer incomplete (status 0x{status:x})"));
            }

            Ok(Self {
                texture,
                framebuffer,
                width,
                height,
                format,
                filter,
            })
        }
    }

    /// Recreates this target at a new size if it differs from the current
    /// one. Transient contents are lost (matches the original's own
    /// resize behavior, which recreates its compute buffers from scratch).
    pub unsafe fn resize(
        &mut self,
        gl: &glow::Context,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        unsafe {
            if self.width == width && self.height == height {
                return Ok(());
            }
            let new = Self::new(gl, width, height, self.format, self.filter)?;
            self.destroy(gl);
            *self = new;
            Ok(())
        }
    }

    pub unsafe fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.viewport(0, 0, self.width, self.height);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.texture);
            gl.delete_framebuffer(self.framebuffer);
        }
    }
}

/// A ping-ponged pair of [`RenderTarget`]s for a value that's computed each
/// frame from its own previous frame's output (the rain simulation's
/// "compute" buffers all work this way). `write()` is where this frame's
/// pass draws into — and, since it happens before [`PingPong::swap`], is
/// also readable by *later* passes in the same frame that need this
/// frame's freshly computed state (matching the original's read/write
/// ordering: e.g. the symbol pass reads the raindrop pass's output from
/// the same tick). `read()` is last frame's output, fed in as
/// `previousXState`. Call [`PingPong::swap`] once per frame after the
/// pass has drawn into `write()`.
pub struct PingPong {
    a: RenderTarget,
    b: RenderTarget,
    write_is_a: bool,
}

impl PingPong {
    pub unsafe fn new(
        gl: &glow::Context,
        width: i32,
        height: i32,
        format: TargetFormat,
        filter: TargetFilter,
    ) -> Result<Self, String> {
        unsafe {
            let a = RenderTarget::new(gl, width, height, format, filter)?;
            let b = match RenderTarget::new(gl, width, height, format, filter) {
                Ok(b) => b,
                Err(err) => {
                    a.destroy(gl);
                    return Err(err);
                }
            };
            Ok(Self {
                a,
                b,
                write_is_a: true,
            })
        }
    }

    pub fn write(&self) -> &RenderTarget {
        if self.write_is_a { &self.a } else { &self.b }
    }

    pub fn read(&self) -> &RenderTarget {
        if self.write_is_a { &self.b } else { &self.a }
    }

    pub fn swap(&mut self) {
        self.write_is_a = !self.write_is_a;
    }

    /// Resizes both buffers, matching the original's behavior of losing
    /// transient compute state on a grid-resolution change.
    pub unsafe fn resize(
        &mut self,
        gl: &glow::Context,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        unsafe {
            self.a.resize(gl, width, height)?;
            self.b.resize(gl, width, height)?;
            Ok(())
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.a.destroy(gl);
            self.b.destroy(gl);
        }
    }
}

/// Uploads a PNG's bytes as an `RGBA8` 2D texture with linear filtering
/// (used for MSDF glyph atlases and decorative textures). Flips the image
/// vertically before upload: the original's texture-coordinate math (e.g.
/// `getSymbolUV`'s row flip in render.frag.glsl) was written assuming
/// WebGL's default `UNPACK_FLIP_Y_WEBGL`-style upload, where texture v=0
/// ends up at the *bottom* of the source image. Desktop GL's
/// `tex_image_2d` has no such default — feeding it the same top-to-bottom
/// row order as the file would silently invert which half of the atlas
/// each glyph index resolves to. This is invisible for glyph sets that
/// fill nearly their whole grid (e.g. `matrixcode`), since almost every
/// row has *some* glyph either way, but is fatal for sparser fonts (e.g.
/// `gothic`, `coptic`), whose real glyphs sit only in the top rows and
/// render as fully blank without this flip.
pub unsafe fn upload_png_texture(
    gl: &glow::Context,
    png_bytes: &[u8],
) -> Result<glow::Texture, String> {
    let img = image::load_from_memory(png_bytes)
        .map_err(|err| format!("failed to decode bundled PNG: {err}"))?
        .flipv()
        .to_rgba8();
    let (width, height) = img.dimensions();
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(img.as_raw())),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
}
