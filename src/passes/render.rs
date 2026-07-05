//! The main render pass: samples the compute-state textures plus MSDF
//! glyph (and, for fonts that have one, glint) atlases and draws the
//! falling glyphs, additively, into whatever framebuffer is bound when
//! [`RenderPass::draw`] is called. Holds two compiled program variants —
//! a 2D fullscreen-quad path and a volumetric per-cell-quad-grid path —
//! selected per draw call by `config.volumetric` (see
//! `shaders/render.vert.glsl`'s header comment for why these are two
//! programs rather than one branching on a uniform).

use std::sync::Arc;

use glow::HasContext as _;

use crate::camera;
use crate::config::MatrixConfig;
use crate::fonts::FontAtlas;
use crate::geometry::QuadGrid;
use crate::gl_util::{self, FullscreenTriangle};
use crate::textures::DecorativeTexture;

const RENDER_VERT_SRC: &str = include_str!("../shaders/render.vert.glsl");
const RENDER_FRAG_SRC: &str = include_str!("../shaders/render.frag.glsl");

/// The signed distance range (in atlas texels) the bundled MSDF atlases
/// were generated with — matches the original's hardcoded `msdfPxRange`.
const MSDF_PX_RANGE: f32 = 4.0;

/// The original's `glyphVerticalSpacing` default, never overridden by any
/// bundled preset — not exposed as a `MatrixConfig` field since nothing
/// uses a different value, but still a real part of the vertex math.
const GLYPH_VERTICAL_SPACING: f32 = 1.0;

#[derive(Default)]
struct RenderUniforms {
    // Shared by both program variants.
    num_columns: Option<glow::UniformLocation>,
    num_rows: Option<glow::UniformLocation>,
    glyph_msdf: Option<glow::UniformLocation>,
    glint_msdf: Option<glow::UniformLocation>,
    base_texture: Option<glow::UniformLocation>,
    glint_texture: Option<glow::UniformLocation>,
    has_base_texture: Option<glow::UniformLocation>,
    has_glint_texture: Option<glow::UniformLocation>,
    msdf_px_range: Option<glow::UniformLocation>,
    glyph_msdf_size: Option<glow::UniformLocation>,
    glint_msdf_size: Option<glow::UniformLocation>,
    glyph_height_to_width: Option<glow::UniformLocation>,
    glyph_sequence_length: Option<glow::UniformLocation>,
    glyph_edge_crop: Option<glow::UniformLocation>,
    base_contrast: Option<glow::UniformLocation>,
    base_brightness: Option<glow::UniformLocation>,
    glint_contrast: Option<glow::UniformLocation>,
    glint_brightness: Option<glow::UniformLocation>,
    brightness_override: Option<glow::UniformLocation>,
    brightness_threshold: Option<glow::UniformLocation>,
    glyph_texture_grid_size: Option<glow::UniformLocation>,
    isolate_cursor: Option<glow::UniformLocation>,
    isolate_glint: Option<glow::UniformLocation>,
    glyph_transform: Option<glow::UniformLocation>,
    raindrop_state: Option<glow::UniformLocation>,
    symbol_state: Option<glow::UniformLocation>,
    effect_state: Option<glow::UniformLocation>,

    // 2D-only.
    screen_size: Option<glow::UniformLocation>,
    slant_vec: Option<glow::UniformLocation>,
    slant_scale: Option<glow::UniformLocation>,
    is_polar: Option<glow::UniformLocation>,

    // Volumetric-only.
    density: Option<glow::UniformLocation>,
    quad_size: Option<glow::UniformLocation>,
    glyph_vertical_spacing: Option<glow::UniformLocation>,
    view_projection: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    animation_speed: Option<glow::UniformLocation>,
    forward_speed: Option<glow::UniformLocation>,
}

