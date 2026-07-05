//! The 4 ping-pong "compute" passes that drive the rain simulation: intro
//! reveal, raindrop fall/decay, glyph cycling, and ripple/thunder effects.
//! Each is a tiny offscreen render target (grid resolution, not screen
//! resolution) redrawn every frame from its own previous frame's output.

use std::sync::Arc;

use glow::HasContext as _;

use crate::config::MatrixConfig;
use crate::fonts::FontAtlas;
use crate::gl_util::{self, FullscreenTriangle, PingPong, TargetFormat};

const FULLSCREEN_VERT_SRC: &str = include_str!("../shaders/fullscreen.vert.glsl");
const INTRO_FRAG_SRC: &str = include_str!("../shaders/intro.frag.glsl");
const RAINDROP_FRAG_SRC: &str = include_str!("../shaders/raindrop.frag.glsl");
const SYMBOL_FRAG_SRC: &str = include_str!("../shaders/symbol.frag.glsl");
const EFFECT_FRAG_SRC: &str = include_str!("../shaders/effect.frag.glsl");

struct IntroUniforms {
    previous_intro_state: Option<glow::UniformLocation>,
    num_columns: Option<glow::UniformLocation>,
    num_rows: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    tick: Option<glow::UniformLocation>,
    animation_speed: Option<glow::UniformLocation>,
    fall_speed: Option<glow::UniformLocation>,
    skip_intro: Option<glow::UniformLocation>,
}

struct RaindropUniforms {
    previous_raindrop_state: Option<glow::UniformLocation>,
    intro_state: Option<glow::UniformLocation>,
    num_columns: Option<glow::UniformLocation>,
    num_rows: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    tick: Option<glow::UniformLocation>,
    animation_speed: Option<glow::UniformLocation>,
    fall_speed: Option<glow::UniformLocation>,
    loops: Option<glow::UniformLocation>,
    skip_intro: Option<glow::UniformLocation>,
    brightness_decay: Option<glow::UniformLocation>,
    raindrop_length: Option<glow::UniformLocation>,
}

struct SymbolUniforms {
    previous_symbol_state: Option<glow::UniformLocation>,
    raindrop_state: Option<glow::UniformLocation>,
    num_columns: Option<glow::UniformLocation>,
    num_rows: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    tick: Option<glow::UniformLocation>,
    cycle_frame_skip: Option<glow::UniformLocation>,
    animation_speed: Option<glow::UniformLocation>,
    cycle_speed: Option<glow::UniformLocation>,
    loops: Option<glow::UniformLocation>,
    glyph_sequence_length: Option<glow::UniformLocation>,
}

struct EffectUniforms {
    num_columns: Option<glow::UniformLocation>,
    num_rows: Option<glow::UniformLocation>,
    time: Option<glow::UniformLocation>,
    tick: Option<glow::UniformLocation>,
    animation_speed: Option<glow::UniformLocation>,
    has_thunder: Option<glow::UniformLocation>,
    loops: Option<glow::UniformLocation>,
    glyph_height_to_width: Option<glow::UniformLocation>,
    ripple_type: Option<glow::UniformLocation>,
    ripple_scale: Option<glow::UniformLocation>,
    ripple_speed: Option<glow::UniformLocation>,
    ripple_thickness: Option<glow::UniformLocation>,
}

/// The 4 ping-pong compute passes plus the shared bookkeeping (grid
/// resolution, frame tick counter) they all need.
pub struct ComputePasses {
    intro_program: glow::Program,
    intro_triangle: FullscreenTriangle,
    intro_uniforms: IntroUniforms,
    intro_buf: PingPong,

    raindrop_program: glow::Program,
    raindrop_triangle: FullscreenTriangle,
    raindrop_uniforms: RaindropUniforms,
    raindrop_buf: PingPong,

    symbol_program: glow::Program,
    symbol_triangle: FullscreenTriangle,
    symbol_uniforms: SymbolUniforms,
    symbol_buf: PingPong,

    effect_program: glow::Program,
    effect_triangle: FullscreenTriangle,
    effect_uniforms: EffectUniforms,
    effect_buf: PingPong,

