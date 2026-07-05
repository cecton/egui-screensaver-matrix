//! The bloom pass: a classic 5-level FBO pyramid (highpass -> horizontal
//! blur -> vertical blur per level, each level half the resolution of the
//! previous) flattened into one bloom texture, ported from
//! `js/regl/bloomPass.js` / `shaders/glsl/bloomPass.*.frag.glsl` at
//! https://github.com/Rezmason/matrix (MIT licensed).

use std::sync::Arc;

use glow::HasContext as _;

use crate::config::MatrixConfig;
use crate::gl_util::{self, FullscreenTriangle, RenderTarget, TargetFilter, TargetFormat};

const FULLSCREEN_VERT_SRC: &str = include_str!("../shaders/fullscreen.vert.glsl");
const HIGHPASS_FRAG_SRC: &str = include_str!("../shaders/bloom_highpass.frag.glsl");
const BLUR_FRAG_SRC: &str = include_str!("../shaders/bloom_blur.frag.glsl");
const COMBINE_FRAG_SRC: &str = include_str!("../shaders/bloom_combine.frag.glsl");

const PYRAMID_HEIGHT: usize = 5;

struct HighpassUniforms {
    tex: Option<glow::UniformLocation>,
    high_pass_threshold: Option<glow::UniformLocation>,
}

struct BlurUniforms {
    tex: Option<glow::UniformLocation>,
    direction: Option<glow::UniformLocation>,
    width: Option<glow::UniformLocation>,
    height: Option<glow::UniformLocation>,
}

struct CombineUniforms {
    pyramid: [Option<glow::UniformLocation>; PYRAMID_HEIGHT],
    bloom_strength: Option<glow::UniformLocation>,
}

/// One pyramid level's 3 render targets: highpass -> horizontal blur ->
/// vertical blur, each at the same (progressively smaller) resolution.
struct PyramidLevel {
    highpass: RenderTarget,
    h_blur: RenderTarget,
    v_blur: RenderTarget,
}

impl PyramidLevel {
    unsafe fn new(gl: &glow::Context, width: i32, height: i32) -> Result<Self, String> {
        unsafe {
            let highpass =
                RenderTarget::new(gl, width, height, TargetFormat::Rgba8, TargetFilter::Linear)?;
            let h_blur = match RenderTarget::new(
                gl,
                width,
                height,
                TargetFormat::Rgba8,
                TargetFilter::Linear,
            ) {
                Ok(t) => t,
                Err(err) => {
                    highpass.destroy(gl);
                    return Err(err);
                }
            };
            let v_blur = match RenderTarget::new(
                gl,
                width,
                height,
                TargetFormat::Rgba8,
                TargetFilter::Linear,
            ) {
                Ok(t) => t,
                Err(err) => {
                    highpass.destroy(gl);
                    h_blur.destroy(gl);
                    return Err(err);
                }
            };
            Ok(Self {
                highpass,
                h_blur,
                v_blur,
            })
        }
    }

    unsafe fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
        unsafe {
            self.highpass.resize(gl, width, height)?;
            self.h_blur.resize(gl, width, height)?;
            self.v_blur.resize(gl, width, height)?;
            Ok(())
        }
    }

    unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.highpass.destroy(gl);
            self.h_blur.destroy(gl);
            self.v_blur.destroy(gl);
        }
    }
}

/// Level `i`'s resolution: `floor(screen_size * bloom_size / 2^i)`,
/// clamped to at least 1 pixel so a very small `bloom_size` can't produce
/// an invalid 0x0 render target.
fn level_size(screen_size: i32, bloom_size: f32, level: usize) -> i32 {
    ((screen_size as f32 * bloom_size) as i32 >> level).max(1)
}

pub struct BloomPasses {
    highpass_program: glow::Program,
    highpass_triangle: FullscreenTriangle,
    highpass_uniforms: HighpassUniforms,

    blur_program: glow::Program,
    blur_triangle: FullscreenTriangle,
    blur_uniforms: BlurUniforms,

    combine_program: glow::Program,
    combine_triangle: FullscreenTriangle,
    combine_uniforms: CombineUniforms,
    combine_output: RenderTarget,

    levels: [PyramidLevel; PYRAMID_HEIGHT],
    screen_width: i32,
    screen_height: i32,
    bloom_size: f32,
}

impl BloomPasses {
    pub unsafe fn new(
        gl: &Arc<glow::Context>,
        header: &str,
        screen_width: i32,
        screen_height: i32,
        bloom_size: f32,
    ) -> Result<Self, String> {
        unsafe {
            let highpass_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, HIGHPASS_FRAG_SRC)?;
            let highpass_triangle = FullscreenTriangle::new(gl, highpass_program)?;
            let highpass_uniforms = HighpassUniforms {
                tex: gl.get_uniform_location(highpass_program, "tex"),
                high_pass_threshold: gl.get_uniform_location(highpass_program, "highPassThreshold"),
            };

            let blur_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, BLUR_FRAG_SRC)?;
            let blur_triangle = FullscreenTriangle::new(gl, blur_program)?;
            let blur_uniforms = BlurUniforms {
                tex: gl.get_uniform_location(blur_program, "tex"),
                direction: gl.get_uniform_location(blur_program, "direction"),
                width: gl.get_uniform_location(blur_program, "width"),
                height: gl.get_uniform_location(blur_program, "height"),
            };