unsafe fn query_uniforms(gl: &glow::Context, program: glow::Program) -> RenderUniforms {
    unsafe {
        RenderUniforms {
            num_columns: gl.get_uniform_location(program, "numColumns"),
            num_rows: gl.get_uniform_location(program, "numRows"),
            glyph_msdf: gl.get_uniform_location(program, "glyphMSDF"),
            glint_msdf: gl.get_uniform_location(program, "glintMSDF"),
            base_texture: gl.get_uniform_location(program, "baseTexture"),
            glint_texture: gl.get_uniform_location(program, "glintTexture"),
            has_base_texture: gl.get_uniform_location(program, "hasBaseTexture"),
            has_glint_texture: gl.get_uniform_location(program, "hasGlintTexture"),
            msdf_px_range: gl.get_uniform_location(program, "msdfPxRange"),
            glyph_msdf_size: gl.get_uniform_location(program, "glyphMSDFSize"),
            glint_msdf_size: gl.get_uniform_location(program, "glintMSDFSize"),
            glyph_height_to_width: gl.get_uniform_location(program, "glyphHeightToWidth"),
            glyph_sequence_length: gl.get_uniform_location(program, "glyphSequenceLength"),
            glyph_edge_crop: gl.get_uniform_location(program, "glyphEdgeCrop"),
            base_contrast: gl.get_uniform_location(program, "baseContrast"),
            base_brightness: gl.get_uniform_location(program, "baseBrightness"),
            glint_contrast: gl.get_uniform_location(program, "glintContrast"),
            glint_brightness: gl.get_uniform_location(program, "glintBrightness"),
            brightness_override: gl.get_uniform_location(program, "brightnessOverride"),
            brightness_threshold: gl.get_uniform_location(program, "brightnessThreshold"),
            glyph_texture_grid_size: gl.get_uniform_location(program, "glyphTextureGridSize"),
            isolate_cursor: gl.get_uniform_location(program, "isolateCursor"),
            isolate_glint: gl.get_uniform_location(program, "isolateGlint"),
            glyph_transform: gl.get_uniform_location(program, "glyphTransform"),
            raindrop_state: gl.get_uniform_location(program, "raindropState"),
            symbol_state: gl.get_uniform_location(program, "symbolState"),
            effect_state: gl.get_uniform_location(program, "effectState"),
            screen_size: gl.get_uniform_location(program, "screenSize"),
            slant_vec: gl.get_uniform_location(program, "slantVec"),
            slant_scale: gl.get_uniform_location(program, "slantScale"),
            is_polar: gl.get_uniform_location(program, "isPolar"),
            density: gl.get_uniform_location(program, "density"),
            quad_size: gl.get_uniform_location(program, "quadSize"),
            glyph_vertical_spacing: gl.get_uniform_location(program, "glyphVerticalSpacing"),
            view_projection: gl.get_uniform_location(program, "viewProjection"),
            time: gl.get_uniform_location(program, "time"),
            animation_speed: gl.get_uniform_location(program, "animationSpeed"),
            forward_speed: gl.get_uniform_location(program, "forwardSpeed"),
        }
    }
}

pub struct RenderPass {
    program_2d: glow::Program,
    triangle: FullscreenTriangle,
    uniforms_2d: RenderUniforms,

    program_volumetric: glow::Program,
    quad_grid: QuadGrid,
    uniforms_volumetric: RenderUniforms,
    quad_columns: u32,
    quad_rows: u32,

    glyph_msdf: glow::Texture,
    glint_msdf: Option<glow::Texture>,
    glyph_msdf_size: [f32; 2],
    glint_msdf_size: [f32; 2],
    grid_size: [f32; 2],
    glyph_sequence_length: f32,

    white_texture: glow::Texture,
    base_texture: Option<(DecorativeTexture, glow::Texture)>,
    glint_texture: Option<(DecorativeTexture, glow::Texture)>,
}

/// Column-major 2x2 matrix combining the glyph-flip and glyph-rotation
/// tunables, ported from the original's
/// `mat2.rotate(mat2.fromScaling(..., [flip ? -1 : 1, 1]), ..., rotationRadians)`.
fn glyph_transform(flip: bool, rotation_degrees: f32) -> [f32; 4] {
    let sx = if flip { -1.0 } else { 1.0 };
    let (s, c) = rotation_degrees.to_radians().sin_cos();
    [sx * c, s, -sx * s, c]
}

unsafe fn upload_white_texture(gl: &glow::Context) -> Result<glow::Texture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            1,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&[255, 255, 255, 255])),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
}

