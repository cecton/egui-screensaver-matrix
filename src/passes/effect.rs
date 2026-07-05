//! The final compositing pass: maps rain+bloom brightness to color, either
//! through a smooth gradient (`palette`) or a repeating/sweeping custom
//! color sequence (`stripes`/`pride`/`trans`), ported from
//! `js/regl/palettePass.js` / `js/regl/stripePass.js` and
//! `shaders/glsl/palettePass.frag.glsl` / `shaders/glsl/stripePass.frag.glsl`
//! at https://github.com/Rezmason/matrix (MIT licensed).

use std::sync::Arc;

use glow::HasContext as _;

use crate::config::{Effect, MatrixConfig, PaletteStop};
use crate::gl_util::{self, FullscreenTriangle};

const FULLSCREEN_VERT_SRC: &str = include_str!("../shaders/fullscreen.vert.glsl");
const PALETTE_FRAG_SRC: &str = include_str!("../shaders/palette.frag.glsl");
const STRIPES_FRAG_SRC: &str = include_str!("../shaders/stripes.frag.glsl");

/// Resolution of the generated palette gradient texture, matching the
/// original's `PALETTE_SIZE`.
const PALETTE_SIZE: usize = 2048;

struct PaletteUniforms {
    tex: Option<glow::UniformLocation>,
    bloom_tex: Option<glow::UniformLocation>,
    palette_tex: Option<glow::UniformLocation>,
    dither_magnitude: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    background_color: Option<glow::UniformLocation>,
    cursor_color: Option<glow::UniformLocation>,
    glint_color: Option<glow::UniformLocation>,
    cursor_intensity: Option<glow::UniformLocation>,
    glint_intensity: Option<glow::UniformLocation>,
}

struct StripesUniforms {
    tex: Option<glow::UniformLocation>,
    bloom_tex: Option<glow::UniformLocation>,
    stripe_tex: Option<glow::UniformLocation>,
    dither_magnitude: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    background_color: Option<glow::UniformLocation>,
    cursor_color: Option<glow::UniformLocation>,
    glint_color: Option<glow::UniformLocation>,
    cursor_intensity: Option<glow::UniformLocation>,
    glint_intensity: Option<glow::UniformLocation>,
}

/// Builds the `PALETTE_SIZE`-texel 1D gradient texture from a set of
/// (possibly unsorted) color stops, ported from `palettePass.js`'s
/// `makePalette`: stops are sorted by `at`, the first/last colors are
/// extended to the texture's edges, and intermediate texels are linearly
/// interpolated between neighboring stops.
fn build_palette_rgba(stops: &[PaletteStop]) -> Vec<u8> {
    let mut sorted: Vec<&PaletteStop> = stops.iter().collect();
    sorted.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap());

    let mut entries: Vec<(usize, [f32; 3])> = sorted
        .iter()
        .map(|stop| {
            let index = ((stop.at.clamp(0.0, 1.0)) * (PALETTE_SIZE - 1) as f32) as usize;
            (index, stop.color)
        })
        .collect();
    entries.insert(0, (0, entries[0].1));
    entries.push((PALETTE_SIZE - 1, entries[entries.len() - 1].1));

    let mut texels = vec![[0.0f32; 3]; PALETTE_SIZE];
    for pair in entries.windows(2) {
        let (start_index, start_color) = pair[0];
        let (end_index, end_color) = pair[1];
        texels[start_index] = start_color;
        let diff = end_index - start_index;
        for i in 0..diff {
            let ratio = i as f32 / diff as f32;
            texels[start_index + i] = [
                start_color[0] * (1.0 - ratio) + end_color[0] * ratio,
                start_color[1] * (1.0 - ratio) + end_color[1] * ratio,
                start_color[2] * (1.0 - ratio) + end_color[2] * ratio,
            ];
        }
    }
    texels[PALETTE_SIZE - 1] = entries.last().unwrap().1;

    let mut rgba = Vec::with_capacity(PALETTE_SIZE * 4);
    for color in texels {
        rgba.extend_from_slice(&[
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            255,
        ]);
    }
    rgba
}

fn build_stripe_rgba(colors: &[[f32; 3]]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(colors.len() * 4);
    for color in colors {
        rgba.extend_from_slice(&[
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            255,
        ]);
    }
    rgba
}

unsafe fn upload_1d_rgba(
    gl: &glow::Context,
    rgba: &[u8],
    width: i32,
) -> Result<glow::Texture, String> {
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
            width,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(rgba)),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
}