            let combine_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, COMBINE_FRAG_SRC)?;
            let combine_triangle = FullscreenTriangle::new(gl, combine_program)?;
            let combine_uniforms = CombineUniforms {
                pyramid: [
                    gl.get_uniform_location(combine_program, "pyr_0"),
                    gl.get_uniform_location(combine_program, "pyr_1"),
                    gl.get_uniform_location(combine_program, "pyr_2"),
                    gl.get_uniform_location(combine_program, "pyr_3"),
                    gl.get_uniform_location(combine_program, "pyr_4"),
                ],
                bloom_strength: gl.get_uniform_location(combine_program, "bloomStrength"),
            };
            let combine_output = RenderTarget::new(
                gl,
                screen_width,
                screen_height,
                TargetFormat::Rgba8,
                TargetFilter::Linear,
            )?;

            let mut level_targets = Vec::with_capacity(PYRAMID_HEIGHT);
            for i in 0..PYRAMID_HEIGHT {
                let w = level_size(screen_width, bloom_size, i);
                let h = level_size(screen_height, bloom_size, i);
                level_targets.push(PyramidLevel::new(gl, w, h)?);
            }
            let levels: [PyramidLevel; PYRAMID_HEIGHT] = level_targets
                .try_into()
                .unwrap_or_else(|_| unreachable!("exactly PYRAMID_HEIGHT levels pushed"));

            Ok(Self {
                highpass_program,
                highpass_triangle,
                highpass_uniforms,
                blur_program,
                blur_triangle,
                blur_uniforms,
                combine_program,
                combine_triangle,
                combine_uniforms,
                combine_output,
                levels,
                screen_width,
                screen_height,
                bloom_size,
            })
        }
    }

    pub unsafe fn resize(
        &mut self,
        gl: &glow::Context,
        screen_width: i32,
        screen_height: i32,
        bloom_size: f32,
    ) -> Result<(), String> {
        unsafe {
            if screen_width == self.screen_width
                && screen_height == self.screen_height
                && bloom_size == self.bloom_size
            {
                return Ok(());
            }
            self.combine_output
                .resize(gl, screen_width, screen_height)?;
            for (i, level) in self.levels.iter_mut().enumerate() {
                let w = level_size(screen_width, bloom_size, i);
                let h = level_size(screen_height, bloom_size, i);
                level.resize(gl, w, h)?;
            }
            self.screen_width = screen_width;
            self.screen_height = screen_height;
            self.bloom_size = bloom_size;
            Ok(())
        }
    }

    /// Runs the full highpass -> blur -> blur -> combine pyramid, reading
    /// `primary_texture` (this frame's rain render) as input, and returns
    /// the resulting bloom texture. Leaves `GL_SCISSOR_TEST` disabled (see
    /// the note in `passes/compute.rs`).
    pub unsafe fn run(
        &self,
        gl: &glow::Context,
        config: &MatrixConfig,
        primary_texture: glow::Texture,
    ) -> glow::Texture {
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);

            gl.use_program(Some(self.highpass_program));
            gl.uniform_1_i32(self.highpass_uniforms.tex.as_ref(), 0);
            gl.uniform_1_f32(
                self.highpass_uniforms.high_pass_threshold.as_ref(),
                config.high_pass_threshold,
            );

            gl.use_program(Some(self.blur_program));
            gl.uniform_1_i32(self.blur_uniforms.tex.as_ref(), 0);

            for (i, level) in self.levels.iter().enumerate() {
                let input = if i == 0 {
                    primary_texture
                } else {
                    self.levels[i - 1].highpass.texture
                };

                gl.use_program(Some(self.highpass_program));
                level.highpass.bind(gl);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(input));
                self.highpass_triangle.draw(gl);

                gl.use_program(Some(self.blur_program));
                gl.uniform_1_f32(
                    self.blur_uniforms.width.as_ref(),
                    level.highpass.width as f32,
                );
                gl.uniform_1_f32(
                    self.blur_uniforms.height.as_ref(),
                    level.highpass.height as f32,
                );

                level.h_blur.bind(gl);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(level.highpass.texture));
                gl.uniform_2_f32(self.blur_uniforms.direction.as_ref(), 1.0, 0.0);
                self.blur_triangle.draw(gl);

                level.v_blur.bind(gl);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(level.h_blur.texture));
                gl.uniform_2_f32(self.blur_uniforms.direction.as_ref(), 0.0, 1.0);
                self.blur_triangle.draw(gl);
            }

            self.combine_output.bind(gl);
            gl.use_program(Some(self.combine_program));
            for (i, loc) in self.combine_uniforms.pyramid.iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + i as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.levels[i].v_blur.texture));
                gl.uniform_1_i32(loc.as_ref(), i as i32);
            }
            gl.uniform_1_f32(
                self.combine_uniforms.bloom_strength.as_ref(),
                config.bloom_strength,
            );
            self.combine_triangle.draw(gl);

            gl.bind_texture(glow::TEXTURE_2D, None);
            self.combine_output.texture
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            for level in &self.levels {
                level.destroy(gl);
            }
            self.combine_output.destroy(gl);
            self.highpass_triangle.destroy(gl);
            self.blur_triangle.destroy(gl);
            self.combine_triangle.destroy(gl);
            gl.delete_program(self.highpass_program);
            gl.delete_program(self.blur_program);
            gl.delete_program(self.combine_program);
        }
    }
}
