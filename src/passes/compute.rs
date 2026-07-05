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

/// Real-time duration one logical `tick` represents, in seconds. Must
/// match `MatrixBackground`'s own repaint-request cadence
/// (`REPAINT_MS` in `lib.rs`, currently 33ms i.e. ~30fps) rather than the
/// original Rezmason/matrix source's 60fps assumption
/// (`js/config.js`'s `fps: 60`): under normal conditions, each rendered
/// frame must represent at most one logical tick, or `symbol.frag.glsl`'s
/// per-cell phase-based desync (each cell's own `previousAge` spreads out
/// *when* it crosses the glyph-cycling threshold) collapses into a
/// synchronized mass reassignment whenever more than one tick's worth of
/// crossings get batched into a single visible frame. Since this port's
/// render rate is throttled below the original's 60fps, matching the
/// *render* cadence here (rather than the original's tick cadence) is
/// what keeps individual glyphs cycling independently — at the cost of
/// running roughly half as fast, absolute-speed-wise, as the original.
const NOMINAL_TICK_SECS: f32 = 1.0 / 30.0;

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
    /// See `count_gate_events` — how many times the glyph-cycling gate
    /// would have fired had ticks been advanced one at a time instead of
    /// batched, since the last call to `run`.
    gate_event_count: Option<glow::UniformLocation>,
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
    /// The `time` value (see `run`'s `time` param — cumulative, unscaled,
    /// wall-clock-derived seconds) as of the last time `tick` advanced.
    /// Used to bin real elapsed time into logical ticks instead of
    /// advancing once per `run()` call — see `advance_tick`.
    last_tick_time: f32,
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
                gate_event_count: gl.get_uniform_location(symbol_program, "gateEventCount"),
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
                last_tick_time: 0.0,
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
    ///
    /// This is a no-op (no draws, no ping-pong swaps, no state mutation —
    /// `raindrop_state()` etc. keep returning whatever they returned last
    /// time) if no new logical tick has elapsed since the previous call to
    /// `run`. This matters because `cycle_frame_skip == 1` (the default)
    /// makes `symbol.frag.glsl`'s `mod(tick, cycleFrameSkip) == 0.` gate a
    /// no-op — it's always true — so without this early return, calling
    /// `run` more often than the nominal tick rate (e.g. extra repaints
    /// `eframe` forces on mouse movement) would advance the glyph-cycling
    /// `age` accumulator once per *call* rather than once per elapsed
    /// tick, exactly the bug this whole tick-pacing mechanism exists to
    /// prevent. Skipping only this pass (not the render/bloom/effect
    /// passes downstream, which still redraw every call) avoids a missing-
    /// frame flicker, since unlike the original's persistent browser
    /// canvas, egui repaints the whole window every call and expects a
    /// fresh frame back.
    pub unsafe fn run(&mut self, gl: &glow::Context, config: &MatrixConfig, time: f32) {
        unsafe {
            let previous_tick = self.tick;
            (self.tick, self.last_tick_time) = advance_tick(self.tick, self.last_tick_time, time);
            if self.tick == previous_tick {
                return;
            }
            let ticks_elapsed = self.tick - previous_tick;
            let gate_event_count =
                count_gate_events(previous_tick, self.tick, config.cycle_frame_skip) as f32;
            // `mix(prev, target, decay)` applied once per elapsed tick, N
            // times in a row, is equivalent to a single
            // `mix(prev, target, 1 - (1-decay)^N)` (holding the target
            // constant across the batch — see `count_gate_events`'s doc
            // comment for the analogous reasoning behind batching ticks in
            // the first place). This keeps `brightness_decay`'s organic
            // fade rate correct even when `ticks_elapsed > 1`, without
            // needing a shader-side loop.
            let effective_brightness_decay =
                1.0 - (1.0 - config.brightness_decay).powi(ticks_elapsed as i32);
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
                effective_brightness_decay,
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
                self.symbol_uniforms.gate_event_count.as_ref(),
                gate_event_count,
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

/// Given the current tick counter and the `time` value as of the last tick
/// advancement, plus this call's `time`, returns the updated
/// `(tick, last_tick_time)`. Advances `tick` by however many
/// `NOMINAL_TICK_SECS`-sized chunks of real elapsed time have accumulated
/// since the last advancement (0 for a burst of near-zero-dt extra calls,
/// 1 for steady nominal-rate calls, more than 1 to catch up after a
/// stall/lag spike) instead of once per call. This keeps glyph-cycling
/// (gated on `tick` in `symbol.frag.glsl`) paced by real time rather than
/// by how many times `run()` happens to be invoked — some hosts (e.g.
/// `eframe`'s native backend) force extra repaints on input events like
/// mouse movement, which would otherwise make `tick`-gated animation speed
/// up during those bursts.
fn advance_tick(tick: u32, last_tick_time: f32, time: f32) -> (u32, f32) {
    if tick == 0 {
        // First call ever: seed tick at 1 unconditionally so `isFirstFrame`
        // (`tick <= 1.` in raindrop.frag.glsl / symbol.frag.glsl) still
        // fires on the true first frame.
        return (1, time);
    }
    let elapsed = (time - last_tick_time).max(0.0);
    // `+ 1e-4` guards against float rounding landing just under an exact
    // tick boundary (e.g. `10.0 * NOMINAL_TICK_SECS / NOMINAL_TICK_SECS`
    // can evaluate to `9.999999` in f32), which would otherwise undercount
    // by one tick.
    let ticks_elapsed = (elapsed / NOMINAL_TICK_SECS + 1e-4).floor() as u32;
    if ticks_elapsed == 0 {
        return (tick, last_tick_time);
    }
    (
        tick + ticks_elapsed,
        last_tick_time + ticks_elapsed as f32 * NOMINAL_TICK_SECS,
    )
}

/// Counts how many multiples of `cycle_frame_skip` lie in
/// `(previous_tick, tick]` — i.e. how many times `symbol.frag.glsl`'s
/// glyph-cycling gate would have fired had ticks advanced one at a time
/// instead of being batched by `advance_tick`. Uses the standard "count
/// multiples of k up to n" identity (`floor(n/k) - floor(m/k)`), so it
/// naturally handles more than one crossing (e.g. after a long stall)
/// without a loop. `cycle_frame_skip` is clamped to a minimum of 1 to
/// avoid dividing by zero should a corrupt/legacy config on disk have
/// `cycle_frame_skip == 0`.
fn count_gate_events(previous_tick: u32, tick: u32, cycle_frame_skip: u32) -> u32 {
    let skip = cycle_frame_skip.max(1);
    tick / skip - previous_tick / skip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_seeds_tick_to_one() {
        assert_eq!(advance_tick(0, 0.0, 0.0), (1, 0.0));
    }

    #[test]
    fn burst_of_near_zero_dt_calls_does_not_advance_tick() {
        let (tick, last) = advance_tick(1, 0.0, 0.0);
        // Several extra calls, each with a tiny real dt (e.g. mouse-move
        // forced repaints), none individually crossing NOMINAL_TICK_SECS.
        let (tick, last) = advance_tick(tick, last, 0.001);
        let (tick, last) = advance_tick(tick, last, 0.002);
        let (tick, _last) = advance_tick(tick, last, 0.003);
        assert_eq!(tick, 1);
    }

    #[test]
    fn steady_nominal_rate_advances_by_one_each_call() {
        let (mut tick, mut last) = (1u32, 0.0f32);
        for i in 1..=10 {
            (tick, last) = advance_tick(tick, last, i as f32 * NOMINAL_TICK_SECS);
        }
        assert_eq!(tick, 11);
    }

    #[test]
    fn stall_catches_up_by_more_than_one() {
        let (tick, last) = advance_tick(5, 1.0, 1.0 + 10.0 * NOMINAL_TICK_SECS);
        assert_eq!(tick, 15);
        assert_eq!(last, 1.0 + 10.0 * NOMINAL_TICK_SECS);
    }

    #[test]
    fn gate_events_fire_every_tick_when_skip_is_one() {
        assert_eq!(count_gate_events(4, 5, 1), 1);
        assert_eq!(count_gate_events(4, 6, 1), 2); // a batched jump of 2 ticks
    }

    #[test]
    fn gate_events_detect_crossed_multiple_mid_batch() {
        // jump from tick 7 to tick 9 with skip 8: crosses the multiple at 8.
        assert_eq!(count_gate_events(7, 9, 8), 1);
    }

    #[test]
    fn gate_events_zero_when_no_multiple_crossed() {
        // jump from tick 0 to tick 2 with skip 3: no multiple of 3 in (0, 2].
        assert_eq!(count_gate_events(0, 2, 3), 0);
    }

    #[test]
    fn gate_events_count_multiple_crossings_after_a_stall() {
        // jump from tick 1 to tick 20 with skip 8: crosses 8 and 16.
        assert_eq!(count_gate_events(1, 20, 8), 2);
    }

    #[test]
    fn gate_events_treat_zero_skip_as_one_to_avoid_div_by_zero() {
        assert_eq!(count_gate_events(4, 6, 0), 2);
    }

    #[test]
    fn effective_decay_is_unchanged_for_single_tick() {
        let effective = 1.0 - (1.0f32 - 0.75).powi(1);
        assert!((effective - 0.75).abs() < 1e-6);
    }

    #[test]
    fn effective_decay_is_identity_at_decay_one() {
        assert_eq!(1.0 - (1.0f32 - 1.0).powi(2), 1.0);
        assert_eq!(1.0 - (1.0f32 - 1.0).powi(1), 1.0);
    }

    #[test]
    fn effective_decay_compounds_for_two_ticks() {
        // mix applied twice with decay=0.75 in a row is equivalent to a
        // single mix with 1-(1-0.75)^2 = 0.9375.
        let effective = 1.0 - (1.0f32 - 0.75).powi(2);
        assert!((effective - 0.9375).abs() < 1e-6);
    }
}
