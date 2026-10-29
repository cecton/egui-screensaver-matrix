pub mod bloom;
pub mod compute;
pub mod effect;
pub mod render;

use egui_wgpu::wgpu;

use crate::config::MatrixConfig;
use crate::fonts::FontId;
use crate::gpu::{RenderTarget, Samplers, TargetFormat};
use bloom::BloomPasses;
use compute::ComputePasses;
use effect::EffectPasses;
use render::RenderPass;

/// The compute/render grid's column and row counts for a given config.
/// Non-volumetric mode is always square (`num_columns` x `num_columns`);
/// volumetric mode scales the column count by `density` (allowing
/// overlapping columns for a denser look) while rows stay at
/// `num_columns` — ported from `js/regl/rainPass.js`'s
/// `[numRows, numColumns] = [config.numColumns, floor(config.numColumns * density)]`.
fn grid_dimensions(config: &MatrixConfig) -> (u32, u32) {
    if config.volumetric {
        let columns = ((config.num_columns as f32) * config.density).floor() as u32;
        (columns.max(1), config.num_columns)
    } else {
        (config.num_columns, config.num_columns)
    }
}

/// Width of one record bucket, in seconds: the animation only advances once
/// per [`crate::REPAINT_MS`].
const RECORD_INTERVAL_SECS: f32 = crate::REPAINT_MS as f32 / 1000.0;

/// Decides which frames re-run the offscreen chain, given the animation
/// `time` each repaint reports.
///
/// The animation only needs a new image once per [`RECORD_INTERVAL_SECS`],
/// while the host repaints at whatever rate its own timers and the
/// compositor ask for: ~60 fps normally, but faster whenever egui has been
/// asked to repaint for something else — pointer motion is the big one,
/// since every mouse event wakes the loop.
///
/// Frames are therefore binned into fixed [`RECORD_INTERVAL_SECS`] buckets
/// of animation time, and only the first frame of each bucket records. That
/// caps the chain at one run per tick whatever the host does above it, and
/// keeps the runs on an even cadence. A "one interval since the last
/// record" test can do neither: it has to be relaxed below the nominal
/// interval to survive the host's timer jitter (a host at exactly the tick
/// rate delivers frames slightly early as often as slightly late), and the
/// relaxation then also admits a host repainting every 8 ms — which records
/// several times per tick. Those extra runs are not neutral here: every
/// pass animates from `time` directly, so the picture would update ~40
/// rather than 30 times a second on pointer motion.
#[derive(Default)]
struct RecordGate {
    /// Index of the bucket the last recorded frame fell in.
    last_bucket: Option<i64>,
}

impl RecordGate {
    /// Records on the next call whatever the bucket says — for anything
    /// that invalidates the cached image outright (a resize, a font switch,
    /// a recreated target).
    fn force(&mut self) {
        self.last_bucket = None;
    }

    fn should_record(&mut self, time: f32) -> bool {
        // The bucket an absolute `time` falls in. Quantising the *animation*
        // clock rather than measuring gaps between records is what makes the
        // rate independent of how often the host repaints.
        let bucket = (f64::from(time) / f64::from(RECORD_INTERVAL_SECS)).floor() as i64;
        if self.last_bucket == Some(bucket) {
            return false;
        }
        self.last_bucket = Some(bucket);
        true
    }
}

/// Owns every GPU resource for one frame's worth of work: the compute
/// passes, the main glyph-render pass (into an offscreen primary target),
/// the bloom pass, and the final palette/stripe effect pass.
///
/// A frame runs in two halves: [`Passes::prepare`] records every offscreen
/// pass into the encoder egui hands to `CallbackTrait::prepare`, and
/// [`Passes::paint`] draws the final composite into egui's render pass.
pub struct Passes {
    samplers: Samplers,
    compute: ComputePasses,
    render: RenderPass,
    bloom: BloomPasses,
    effect: EffectPasses,
    primary: RenderTarget,
    grid_columns: u32,
    grid_rows: u32,
    current_font: FontId,
    screen_width: u32,
    screen_height: u32,
    bloom_size: f32,
    /// Decides which frames actually re-run the offscreen chain.
    gate: RecordGate,
}

