//! The 4 "compute" passes that drive the rain simulation: intro reveal,
//! raindrop fall/decay, glyph cycling, and ripple/thunder effects. Each is a
//! tiny offscreen render target (grid resolution, not screen resolution);
//! raindrop and symbol are redrawn every tick from their own previous
//! output (ping-pong), intro and effect are pure functions of time.

use egui_wgpu::wgpu;

use crate::config::{MatrixConfig, RippleType};
use crate::gpu::{
    self, PassPipeline, PingPong, RenderTarget, Samplers, TargetFormat, UniformBuffer, Uniforms,
};

const INTRO_SRC: &str = include_str!("../shaders/intro.wgsl");
const RAINDROP_SRC: &str = include_str!("../shaders/raindrop.wgsl");
const SYMBOL_SRC: &str = include_str!("../shaders/symbol.wgsl");
const EFFECT_SRC: &str = include_str!("../shaders/effect.wgsl");

/// Real-time duration one logical `tick` represents, in seconds. Must
/// match `MatrixBackground`'s own repaint-request cadence
/// (`REPAINT_MS` in `lib.rs`, currently 33ms i.e. ~30fps) rather than the
/// original Rezmason/matrix source's 60fps assumption
/// (`js/config.js`'s `fps: 60`): under normal conditions, each rendered
/// frame must represent at most one logical tick, or `symbol.wgsl`'s
/// per-cell phase-based desync (each cell's own `previousAge` spreads out
/// *when* it crosses the glyph-cycling threshold) collapses into a
/// synchronized mass reassignment whenever more than one tick's worth of
/// crossings get batched into a single visible frame. Since this port's
/// render rate is throttled below the original's 60fps, matching the
/// *render* cadence here (rather than the original's tick cadence) is
/// what keeps individual glyphs cycling independently — at the cost of
/// running roughly half as fast, absolute-speed-wise, as the original.
const NOMINAL_TICK_SECS: f32 = 1.0 / 30.0;

const FRAGMENT: wgpu::ShaderStages = wgpu::ShaderStages::FRAGMENT;

/// Size in bytes of each pass's (16-byte padded) uniform block.
const INTRO_UNIFORM_SIZE: u64 = 32;
const RAINDROP_UNIFORM_SIZE: u64 = 48;
const SYMBOL_UNIFORM_SIZE: u64 = 48;
const EFFECT_UNIFORM_SIZE: u64 = 48;

/// The 4 compute passes plus the shared bookkeeping (grid resolution,
/// frame tick counter) they all need.
pub struct ComputePasses {
    intro_pass: PassPipeline,
    intro_uniforms: UniformBuffer,
    intro_target: RenderTarget,

    raindrop_pass: PassPipeline,
    raindrop_uniforms: UniformBuffer,
    raindrop_buf: PingPong,

    symbol_pass: PassPipeline,
    symbol_uniforms: UniformBuffer,
    symbol_buf: PingPong,

    effect_pass: PassPipeline,
    effect_uniforms: UniformBuffer,
    effect_target: RenderTarget,

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
    pub fn new(device: &wgpu::Device, num_columns: u32, num_rows: u32) -> Self {
        let state_format = TargetFormat::Rgba16F.wgpu();
        let make = |label: &str, src: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
            let module = gpu::shader_module(device, label, src);
            PassPipeline::new(
                device,
                label,
                &module,
                "vs_offscreen",
                "fs_main",
                entries,
                &[],
                state_format,
                None,
            )
        };

        let intro_pass = make(
            "matrix intro",
            INTRO_SRC,
            &[gpu::uniform_entry(0, FRAGMENT)],
        );
        let raindrop_pass = make(
            "matrix raindrop",
            RAINDROP_SRC,
            &[
                gpu::uniform_entry(0, FRAGMENT),
                gpu::texture_entry(1, FRAGMENT),
                gpu::texture_entry(2, FRAGMENT),
                gpu::sampler_entry(3, FRAGMENT),
            ],
        );
        let symbol_pass = make(
            "matrix symbol",
            SYMBOL_SRC,
            &[
                gpu::uniform_entry(0, FRAGMENT),
                gpu::texture_entry(1, FRAGMENT),
                gpu::texture_entry(2, FRAGMENT),
                gpu::sampler_entry(3, FRAGMENT),
            ],
        );
        let effect_pass = make(
            "matrix effect",
            EFFECT_SRC,
            &[gpu::uniform_entry(0, FRAGMENT)],
        );