fn image_dimensions(png_bytes: &[u8]) -> Result<(u32, u32), String> {
    image::load_from_memory(png_bytes)
        .map(|img| image::GenericImageView::dimensions(&img))
        .map_err(|err| format!("failed to decode bundled PNG: {err}"))
}

impl RenderPass {
    pub unsafe fn new(
        gl: &Arc<glow::Context>,
        header: &str,
        font: &FontAtlas,
    ) -> Result<Self, String> {
        unsafe {
            let vert_2d = format!("{header}\n#define VOLUMETRIC 0\n");
            let program_2d =
                gl_util::compile_program(gl, &vert_2d, RENDER_VERT_SRC, RENDER_FRAG_SRC)?;
            let triangle = FullscreenTriangle::new(gl, program_2d)?;
            let uniforms_2d = query_uniforms(gl, program_2d);

            let vert_volumetric = format!("{header}\n#define VOLUMETRIC 1\n");
            let program_volumetric =
                gl_util::compile_program(gl, &vert_volumetric, RENDER_VERT_SRC, RENDER_FRAG_SRC)?;
            let quad_grid = QuadGrid::new(gl, program_volumetric, 1, 1)?;
            let uniforms_volumetric = query_uniforms(gl, program_volumetric);

            let glyph_msdf = gl_util::upload_png_texture(gl, font.png_bytes)?;
            let glyph_msdf_size = image_dimensions(font.png_bytes)?;
            let (glint_msdf, glint_msdf_size) = match font.glint_png_bytes {
                Some(bytes) => (
                    Some(gl_util::upload_png_texture(gl, bytes)?),
                    image_dimensions(bytes)?,
                ),
                None => (None, (1, 1)),
            };

            let white_texture = upload_white_texture(gl)?;

            Ok(Self {
                program_2d,
                triangle,
                uniforms_2d,
                program_volumetric,
                quad_grid,
                uniforms_volumetric,
                quad_columns: 1,
                quad_rows: 1,
                glyph_msdf,
                glint_msdf,
                glyph_msdf_size: [glyph_msdf_size.0 as f32, glyph_msdf_size.1 as f32],
                glint_msdf_size: [glint_msdf_size.0 as f32, glint_msdf_size.1 as f32],
                grid_size: [font.grid_cols as f32, font.grid_rows as f32],
                glyph_sequence_length: font.glyph_sequence_length as f32,
                white_texture,
                base_texture: None,
                glint_texture: None,
            })
        }
    }

    /// Swaps in a different font's glyph (and, if present, glint) atlas —
    /// the programs/uniforms stay the same across fonts, only the
    /// textures and their grid metadata change.
    pub unsafe fn set_font(&mut self, gl: &glow::Context, font: &FontAtlas) -> Result<(), String> {
        unsafe {
            let glyph_msdf = gl_util::upload_png_texture(gl, font.png_bytes)?;
            let glyph_msdf_size = image_dimensions(font.png_bytes)?;
            let (glint_msdf, glint_msdf_size) = match font.glint_png_bytes {
                Some(bytes) => (
                    Some(gl_util::upload_png_texture(gl, bytes)?),
                    image_dimensions(bytes)?,
                ),
                None => (None, (1, 1)),
            };

            gl.delete_texture(self.glyph_msdf);
            if let Some(old) = self.glint_msdf.take() {
                gl.delete_texture(old);
            }
            self.glyph_msdf = glyph_msdf;
            self.glint_msdf = glint_msdf;
            self.glyph_msdf_size = [glyph_msdf_size.0 as f32, glyph_msdf_size.1 as f32];
            self.glint_msdf_size = [glint_msdf_size.0 as f32, glint_msdf_size.1 as f32];
            self.grid_size = [font.grid_cols as f32, font.grid_rows as f32];
            self.glyph_sequence_length = font.glyph_sequence_length as f32;
            Ok(())
        }
    }