fn stripe_colors_for(effect: &Effect) -> Option<Vec<[f32; 3]>> {
    match effect {
        Effect::Stripes(colors) => Some(colors.clone()),
        Effect::Pride => Some(crate::config::pride_stripe_colors()),
        Effect::Trans => Some(crate::config::trans_stripe_colors()),
        Effect::Palette(_) => None,
    }
}

pub struct EffectPasses {
    palette_program: glow::Program,
    palette_triangle: FullscreenTriangle,
    palette_uniforms: PaletteUniforms,

    stripes_program: glow::Program,
    stripes_triangle: FullscreenTriangle,
    stripes_uniforms: StripesUniforms,

    palette_texture: glow::Texture,
    built_palette: Vec<PaletteStop>,
    stripe_texture: glow::Texture,
    built_stripes: Vec<[f32; 3]>,
}

impl EffectPasses {
    pub unsafe fn new(
        gl: &Arc<glow::Context>,
        header: &str,
        config: &MatrixConfig,
    ) -> Result<Self, String> {
        unsafe {
            let palette_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, PALETTE_FRAG_SRC)?;
            let palette_triangle = FullscreenTriangle::new(gl, palette_program)?;
            let palette_uniforms = PaletteUniforms {
                tex: gl.get_uniform_location(palette_program, "tex"),
                bloom_tex: gl.get_uniform_location(palette_program, "bloomTex"),
                palette_tex: gl.get_uniform_location(palette_program, "paletteTex"),
                dither_magnitude: gl.get_uniform_location(palette_program, "ditherMagnitude"),
                time: gl.get_uniform_location(palette_program, "time"),
                background_color: gl.get_uniform_location(palette_program, "backgroundColor"),
                cursor_color: gl.get_uniform_location(palette_program, "cursorColor"),
                glint_color: gl.get_uniform_location(palette_program, "glintColor"),
                cursor_intensity: gl.get_uniform_location(palette_program, "cursorIntensity"),
                glint_intensity: gl.get_uniform_location(palette_program, "glintIntensity"),
            };

            let stripes_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, STRIPES_FRAG_SRC)?;
            let stripes_triangle = FullscreenTriangle::new(gl, stripes_program)?;
            let stripes_uniforms = StripesUniforms {
                tex: gl.get_uniform_location(stripes_program, "tex"),
                bloom_tex: gl.get_uniform_location(stripes_program, "bloomTex"),
                stripe_tex: gl.get_uniform_location(stripes_program, "stripeTex"),
                dither_magnitude: gl.get_uniform_location(stripes_program, "ditherMagnitude"),
                time: gl.get_uniform_location(stripes_program, "time"),
                background_color: gl.get_uniform_location(stripes_program, "backgroundColor"),
                cursor_color: gl.get_uniform_location(stripes_program, "cursorColor"),
                glint_color: gl.get_uniform_location(stripes_program, "glintColor"),
                cursor_intensity: gl.get_uniform_location(stripes_program, "cursorIntensity"),
                glint_intensity: gl.get_uniform_location(stripes_program, "glintIntensity"),
            };

            let built_palette = match &config.effect {
                Effect::Palette(stops) => stops.clone(),
                _ => vec![PaletteStop {
                    color: [0.0, 1.0, 0.0],
                    at: 1.0,
                }],
            };
            let palette_texture =
                upload_1d_rgba(gl, &build_palette_rgba(&built_palette), PALETTE_SIZE as i32)?;

            let built_stripes =
                stripe_colors_for(&config.effect).unwrap_or_else(|| vec![[0.0, 1.0, 0.0]]);
            let stripe_texture = upload_1d_rgba(
                gl,
                &build_stripe_rgba(&built_stripes),
                built_stripes.len() as i32,
            )?;