        Self {
            intro_pass,
            intro_uniforms: UniformBuffer::new(device, "matrix intro", INTRO_UNIFORM_SIZE),
            intro_target: RenderTarget::new(device, num_columns, 1, TargetFormat::Rgba16F),
            raindrop_pass,
            raindrop_uniforms: UniformBuffer::new(device, "matrix raindrop", RAINDROP_UNIFORM_SIZE),
            raindrop_buf: PingPong::new(device, num_columns, num_rows, TargetFormat::Rgba16F),
            symbol_pass,
            symbol_uniforms: UniformBuffer::new(device, "matrix symbol", SYMBOL_UNIFORM_SIZE),
            symbol_buf: PingPong::new(device, num_columns, num_rows, TargetFormat::Rgba16F),
            effect_pass,
            effect_uniforms: UniformBuffer::new(device, "matrix effect", EFFECT_UNIFORM_SIZE),
            effect_target: RenderTarget::new(device, num_columns, num_rows, TargetFormat::Rgba16F),
            num_columns,
            num_rows,
            tick: 0,
            last_tick_time: 0.0,
        }
    }

    /// Resets the tick counter, forcing every cell through the
    /// `is_first_frame`/`reset_glyph` path on the next `run()` — cheaply
    /// re-rolling each cell's stored `symbol` index fresh against
    /// whatever `glyph_sequence_length` is current. Must be called whenever
    /// a change makes the previously-stored per-cell `symbol_buf` state
    /// stale: switching to a font with a different `glyph_sequence_length`
    /// (see `passes/mod.rs`'s font-switch branch) reuses the ping-pong
    /// buffer's old indices, which were only ever drawn from the
    /// *previous* font's (possibly much smaller) range — reinterpreted
    /// against a larger atlas, those stale low-value indices all land in
    /// the same narrow corner of the new grid, showing as a mass of
    /// near-identical glyphs until each cell happens to cycle again on its
    /// own schedule.
    pub fn reset_tick(&mut self) {
        self.tick = 0;
        self.last_tick_time = 0.0;
    }

    /// Recreates the compute buffers if the grid resolution changed
    /// (driven by `num_columns` and, in volumetric mode, `density` — see
    /// `passes/mod.rs` — non-volumetric mode always has `num_rows ==
    /// num_columns`, matching the original: the render pass maps this
    /// square grid onto the possibly-non-square viewport via a
    /// `screen_size` stretch/crop instead of a non-square grid), matching
    /// the original's own resize behavior of losing transient state on a
    /// resolution change.
    pub fn resize(&mut self, device: &wgpu::Device, num_columns: u32, num_rows: u32) {
        if num_columns == self.num_columns && num_rows == self.num_rows {
            return;
        }
        self.intro_target.resize(device, num_columns, 1);
        self.raindrop_buf.resize(device, num_columns, num_rows);
        self.symbol_buf.resize(device, num_columns, num_rows);
        self.effect_target.resize(device, num_columns, num_rows);
        self.num_columns = num_columns;
        self.num_rows = num_rows;
        // The freshly (re)created buffers above start blank (every
        // cell's `previousAge`/`previousSymbol` reads back as 0)
        // rather than through the proper per-cell random reset path,
        // which only fires when `is_first_frame` is true — without this,
        // every cell would start in lockstep (same age, same symbol)
        // instead of independently phased. See `reset_tick`'s doc
        // comment for the same underlying "stale/invalid state must
        // trigger a reset" reasoning.
        self.reset_tick();
    }

    /// Records intro -> raindrop -> symbol -> effect, then swaps the
    /// ping-pong buffers for next frame. `raindrop_state()`,
    /// `symbol_state()`, and `effect_state()` are valid to sample
    /// (reflecting this frame's freshly computed state) once the encoder
    /// has been submitted, until the next call to `run`.
    ///
    /// This is a no-op (no draws, no ping-pong swaps, no state mutation —
    /// `raindrop_state()` etc. keep returning whatever they returned last
    /// time) if no new logical tick has elapsed since the previous call to
    /// `run`. This matters because `cycle_frame_skip == 1` (the default)
    /// makes the glyph-cycling gate a no-op — it's always true — so
    /// without this early return, calling `run` more often than the
    /// nominal tick rate (e.g. extra repaints `eframe` forces on mouse
    /// movement) would advance the glyph-cycling `age` accumulator once
    /// per *call* rather than once per elapsed tick, exactly the bug this
    /// whole tick-pacing mechanism exists to prevent. Skipping only this
    /// pass (not the render/bloom/effect passes downstream, which still
    /// redraw every call) avoids a missing-frame flicker, since unlike the
    /// original's persistent browser canvas, egui repaints the whole
    /// window every call and expects a fresh frame back.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        samplers: &Samplers,
        config: &MatrixConfig,
        time: f32,
        glyph_sequence_length: u32,
    ) {
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

        // --- intro ---
        let mut u = Uniforms::new();
        u.f32(time)
            .f32(tick)
            .f32(config.animation_speed)
            .f32(config.fall_speed)
            .f32(num_columns)
            .f32(num_rows)
            .bool(config.skip_intro);
        self.intro_uniforms.write(queue, u);
        let bind_group = self.intro_pass.bind_group(
            device,
            &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.intro_uniforms.binding(),
            }],
        );
        gpu::draw_fullscreen(
            encoder,
            &self.intro_target.view,
            &self.intro_pass,
            &bind_group,
        );

        // --- raindrop ---
        let mut u = Uniforms::new();
        u.f32(time)
            .f32(tick)
            .f32(config.animation_speed)
            .f32(config.fall_speed)
            .f32(num_columns)
            .f32(num_rows)
            .f32(effective_brightness_decay)
            .f32(config.raindrop_length)
            .bool(false)
            .bool(config.skip_intro);
        self.raindrop_uniforms.write(queue, u);
        let bind_group = self.raindrop_pass.bind_group(
            device,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.raindrop_uniforms.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.raindrop_buf.read().view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.intro_target.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&samplers.nearest),
                },
            ],
        );
        gpu::draw_fullscreen(
            encoder,
            &self.raindrop_buf.write().view,
            &self.raindrop_pass,
            &bind_group,
        );
        self.raindrop_buf.swap();

        // --- symbol ---
        let mut u = Uniforms::new();
        u.f32(time)
            .f32(tick)
            .f32(config.cycle_frame_skip as f32)
            .f32(config.animation_speed)
            .f32(config.cycle_speed)
            .f32(gate_event_count)
            .f32(num_columns)
            .f32(num_rows)
            .f32(glyph_sequence_length as f32)
            .bool(false);
        self.symbol_uniforms.write(queue, u);
        let bind_group = self.symbol_pass.bind_group(
            device,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.symbol_uniforms.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.symbol_buf.read().view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.raindrop_buf.read().view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&samplers.nearest),
                },
            ],
        );
        gpu::draw_fullscreen(
            encoder,
            &self.symbol_buf.write().view,
            &self.symbol_pass,
            &bind_group,
        );
        self.symbol_buf.swap();

        // --- effect ---
        let ripple_type = match config.ripple_type {
            None => -1,
            Some(RippleType::Box) => 0,
            Some(RippleType::Circle) => 1,
        };
        let mut u = Uniforms::new();
        u.f32(time)
            .f32(tick)
            .f32(config.animation_speed)
            .f32(num_columns)
            .f32(num_rows)
            .f32(config.glyph_height_to_width)
            .f32(config.ripple_scale)
            .f32(config.ripple_speed)
            .f32(config.ripple_thickness)
            .i32(ripple_type)
            .bool(config.has_thunder)
            .bool(false);
        self.effect_uniforms.write(queue, u);
        let bind_group = self.effect_pass.bind_group(
            device,
            &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.effect_uniforms.binding(),
            }],
        );
        gpu::draw_fullscreen(
            encoder,
            &self.effect_target.view,
            &self.effect_pass,
            &bind_group,
        );
    }

    pub fn raindrop_state(&self) -> &wgpu::TextureView {
        &self.raindrop_buf.read().view
    }

    pub fn symbol_state(&self) -> &wgpu::TextureView {
        &self.symbol_buf.read().view
    }

    pub fn effect_state(&self) -> &wgpu::TextureView {
        &self.effect_target.view
    }
}

/// Given the current tick counter and the `time` value as of the last tick
/// advancement, plus this call's `time`, returns the updated
/// `(tick, last_tick_time)`. Advances `tick` by however many
/// `NOMINAL_TICK_SECS`-sized chunks of real elapsed time have accumulated
/// since the last advancement (0 for a burst of near-zero-dt extra calls,
/// 1 for steady nominal-rate calls, more than 1 to catch up after a
/// stall/lag spike) instead of once per call. This keeps glyph-cycling
/// (gated on `tick` in `symbol.wgsl`) paced by real time rather than
/// by how many times `run()` happens to be invoked — some hosts (e.g.
/// `eframe`'s native backend) force extra repaints on input events like
/// mouse movement, which would otherwise make `tick`-gated animation speed
/// up during those bursts.
fn advance_tick(tick: u32, last_tick_time: f32, time: f32) -> (u32, f32) {
    if tick == 0 {
        // First call ever: seed tick at 1 unconditionally so `isFirstFrame`
        // (`tick <= 1.` in raindrop.wgsl / symbol.wgsl) still
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
/// `(previous_tick, tick]` — i.e. how many times `symbol.wgsl`'s
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