    unsafe fn sync_decorative_texture(
        gl: &glow::Context,
        slot: &mut Option<(DecorativeTexture, glow::Texture)>,
        wanted: Option<DecorativeTexture>,
    ) {
        unsafe {
            let current_id = slot.as_ref().map(|(id, _)| *id);
            if current_id == wanted {
                return;
            }
            if let Some((_, texture)) = slot.take() {
                gl.delete_texture(texture);
            }
            if let Some(id) = wanted {
                match gl_util::upload_png_texture(gl, id.png_bytes()) {
                    Ok(texture) => *slot = Some((id, texture)),
                    Err(err) => {
                        log::error!(
                            "egui-screensaver-matrix: failed to load decorative texture: {err}"
                        )
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn set_common_uniforms(
        &self,
        gl: &glow::Context,
        u: &RenderUniforms,
        config: &MatrixConfig,
        num_columns: u32,
        num_rows: u32,
        raindrop_state: glow::Texture,
        symbol_state: glow::Texture,
        effect_state: glow::Texture,
    ) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(raindrop_state));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(symbol_state));
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(effect_state));
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.glyph_msdf));
            gl.active_texture(glow::TEXTURE4);
            gl.bind_texture(
                glow::TEXTURE_2D,
                Some(self.glint_msdf.unwrap_or(self.white_texture)),
            );
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(
                glow::TEXTURE_2D,
                Some(
                    self.base_texture
                        .map(|(_, t)| t)
                        .unwrap_or(self.white_texture),
                ),
            );
            gl.active_texture(glow::TEXTURE6);
            gl.bind_texture(
                glow::TEXTURE_2D,
                Some(
                    self.glint_texture
                        .map(|(_, t)| t)
                        .unwrap_or(self.white_texture),
                ),
            );

            gl.uniform_1_i32(u.raindrop_state.as_ref(), 0);
            gl.uniform_1_i32(u.symbol_state.as_ref(), 1);
            gl.uniform_1_i32(u.effect_state.as_ref(), 2);
            gl.uniform_1_i32(u.glyph_msdf.as_ref(), 3);
            gl.uniform_1_i32(u.glint_msdf.as_ref(), 4);
            gl.uniform_1_i32(u.base_texture.as_ref(), 5);
            gl.uniform_1_i32(u.glint_texture.as_ref(), 6);

