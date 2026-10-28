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
        })
    }

    /// Records every offscreen pass for this frame into `encoder` and
    /// prepares the final composite for [`Passes::paint`].
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
