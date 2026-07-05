pub mod bloom;
pub mod compute;
pub mod effect;
pub mod render;

use std::sync::Arc;

use glow::HasContext as _;

use crate::config::MatrixConfig;
use crate::fonts::FontId;
use crate::gl_util::{RenderTarget, TargetFilter, TargetFormat};
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

/// Owns every GPU resource for one frame's worth of work: the 4 ping-pong
/// compute passes, the main glyph-render pass (into an offscreen primary
/// target), the bloom pass, and the final palette/stripe effect pass.
/// `run` executes the full per-frame sequence, leaving the final image
/// drawn into whatever framebuffer is bound when it's called (the caller
/// — [`crate::MatrixBackground`] — is responsible for making sure that's
/// the right target, since this crate has no portable way to query/
/// restore an arbitrary previous framebuffer binding across the native/
/// WebGL backends; see `lib.rs`).
pub struct Passes {
    compute: ComputePasses,
    render: RenderPass,
    bloom: BloomPasses,
    effect: EffectPasses,
    primary: RenderTarget,
    grid_columns: u32,
    grid_rows: u32,
    current_font: FontId,
    screen_width: i32,
    screen_height: i32,
    bloom_size: f32,
}

impl Passes {
    pub unsafe fn new(
        gl: &Arc<glow::Context>,
        config: &MatrixConfig,
        screen_width: i32,
        screen_height: i32,
    ) -> Result<Self, String> {
        unsafe {
            let header = crate::gl_util::shader_header(gl);
            let font = config.font.atlas();
            let (grid_columns, grid_rows) = grid_dimensions(config);
            let compute = ComputePasses::new(gl, &header, grid_columns, grid_rows, font)?;
            let render = RenderPass::new(gl, &header, font)?;
            let bloom =
                BloomPasses::new(gl, &header, screen_width, screen_height, config.bloom_size)?;
            let effect = EffectPasses::new(gl, &header, config)?;
            let primary = RenderTarget::new(
                gl,
                screen_width,
                screen_height,
                TargetFormat::Rgba8,
                TargetFilter::Linear,
            )?;
            Ok(Self {
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
    }

    pub unsafe fn run(
        &mut self,
        gl: &glow::Context,
        config: &MatrixConfig,
        time: f32,
        screen_size_px: [u32; 2],
    ) {
        unsafe {
            let (grid_columns, grid_rows) = grid_dimensions(config);
            if grid_columns != self.grid_columns || grid_rows != self.grid_rows {
                if let Err(err) = self.compute.resize(gl, grid_columns, grid_rows) {
                    log::error!("egui-screensaver-matrix: failed to resize compute buffers: {err}");
                } else {
                    self.grid_columns = grid_columns;
                    self.grid_rows = grid_rows;
                }
            }

            if config.font != self.current_font {
                let font = config.font.atlas();
                if let Err(err) = self.render.set_font(gl, font) {
                    log::error!("egui-screensaver-matrix: failed to switch font: {err}");
                } else {
                    self.compute
                        .set_glyph_sequence_length(gl, font.glyph_sequence_length);
                    self.current_font = config.font;
                }
            }

            let screen_width = screen_size_px[0] as i32;
            let screen_height = screen_size_px[1] as i32;
            if screen_width != self.screen_width
                || screen_height != self.screen_height
                || config.bloom_size != self.bloom_size
            {
                if let Err(err) = self.primary.resize(gl, screen_width, screen_height) {
                    log::error!("egui-screensaver-matrix: failed to resize primary target: {err}");
                }
                if let Err(err) =
                    self.bloom
                        .resize(gl, screen_width, screen_height, config.bloom_size)
                {
                    log::error!("egui-screensaver-matrix: failed to resize bloom pyramid: {err}");
                }
                self.screen_width = screen_width;
                self.screen_height = screen_height;
                self.bloom_size = config.bloom_size;
            }

            self.compute.run(gl, config, time);

            self.primary.bind(gl);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            self.render.draw(
                gl,
                config,
                time,
                self.grid_columns,
                self.grid_rows,
                self.compute.raindrop_state(),
                self.compute.symbol_state(),
                self.compute.effect_state(),
                screen_width,
                screen_height,
            );

            let bloom_texture = self.bloom.run(gl, config, self.primary.texture);

            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, screen_width, screen_height);
            self.effect
                .run(gl, config, time, self.primary.texture, bloom_texture);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.compute.destroy(gl);
            self.render.destroy(gl);
            self.bloom.destroy(gl);
            self.effect.destroy(gl);
            self.primary.destroy(gl);
        }
    }
}