impl Passes {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        config: &MatrixConfig,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<Self, String> {
        let samplers = Samplers::new(device);
        let font = config.font.atlas();
        let (grid_columns, grid_rows) = grid_dimensions(config);
        let compute = ComputePasses::new(device, grid_columns, grid_rows);
        let render = RenderPass::new(device, queue, font)?;
        let primary = RenderTarget::new(device, screen_width, screen_height, TargetFormat::Rgba8);
        let bloom = BloomPasses::new(
            device,
            queue,
            &samplers,
            &primary.view,
            screen_width,
            screen_height,
            config.bloom_size,
        );
        let effect = EffectPasses::new(device, queue, target_format, config);
        Ok(Self {
            samplers,
            compute,
            render,
            bloom,
            effect,
            primary,
            grid_columns,
            grid_rows,
            current_font: config.font,
            screen_width,
            screen_height,
            bloom_size: config.bloom_size,
            gate: RecordGate::default(),
        })
    }

    /// Records every offscreen pass for this frame into `encoder` and
    /// prepares the final composite for [`Passes::paint`].
    ///
    /// Skips the whole offscreen chain (leaving the previous frame's image
    /// in place for [`Passes::paint`] to blit) when less than one
    /// [`crate::REPAINT_MS`] has elapsed since the last recorded frame: the
    /// animation only advances that often, and the host app repaints at
    /// whatever rate its own timers and the compositor ask for — commonly
    /// around 60 fps, i.e. twice per tick, which would otherwise run this
    /// ~10-pass chain twice per image with nothing to show for it. Anything
    /// that invalidates the cached image (a resize, a new bloom size, a font
    /// switch) calls [`RecordGate::force`] so the next call always records.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        config: &MatrixConfig,
        time: f32,
        screen_size_px: [u32; 2],
    ) {
        let (grid_columns, grid_rows) = grid_dimensions(config);
        if grid_columns != self.grid_columns || grid_rows != self.grid_rows {
            self.compute.resize(device, grid_columns, grid_rows);
            self.grid_columns = grid_columns;
            self.grid_rows = grid_rows;
            // The ping-pong state was just thrown away; the cached image is
            // stale regardless of how long ago it was recorded.
            self.gate.force();
        }

        if config.font != self.current_font {
            let font = config.font.atlas();
            if let Err(err) = self.render.set_font(device, queue, font) {
                log::error!("egui-screensaver-matrix: failed to switch font: {err}");
            } else {
                // Cells' stored `symbol` indices were drawn from the
                // *old* font's glyphSequenceLength range; reinterpreted
                // against the new (possibly larger) atlas they'd all
                // cluster into its low-index corner until each cell
                // happens to cycle again on its own. Force everyone to
                // re-roll now instead. See `reset_tick`'s doc comment.
                self.compute.reset_tick();
                self.current_font = config.font;
                self.gate.force();
            }
        }

        let [screen_width, screen_height] = screen_size_px.map(|v| v.max(1));
        if screen_width != self.screen_width
            || screen_height != self.screen_height
            || config.bloom_size != self.bloom_size
        {
            self.primary.resize(device, screen_width, screen_height);
            self.bloom.resize(
                device,
                queue,
                &self.samplers,
                &self.primary.view,
                screen_width,
                screen_height,
                config.bloom_size,
            );
            self.screen_width = screen_width;
            self.screen_height = screen_height;
            self.bloom_size = config.bloom_size;
            // Fresh (cleared) targets and a bind group still pointing at the
            // old views: record unconditionally this frame.
            self.gate.force();
        }

        if !self.gate.should_record(time) {
            return;
        }

        self.compute.run(
            device,
            queue,
            encoder,
            &self.samplers,
            config,
            time,
            self.render.glyph_sequence_length(),
        );

        self.render.draw(
            device,
            queue,
            encoder,
            &self.samplers,
            config,
            time,
            self.grid_columns,
            self.grid_rows,
            [
                self.compute.raindrop_state(),
                self.compute.symbol_state(),
                self.compute.effect_state(),
            ],
            &self.primary.view,
            screen_width,
            screen_height,
        );

        self.bloom.run(queue, encoder, config);

        self.effect.prepare(
            device,
            queue,
            &self.samplers,
            config,
            time,
            &self.primary.view,
            self.bloom.output(),
        );
    }

    /// Draws the final composite into egui's render pass.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        self.effect.paint(render_pass);
    }
}