            Ok(Self {
                palette_program,
                palette_triangle,
                palette_uniforms,
                stripes_program,
                stripes_triangle,
                stripes_uniforms,
                palette_texture,
                built_palette,
                stripe_texture,
                built_stripes,
            })
        }
    }

    unsafe fn sync_textures(&mut self, gl: &glow::Context, effect: &Effect) {
        unsafe {
            if let Effect::Palette(stops) = effect
                && stops.as_slice() != self.built_palette.as_slice()
            {
                gl.delete_texture(self.palette_texture);
                self.palette_texture =
                    upload_1d_rgba(gl, &build_palette_rgba(stops), PALETTE_SIZE as i32)
                        .expect("rebuilding the palette texture should not fail");
                self.built_palette = stops.clone();
            }

            if let Some(colors) = stripe_colors_for(effect)
                && colors != self.built_stripes
            {
                gl.delete_texture(self.stripe_texture);
                self.stripe_texture =
                    upload_1d_rgba(gl, &build_stripe_rgba(&colors), colors.len() as i32)
                        .expect("rebuilding the stripe texture should not fail");
                self.built_stripes = colors;
            }
        }
    }

    /// Draws the final composited image into whatever framebuffer is
    /// currently bound.
    pub unsafe fn run(
        &mut self,
        gl: &glow::Context,
        config: &MatrixConfig,
        time: f32,
        primary: glow::Texture,
        bloom: glow::Texture,
    ) {
        unsafe {
            self.sync_textures(gl, &config.effect);

            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(primary));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(bloom));

            match &config.effect {
                Effect::Palette(_) => {
                    gl.active_texture(glow::TEXTURE2);
                    gl.bind_texture(glow::TEXTURE_2D, Some(self.palette_texture));
                    gl.use_program(Some(self.palette_program));
                    gl.uniform_1_i32(self.palette_uniforms.tex.as_ref(), 0);
                    gl.uniform_1_i32(self.palette_uniforms.bloom_tex.as_ref(), 1);
                    gl.uniform_1_i32(self.palette_uniforms.palette_tex.as_ref(), 2);
                    gl.uniform_1_f32(
                        self.palette_uniforms.dither_magnitude.as_ref(),
                        config.dither_magnitude,
                    );
                    gl.uniform_1_f32(self.palette_uniforms.time.as_ref(), time);
                    gl.uniform_3_f32(
                        self.palette_uniforms.background_color.as_ref(),
                        config.background_color[0],
                        config.background_color[1],
                        config.background_color[2],
                    );
                    gl.uniform_3_f32(
                        self.palette_uniforms.cursor_color.as_ref(),
                        config.cursor_color[0],
                        config.cursor_color[1],
                        config.cursor_color[2],
                    );
                    gl.uniform_3_f32(
                        self.palette_uniforms.glint_color.as_ref(),
                        config.glint_color[0],
                        config.glint_color[1],
                        config.glint_color[2],
                    );
                    gl.uniform_1_f32(
                        self.palette_uniforms.cursor_intensity.as_ref(),
                        config.cursor_intensity,
                    );
                    gl.uniform_1_f32(
                        self.palette_uniforms.glint_intensity.as_ref(),
                        config.glint_intensity,
                    );
                    self.palette_triangle.draw(gl);
                }
                Effect::Stripes(_) | Effect::Pride | Effect::Trans => {
                    gl.active_texture(glow::TEXTURE2);
                    gl.bind_texture(glow::TEXTURE_2D, Some(self.stripe_texture));
                    gl.use_program(Some(self.stripes_program));
                    gl.uniform_1_i32(self.stripes_uniforms.tex.as_ref(), 0);
                    gl.uniform_1_i32(self.stripes_uniforms.bloom_tex.as_ref(), 1);
                    gl.uniform_1_i32(self.stripes_uniforms.stripe_tex.as_ref(), 2);
                    gl.uniform_1_f32(
                        self.stripes_uniforms.dither_magnitude.as_ref(),
                        config.dither_magnitude,
                    );
                    gl.uniform_1_f32(self.stripes_uniforms.time.as_ref(), time);
                    gl.uniform_3_f32(
                        self.stripes_uniforms.background_color.as_ref(),
                        config.background_color[0],
                        config.background_color[1],
                        config.background_color[2],
                    );
                    gl.uniform_3_f32(
                        self.stripes_uniforms.cursor_color.as_ref(),
                        config.cursor_color[0],
                        config.cursor_color[1],
                        config.cursor_color[2],
                    );
                    gl.uniform_3_f32(
                        self.stripes_uniforms.glint_color.as_ref(),
                        config.glint_color[0],
                        config.glint_color[1],
                        config.glint_color[2],
                    );
                    gl.uniform_1_f32(
                        self.stripes_uniforms.cursor_intensity.as_ref(),
                        config.cursor_intensity,
                    );
                    gl.uniform_1_f32(
                        self.stripes_uniforms.glint_intensity.as_ref(),
                        config.glint_intensity,
                    );
                    self.stripes_triangle.draw(gl);
                }
            }

            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.palette_triangle.destroy(gl);
            self.stripes_triangle.destroy(gl);
            gl.delete_program(self.palette_program);
            gl.delete_program(self.stripes_program);
            gl.delete_texture(self.palette_texture);
            gl.delete_texture(self.stripe_texture);
        }
    }
}