            gl.uniform_1_f32(u.num_columns.as_ref(), num_columns as f32);
            gl.uniform_1_f32(u.num_rows.as_ref(), num_rows as f32);
            gl.uniform_1_f32(u.msdf_px_range.as_ref(), MSDF_PX_RANGE);
            gl.uniform_2_f32(
                u.glyph_msdf_size.as_ref(),
                self.glyph_msdf_size[0],
                self.glyph_msdf_size[1],
            );
            gl.uniform_2_f32(
                u.glint_msdf_size.as_ref(),
                self.glint_msdf_size[0],
                self.glint_msdf_size[1],
            );
            gl.uniform_1_i32(
                u.has_base_texture.as_ref(),
                self.base_texture.is_some() as i32,
            );
            gl.uniform_1_i32(
                u.has_glint_texture.as_ref(),
                self.glint_texture.is_some() as i32,
            );
            gl.uniform_1_f32(
                u.glyph_height_to_width.as_ref(),
                config.glyph_height_to_width,
            );
            gl.uniform_1_f32(u.glyph_sequence_length.as_ref(), self.glyph_sequence_length);
            gl.uniform_1_f32(u.glyph_edge_crop.as_ref(), config.glyph_edge_crop);
            gl.uniform_1_f32(u.base_contrast.as_ref(), config.base_contrast);
            gl.uniform_1_f32(u.base_brightness.as_ref(), config.base_brightness);
            gl.uniform_1_f32(u.glint_contrast.as_ref(), config.glint_contrast);
            gl.uniform_1_f32(u.glint_brightness.as_ref(), config.glint_brightness);
            gl.uniform_1_f32(u.brightness_override.as_ref(), config.brightness_override);
            gl.uniform_1_f32(u.brightness_threshold.as_ref(), config.brightness_threshold);
            gl.uniform_2_f32(
                u.glyph_texture_grid_size.as_ref(),
                self.grid_size[0],
                self.grid_size[1],
            );
            gl.uniform_1_i32(u.isolate_cursor.as_ref(), config.isolate_cursor as i32);
            gl.uniform_1_i32(u.isolate_glint.as_ref(), config.isolate_glint as i32);
            if let Some(loc) = u.glyph_transform.as_ref() {
                let m = glyph_transform(config.glyph_flip, config.glyph_rotation);
                gl.uniform_matrix_2_f32_slice(Some(loc), false, &m);
            }
        }
    }

    /// Draws additively into whatever framebuffer is currently bound.
    /// Caller is responsible for having bound the intended target and
    /// viewport.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn draw(
        &mut self,
        gl: &glow::Context,
        config: &MatrixConfig,
        time: f32,
        num_columns: u32,
        num_rows: u32,
        raindrop_state: glow::Texture,
        symbol_state: glow::Texture,
        effect_state: glow::Texture,
        screen_width: i32,
        screen_height: i32,
    ) {
        unsafe {
            Self::sync_decorative_texture(gl, &mut self.base_texture, config.base_texture);
            Self::sync_decorative_texture(gl, &mut self.glint_texture, config.glint_texture);

            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE);

            if config.volumetric {
                if num_columns != self.quad_columns || num_rows != self.quad_rows {
                    if let Err(err) =
                        self.quad_grid
                            .resize(gl, self.program_volumetric, num_columns, num_rows)
                    {
                        log::error!("egui-screensaver-matrix: failed to resize quad grid: {err}");
                    } else {
                        self.quad_columns = num_columns;
                        self.quad_rows = num_rows;
                    }
                }

                gl.use_program(Some(self.program_volumetric));
                let u = &self.uniforms_volumetric;
                self.set_common_uniforms(
                    gl,
                    u,
                    config,
                    num_columns,
                    num_rows,
                    raindrop_state,
                    symbol_state,
                    effect_state,
                );
                gl.uniform_1_f32(u.density.as_ref(), config.density);
                gl.uniform_2_f32(
                    u.quad_size.as_ref(),
                    1.0 / num_columns as f32,
                    1.0 / num_rows as f32,
                );
                gl.uniform_1_f32(u.glyph_vertical_spacing.as_ref(), GLYPH_VERTICAL_SPACING);
                let aspect_ratio = screen_width as f32 / screen_height as f32;
                let view_projection = camera::volumetric_view_projection(aspect_ratio);
                gl.uniform_matrix_4_f32_slice(u.view_projection.as_ref(), false, &view_projection);
                gl.uniform_1_f32(u.time.as_ref(), time);
                gl.uniform_1_f32(u.animation_speed.as_ref(), config.animation_speed);
                gl.uniform_1_f32(u.forward_speed.as_ref(), config.forward_speed);

                self.quad_grid.draw(gl);
            } else {
                gl.use_program(Some(self.program_2d));
                let u = &self.uniforms_2d;
                self.set_common_uniforms(
                    gl,
                    u,
                    config,
                    num_columns,
                    num_rows,
                    raindrop_state,
                    symbol_state,
                    effect_state,
                );
                let aspect_ratio = screen_width as f32 / screen_height as f32;
                let screen_size = if aspect_ratio > 1.0 {
                    [1.0, aspect_ratio]
                } else {
                    [1.0 / aspect_ratio, 1.0]
                };
                gl.uniform_2_f32(u.screen_size.as_ref(), screen_size[0], screen_size[1]);
                let slant_vec = [config.slant.cos(), config.slant.sin()];
                let slant_scale =
                    1.0 / ((2.0 * config.slant).sin().abs() * (2.0f32.sqrt() - 1.0) + 1.0);
                gl.uniform_2_f32(u.slant_vec.as_ref(), slant_vec[0], slant_vec[1]);
                gl.uniform_1_f32(u.slant_scale.as_ref(), slant_scale);
                gl.uniform_1_i32(u.is_polar.as_ref(), config.is_polar as i32);

                self.triangle.draw(gl);
            }

            gl.disable(glow::BLEND);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.triangle.destroy(gl);
            self.quad_grid.destroy(gl);
            gl.delete_texture(self.glyph_msdf);
            if let Some(t) = self.glint_msdf {
                gl.delete_texture(t);
            }
            if let Some((_, t)) = self.base_texture {
                gl.delete_texture(t);
            }
            if let Some((_, t)) = self.glint_texture {
                gl.delete_texture(t);
            }
            gl.delete_texture(self.white_texture);
            gl.delete_program(self.program_2d);
            gl.delete_program(self.program_volumetric);
        }
    }
}