    num_columns: u32,
    num_rows: u32,
    tick: u32,
}

impl ComputePasses {
    pub unsafe fn new(
        gl: &Arc<glow::Context>,
        header: &str,
        num_columns: u32,
        num_rows: u32,
        font: &FontAtlas,
    ) -> Result<Self, String> {
        unsafe {
            let intro_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, INTRO_FRAG_SRC)?;
            let intro_triangle = FullscreenTriangle::new(gl, intro_program)?;
            let intro_uniforms = IntroUniforms {
                previous_intro_state: gl.get_uniform_location(intro_program, "previousIntroState"),
                num_columns: gl.get_uniform_location(intro_program, "numColumns"),
                num_rows: gl.get_uniform_location(intro_program, "numRows"),
                time: gl.get_uniform_location(intro_program, "time"),
                tick: gl.get_uniform_location(intro_program, "tick"),
                animation_speed: gl.get_uniform_location(intro_program, "animationSpeed"),
                fall_speed: gl.get_uniform_location(intro_program, "fallSpeed"),
                skip_intro: gl.get_uniform_location(intro_program, "skipIntro"),
            };
            let intro_buf = PingPong::new(
                gl,
                num_columns as i32,
                1,
                TargetFormat::Rgba16F,
                gl_util::TargetFilter::Nearest,
            )?;

            let raindrop_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, RAINDROP_FRAG_SRC)?;
            let raindrop_triangle = FullscreenTriangle::new(gl, raindrop_program)?;
            let raindrop_uniforms = RaindropUniforms {
                previous_raindrop_state: gl
                    .get_uniform_location(raindrop_program, "previousRaindropState"),
                intro_state: gl.get_uniform_location(raindrop_program, "introState"),
                num_columns: gl.get_uniform_location(raindrop_program, "numColumns"),
                num_rows: gl.get_uniform_location(raindrop_program, "numRows"),
                time: gl.get_uniform_location(raindrop_program, "time"),
                tick: gl.get_uniform_location(raindrop_program, "tick"),
                animation_speed: gl.get_uniform_location(raindrop_program, "animationSpeed"),
                fall_speed: gl.get_uniform_location(raindrop_program, "fallSpeed"),
                loops: gl.get_uniform_location(raindrop_program, "loops"),
                skip_intro: gl.get_uniform_location(raindrop_program, "skipIntro"),
                brightness_decay: gl.get_uniform_location(raindrop_program, "brightnessDecay"),
                raindrop_length: gl.get_uniform_location(raindrop_program, "raindropLength"),
            };
            let raindrop_buf = PingPong::new(
                gl,
                num_columns as i32,
                num_rows as i32,
                TargetFormat::Rgba16F,
                gl_util::TargetFilter::Nearest,
            )?;