#[cfg(test)]
mod tests {
    use super::{RECORD_INTERVAL_SECS, RecordGate};

    /// Drives the gate with a host repainting every `cadence` seconds for
    /// `seconds`, returning the times that recorded.
    fn drive(cadence: f64, seconds: f64, jitter: f64) -> Vec<f64> {
        let mut gate = RecordGate::default();
        let mut t = 0.0;
        let mut records = Vec::new();
        let mut i = 0.0f64;
        while t < seconds {
            if gate.should_record(t as f32) {
                records.push(t);
            }
            i += 1.0;
            t += cadence + jitter * (i * 1.7).sin();
        }
        records
    }

    fn tick_rate() -> f64 {
        1.0 / f64::from(RECORD_INTERVAL_SECS)
    }

    fn rate(cadence: f64, seconds: f64, jitter: f64) -> f64 {
        drive(cadence, seconds, jitter).len() as f64 / seconds
    }

    /// The whole point: however fast the host repaints above the tick rate,
    /// the chain runs at the tick rate and no faster. Every pass animates
    /// from `time` directly, so running it more often would visibly speed
    /// the animation up — which is exactly what pointer motion does to a
    /// host that repaints on every mouse event.
    #[test]
    fn any_fast_host_records_at_the_tick_rate() {
        for fps in [60.0, 61.0, 75.0, 90.0, 120.0, 144.0, 165.0, 240.0] {
            let got = rate(1.0 / fps, 20.0, 0.0);
            assert!(
                (got - tick_rate()).abs() < 1.0,
                "host at {fps} fps recorded {got}/s, expected ~{}/s",
                tick_rate()
            );
        }
    }

    /// A host slower than the tick rate records every frame: there is
    /// nothing to skip.
    #[test]
    fn a_slow_host_records_every_frame() {
        for fps in [30.0, 25.0, 24.0, 20.0, 15.0] {
            let got = rate(1.0 / fps, 20.0, 0.0);
            assert!(
                (got - fps).abs() < 0.5,
                "host at {fps} fps recorded {got}/s, expected {fps}/s"
            );
        }
    }

    /// The rate holds under the host's timer jitter, which is what the
    /// bucket boundaries make free: a frame that arrives a millisecond
    /// either side of a boundary lands in whichever bucket it lands in.
    #[test]
    fn the_rate_holds_under_jitter() {
        for (fps, jitter) in [(60.0, 0.0), (61.0, 0.0006), (144.0, 0.0006)] {
            let got = rate(1.0 / fps, 20.0, jitter);
            assert!(
                (got - tick_rate()).abs() < 1.0,
                "host at {fps} fps recorded {got}/s, expected ~{}/s",
                tick_rate()
            );
        }
    }

    /// Two repaints inside the same tick record once, not twice. This is the
    /// regression the gate exists to prevent.
    #[test]
    fn two_repaints_inside_one_tick_record_once() {
        let mut gate = RecordGate::default();
        let tick = RECORD_INTERVAL_SECS;
        assert!(gate.should_record(0.0));
        // An 8 ms host: several repaints before the next tick boundary.
        assert!(!gate.should_record(tick * 0.25));
        assert!(!gate.should_record(tick * 0.5));
        assert!(!gate.should_record(tick * 0.75));
        assert!(gate.should_record(tick * 1.01));
    }

    /// The first frame records whenever it arrives.
    #[test]
    fn the_first_frame_always_records() {
        let mut gate = RecordGate::default();
        assert!(gate.should_record(12.5));
        assert!(!gate.should_record(12.5 + RECORD_INTERVAL_SECS * 0.2));
    }

    /// A stall of several ticks records once on return, then carries on.
    #[test]
    fn a_stall_records_once() {
        let mut gate = RecordGate::default();
        assert!(gate.should_record(0.0));
        assert!(gate.should_record(1.0));
        assert!(!gate.should_record(1.0 + RECORD_INTERVAL_SECS * 0.2));
    }

    /// `force` (a resize, a font switch) makes the very next frame record
    /// even though its bucket has already been recorded.
    #[test]
    fn force_records_the_next_frame() {
        let mut gate = RecordGate::default();
        assert!(gate.should_record(0.0));
        assert!(!gate.should_record(0.01));
        gate.force();
        assert!(gate.should_record(0.011));
    }
}
