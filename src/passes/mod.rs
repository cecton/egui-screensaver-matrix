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

/// Minimum spacing between two recorded frames, in seconds: the animation
/// only advances once per [`crate::REPAINT_MS`], so anything sooner would
/// rebuild the same image.
///
/// Relaxed to 3/4 of a tick because the host's repaint timer does not land
/// exactly on the nominal interval: a host already running at the tick rate
/// delivers frames slightly early as often as slightly late, and a strict
/// comparison would skip every other one and halve the animation to 15 fps.
/// 3/4 still skips the second frame of a host that renders twice per tick.
const RECORD_INTERVAL_SECS: f32 = crate::REPAINT_MS as f32 / 1000.0 * 0.75;

/// Whether a frame painted at `time` should re-record the offscreen chain,
/// given the `time` of the last frame that did (`None` before the first).
///
/// Left un-skipped for a non-positive elapsed time: that means the host's
/// clock restarted (a fresh `egui::Context` starts `time` over at zero), so
/// recording — rather than trusting an image from an older clock — keeps a
/// cached frame from going stale forever.
fn should_record(last_recorded_at: Option<f32>, time: f32) -> bool {
    let Some(last) = last_recorded_at else {
        return true;
    };
    !(0.0..RECORD_INTERVAL_SECS).contains(&(time - last))
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
    /// The `time` value of the last frame that recorded offscreen work, or
    /// `None` before the first one. [`Passes::prepare`] skips the whole
    /// offscreen chain while less than one [`crate::REPAINT_MS`] has
    /// elapsed since this, since the image such a frame would produce is
    /// the one already sitting in the targets.
    last_recorded_at: Option<f32>,
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
            last_recorded_at: None,
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
    /// switch) clears `last_recorded_at` so the next call always records.
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
            self.last_recorded_at = None;
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
                self.last_recorded_at = None;
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
            self.last_recorded_at = None;
        }

        if !should_record(self.last_recorded_at, time) {
            return;
        }
        self.last_recorded_at = Some(time);

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
    use super::should_record;

    /// The whole point of the skip: a repaint that lands less than one tick
    /// after the last recorded frame must not re-run the offscreen chain.
    #[test]
    fn frame_one_frame_after_a_recorded_one_is_skipped() {
        // Two frames of a 60 fps host, against a 33 ms tick.
        assert!(!should_record(Some(0.0), 0.016));
        assert!(!should_record(Some(0.016), 0.032));
    }

    #[test]
    fn first_frame_always_records() {
        assert!(should_record(None, 0.0));
        assert!(should_record(None, 12.5));
    }

    #[test]
    fn a_full_tick_or_more_records() {
        assert!(should_record(Some(0.0), 0.033));
        assert!(should_record(Some(0.0), 0.05));
        // A stalled host catches up rather than staying frozen.
        assert!(should_record(Some(0.0), 30.0));
    }

    /// A clock that stands still skips (the animation's own time comes from
    /// that clock, so it has not advanced either), but a clock that goes
    /// *backwards* — `egui::Context::default()` starts `time` over at zero —
    /// records, so an image drawn against the old clock can never be trusted
    /// as current forever.
    #[test]
    fn a_still_clock_skips_but_a_restarted_one_records() {
        assert!(!should_record(Some(5.0), 5.0));
        assert!(should_record(Some(5.0), 0.0));
        assert!(should_record(Some(5.0), -1.0));
    }
}