            let symbol_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, SYMBOL_FRAG_SRC)?;
            let symbol_triangle = FullscreenTriangle::new(gl, symbol_program)?;
            let symbol_uniforms = SymbolUniforms {
                previous_symbol_state: gl
                    .get_uniform_location(symbol_program, "previousSymbolState"),
                raindrop_state: gl.get_uniform_location(symbol_program, "raindropState"),
                num_columns: gl.get_uniform_location(symbol_program, "numColumns"),
                num_rows: gl.get_uniform_location(symbol_program, "numRows"),
                time: gl.get_uniform_location(symbol_program, "time"),
                tick: gl.get_uniform_location(symbol_program, "tick"),
                cycle_frame_skip: gl.get_uniform_location(symbol_program, "cycleFrameSkip"),
                animation_speed: gl.get_uniform_location(symbol_program, "animationSpeed"),
                cycle_speed: gl.get_uniform_location(symbol_program, "cycleSpeed"),
                loops: gl.get_uniform_location(symbol_program, "loops"),
                glyph_sequence_length: gl
                    .get_uniform_location(symbol_program, "glyphSequenceLength"),
            };
            // `glyphSequenceLength` depends only on the font, not on any
            // per-frame state, so it's set once here rather than every
            // frame in `run` — uniform values persist on a program
            // regardless of which program is currently bound.
            gl.use_program(Some(symbol_program));
            gl.uniform_1_f32(
                symbol_uniforms.glyph_sequence_length.as_ref(),
                font.glyph_sequence_length as f32,
            );
            gl.use_program(None);
            let symbol_buf = PingPong::new(
                gl,
                num_columns as i32,
                num_rows as i32,
                TargetFormat::Rgba16F,
                gl_util::TargetFilter::Nearest,
            )?;

            let effect_program =
                gl_util::compile_program(gl, header, FULLSCREEN_VERT_SRC, EFFECT_FRAG_SRC)?;
            let effect_triangle = FullscreenTriangle::new(gl, effect_program)?;
            let effect_uniforms = EffectUniforms {
                num_columns: gl.get_uniform_location(effect_program, "numColumns"),
                num_rows: gl.get_uniform_location(effect_program, "numRows"),
                time: gl.get_uniform_location(effect_program, "time"),
                tick: gl.get_uniform_location(effect_program, "tick"),
                animation_speed: gl.get_uniform_location(effect_program, "animationSpeed"),
                has_thunder: gl.get_uniform_location(effect_program, "hasThunder"),
                loops: gl.get_uniform_location(effect_program, "loops"),
                glyph_height_to_width: gl
                    .get_uniform_location(effect_program, "glyphHeightToWidth"),
                ripple_type: gl.get_uniform_location(effect_program, "rippleType"),
                ripple_scale: gl.get_uniform_location(effect_program, "rippleScale"),
                ripple_speed: gl.get_uniform_location(effect_program, "rippleSpeed"),
                ripple_thickness: gl.get_uniform_location(effect_program, "rippleThickness"),
            };
            let effect_buf = PingPong::new(
                gl,
                num_columns as i32,
                num_rows as i32,
                TargetFormat::Rgba16F,
                gl_util::TargetFilter::Nearest,
            )?;

            Ok(Self {
                intro_program,
                intro_triangle,
                intro_uniforms,
                intro_buf,
                raindrop_program,
                raindrop_triangle,
                raindrop_uniforms,
                raindrop_buf,
                symbol_program,
                symbol_triangle,
                symbol_uniforms,
                symbol_buf,
                effect_program,
                effect_triangle,
                effect_uniforms,
                effect_buf,
                num_columns,
                num_rows,
                tick: 0,
            })
        }
    }

    /// Updates the symbol pass's `glyphSequenceLength` uniform for a new
    /// font (this value depends only on the font, so it's normally set
    /// once at construction — see the comment where it's first set — but
    /// needs updating if the font changes at runtime, e.g. a preset
    /// switch).
    pub unsafe fn set_glyph_sequence_length(&self, gl: &glow::Context, glyph_sequence_length: u32) {
        unsafe {
            gl.use_program(Some(self.symbol_program));
            gl.uniform_1_f32(
                self.symbol_uniforms.glyph_sequence_length.as_ref(),
                glyph_sequence_length as f32,
            );
            gl.use_program(None);
        }
    }

    /// Recreates the compute buffers if the grid resolution changed
    /// (driven by `num_columns` and, in volumetric mode, `density` — see
    /// `passes/mod.rs` — non-volumetric mode always has `num_rows ==
    /// num_columns`, matching the original: the render pass maps this
    /// square grid onto the possibly-non-square viewport via a
    /// `screenSize` stretch/crop instead of a non-square grid), matching
    /// the original's own resize behavior of losing transient state on a
    /// resolution change.
    pub unsafe fn resize(
        &mut self,
        gl: &glow::Context,
        num_columns: u32,
        num_rows: u32,
    ) -> Result<(), String> {
        unsafe {
            if num_columns == self.num_columns && num_rows == self.num_rows {
                return Ok(());
            }
            self.intro_buf.resize(gl, num_columns as i32, 1)?;
            self.raindrop_buf
                .resize(gl, num_columns as i32, num_rows as i32)?;
            self.symbol_buf
                .resize(gl, num_columns as i32, num_rows as i32)?;
            self.effect_buf
                .resize(gl, num_columns as i32, num_rows as i32)?;
            self.num_columns = num_columns;
            self.num_rows = num_rows;
            Ok(())
        }
    }

    /// Runs intro -> raindrop -> symbol -> effect in order, then swaps
    /// every ping-pong buffer for next frame. `raindrop_state()`,
    /// `symbol_state()`, and `effect_state()` are valid to sample
    /// (reflecting this frame's freshly computed state) immediately after
    /// this returns, until the next call to `run`.
    pub unsafe fn run(&mut self, gl: &glow::Context, config: &MatrixConfig, time: f32) {
        unsafe {
            self.tick += 1;
            let tick = self.tick as f32;
            let num_columns = self.num_columns as f32;
            let num_rows = self.num_rows as f32;

            // egui_glow leaves GL_SCISSOR_TEST enabled (with whatever rect
            // it last clipped a UI element to) between paint callbacks, and
            // GL_BLEND enabled/disabled depending on what it last drew.
            // Without disabling both here, our "full buffer" fullscreen-
            // triangle draws would only update the sub-rect still covered
            // by a stale scissor rect, leaving the rest of the buffer's
            // previous contents in place — which, fed back through the
            // ping-pong buffers, explains runaway/incoherent per-cell state.
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);

            // --- intro ---
            self.intro_buf.write().bind(gl);
            gl.use_program(Some(self.intro_program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.intro_buf.read().texture));
            gl.uniform_1_i32(self.intro_uniforms.previous_intro_state.as_ref(), 0);
            gl.uniform_1_f32(self.intro_uniforms.num_columns.as_ref(), num_columns);
            gl.uniform_1_f32(self.intro_uniforms.num_rows.as_ref(), num_rows);
            gl.uniform_1_f32(self.intro_uniforms.time.as_ref(), time);
            gl.uniform_1_f32(self.intro_uniforms.tick.as_ref(), tick);
            gl.uniform_1_f32(
                self.intro_uniforms.animation_speed.as_ref(),
                config.animation_speed,
            );
            gl.uniform_1_f32(self.intro_uniforms.fall_speed.as_ref(), config.fall_speed);
            gl.uniform_1_i32(
                self.intro_uniforms.skip_intro.as_ref(),
                config.skip_intro as i32,
            );
            self.intro_triangle.draw(gl);
            self.intro_buf.swap();

            // --- raindrop ---
            self.raindrop_buf.write().bind(gl);
            gl.use_program(Some(self.raindrop_program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.raindrop_buf.read().texture));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.intro_buf.read().texture));
            gl.uniform_1_i32(self.raindrop_uniforms.previous_raindrop_state.as_ref(), 0);
            gl.uniform_1_i32(self.raindrop_uniforms.intro_state.as_ref(), 1);
            gl.uniform_1_f32(self.raindrop_uniforms.num_columns.as_ref(), num_columns);
            gl.uniform_1_f32(self.raindrop_uniforms.num_rows.as_ref(), num_rows);
            gl.uniform_1_f32(self.raindrop_uniforms.time.as_ref(), time);
            gl.uniform_1_f32(self.raindrop_uniforms.tick.as_ref(), tick);
            gl.uniform_1_f32(
                self.raindrop_uniforms.animation_speed.as_ref(),
                config.animation_speed,
            );
            gl.uniform_1_f32(
                self.raindrop_uniforms.fall_speed.as_ref(),
                config.fall_speed,
            );
            gl.uniform_1_i32(self.raindrop_uniforms.loops.as_ref(), 0);
            gl.uniform_1_i32(
                self.raindrop_uniforms.skip_intro.as_ref(),
                config.skip_intro as i32,
            );
            gl.uniform_1_f32(
                self.raindrop_uniforms.brightness_decay.as_ref(),
                config.brightness_decay,
            );
            gl.uniform_1_f32(
                self.raindrop_uniforms.raindrop_length.as_ref(),
                config.raindrop_length,
            );
            self.raindrop_triangle.draw(gl);
            self.raindrop_buf.swap();

            // --- symbol ---
            self.symbol_buf.write().bind(gl);
            gl.use_program(Some(self.symbol_program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.symbol_buf.read().texture));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.raindrop_buf.read().texture));
            gl.uniform_1_i32(self.symbol_uniforms.previous_symbol_state.as_ref(), 0);
            gl.uniform_1_i32(self.symbol_uniforms.raindrop_state.as_ref(), 1);
            gl.uniform_1_f32(self.symbol_uniforms.num_columns.as_ref(), num_columns);
            gl.uniform_1_f32(self.symbol_uniforms.num_rows.as_ref(), num_rows);
            gl.uniform_1_f32(self.symbol_uniforms.time.as_ref(), time);
            gl.uniform_1_f32(self.symbol_uniforms.tick.as_ref(), tick);
            gl.uniform_1_f32(
                self.symbol_uniforms.cycle_frame_skip.as_ref(),
                config.cycle_frame_skip as f32,
            );
            gl.uniform_1_f32(
                self.symbol_uniforms.animation_speed.as_ref(),
                config.animation_speed,
            );
            gl.uniform_1_f32(
                self.symbol_uniforms.cycle_speed.as_ref(),
                config.cycle_speed,
            );
            gl.uniform_1_i32(self.symbol_uniforms.loops.as_ref(), 0);
            self.symbol_triangle.draw(gl);
            self.symbol_buf.swap();

            // --- effect ---
            self.effect_buf.write().bind(gl);
            gl.use_program(Some(self.effect_program));
            gl.uniform_1_f32(self.effect_uniforms.num_columns.as_ref(), num_columns);
            gl.uniform_1_f32(self.effect_uniforms.num_rows.as_ref(), num_rows);
            gl.uniform_1_f32(self.effect_uniforms.time.as_ref(), time);
            gl.uniform_1_f32(self.effect_uniforms.tick.as_ref(), tick);
            gl.uniform_1_f32(
                self.effect_uniforms.animation_speed.as_ref(),
                config.animation_speed,
            );
            gl.uniform_1_i32(
                self.effect_uniforms.has_thunder.as_ref(),
                config.has_thunder as i32,
            );
            gl.uniform_1_i32(self.effect_uniforms.loops.as_ref(), 0);
            gl.uniform_1_f32(
                self.effect_uniforms.glyph_height_to_width.as_ref(),
                config.glyph_height_to_width,
            );
            let ripple_type = match config.ripple_type {
                None => -1,
                Some(crate::config::RippleType::Box) => 0,
                Some(crate::config::RippleType::Circle) => 1,
            };
            gl.uniform_1_i32(self.effect_uniforms.ripple_type.as_ref(), ripple_type);
            gl.uniform_1_f32(
                self.effect_uniforms.ripple_scale.as_ref(),
                config.ripple_scale,
            );
            gl.uniform_1_f32(
                self.effect_uniforms.ripple_speed.as_ref(),
                config.ripple_speed,
            );
            gl.uniform_1_f32(
                self.effect_uniforms.ripple_thickness.as_ref(),
                config.ripple_thickness,
            );
            self.effect_triangle.draw(gl);
            self.effect_buf.swap();

            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub fn raindrop_state(&self) -> glow::Texture {
        self.raindrop_buf.read().texture
    }

    pub fn symbol_state(&self) -> glow::Texture {
        self.symbol_buf.read().texture
    }

    pub fn effect_state(&self) -> glow::Texture {
        self.effect_buf.read().texture
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.intro_buf.destroy(gl);
            self.raindrop_buf.destroy(gl);
            self.symbol_buf.destroy(gl);
            self.effect_buf.destroy(gl);
            self.intro_triangle.destroy(gl);
            self.raindrop_triangle.destroy(gl);
            self.symbol_triangle.destroy(gl);
            self.effect_triangle.destroy(gl);
            gl.delete_program(self.intro_program);
            gl.delete_program(self.raindrop_program);
            gl.delete_program(self.symbol_program);
            gl.delete_program(self.effect_program);
        }
    }
}
