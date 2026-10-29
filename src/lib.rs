//! Matrix digital rain screensaver for [egui](https://github.com/emilk/egui),
//! ported from [Rezmason's Matrix](https://github.com/Rezmason/matrix)
//! WebGL original.
//!
//! Like the original, this runs the whole simulation on the GPU: ping-pong
//! "compute" render targets drive the falling-glyph animation, an MSDF
//! font atlas renders the glyphs, and a bloom pass plus a palette/stripe
//! color-effect pass finish the look — all through an
//! [`egui::PaintCallback`] via [`egui_wgpu`]. **This requires the host
//! `eframe::App` to use the `wgpu` rendering backend** (not `glow`) —
//! [`MatrixBackground::paint`] simply skips drawing if no
//! [`egui_wgpu::RenderState`] is available.
//!
//! The final pass targets the render pass eframe sets up by default: no
//! depth buffer and no multisampling.
//!
//! # Usage
//!
//! ```rust,no_run
//! use egui_screensaver_matrix::MatrixBackground;
//!
//! struct MyApp {
//!     matrix: MatrixBackground,
//! }
//!
//! impl eframe::App for MyApp {
//!     fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
//!         let ctx = ui.ctx().clone();
//!         // Call paint once per frame before drawing any UI windows so the
//!         // screensaver sits on the background layer behind everything else.
//!         self.matrix.paint(&ctx, frame.wgpu_render_state());
//!     }
//! }
//! ```

mod camera;
mod config;
pub mod fonts;
mod geometry;
mod gpu;
mod passes;
pub mod textures;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, Context, LayerId, Painter, Shape};
use egui_wgpu::{CallbackTrait, RenderState, ScreenDescriptor, wgpu};

pub use config::{Effect, MatrixConfig, PaletteStop, Preset, RippleType};
pub use fonts::FontId;
pub use textures::DecorativeTexture;

/// How often the animation is meant to advance, in milliseconds. Also the
/// minimum spacing between two genuinely re-rendered frames: a repaint that
/// arrives sooner than this (the host app's own timers interleave with
/// `request_repaint_after`, so it renders at whatever rate its widgets and
/// the compositor ask for) re-draws the previous frame instead of running
/// the whole offscreen chain again — see `Passes::prepare`.
pub(crate) const REPAINT_MS: u64 = 33;

/// Matrix screensaver state.
///
/// Create one instance (e.g. as a field of your `eframe::App` struct) and
/// call [`MatrixBackground::paint`] every frame from your `ui` method.
pub struct MatrixBackground {
    /// Every tunable parameter (rain speed, brightness, glyph shape, ...).
    /// See [`MatrixConfig`]'s field docs.
    pub config: MatrixConfig,
    time: f32,
    last_time: Option<f64>,
    passes: Option<Arc<Mutex<passes::Passes>>>,
}

impl MatrixBackground {
    /// Create a new matrix state with the given configuration.
    pub fn new(config: MatrixConfig) -> Self {
        Self {
            config,
            time: 0.0,
            last_time: None,
            passes: None,
        }
    }

    /// Paint the screensaver onto the egui background layer for this frame.
    ///
    /// Call this once per frame **before** drawing any UI panels or windows
    /// so the animation appears behind all other content. `render_state`
    /// should come from `eframe::Frame::wgpu_render_state()` — if it's
    /// `None` (e.g. the host app uses the `glow` backend instead of `wgpu`),
    /// this draws nothing but a black background.
    pub fn paint(&mut self, ctx: &Context, render_state: Option<&RenderState>) {
        ctx.request_repaint_after(Duration::from_millis(REPAINT_MS));

        let now = ctx.input(|i| i.time);
        let dt = match self.last_time {
            Some(last) => (now - last).clamp(0.0, 0.1) as f32,
            None => 0.0,
        };
        self.last_time = Some(now);
        // Animation speed is applied inside the shaders themselves
        // (`simTime = time * animationSpeed`), so this stays a plain
        // unscaled clock — applying `animation_speed` here too would
        // double it up.
        self.time += dt;

        if self.passes.is_none()
            && let Some(render_state) = render_state
        {
            // Constructed at a placeholder 1x1 size — `Passes::prepare`'s
            // resize check fixes this up to the real screen size before
            // the first frame is drawn (real pixel dimensions are only
            // available from the `ScreenDescriptor`, later).
            match passes::Passes::new(
                &render_state.device,
                &render_state.queue,
                render_state.target_format,
                &self.config,
                1,
                1,
            ) {
                Ok(passes) => self.passes = Some(Arc::new(Mutex::new(passes))),
                Err(err) => log::error!("egui-screensaver-matrix: GPU setup failed: {err}"),
            }
        }

        let screen_rect = ctx.viewport_rect();
        let painter = Painter::new(ctx.clone(), LayerId::background(), screen_rect);
        painter.rect_filled(screen_rect, 0.0, Color32::BLACK);

        if let Some(passes) = self.passes.clone() {
            painter.add(Shape::Callback(egui_wgpu::Callback::new_paint_callback(
                screen_rect,
                MatrixCallback {
                    passes,
                    config: self.config.clone(),
                    time: self.time,
                },
            )));
        }
    }
}

/// The per-frame [`CallbackTrait`] handed to egui: records every offscreen
/// pass in `prepare` and draws the final composite in `paint`.
struct MatrixCallback {
    passes: Arc<Mutex<passes::Passes>>,
    config: MatrixConfig,
    time: f32,
}

impl CallbackTrait for MatrixCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Ok(mut passes) = self.passes.lock() {
            passes.prepare(
                device,
                queue,
                egui_encoder,
                &self.config,
                self.time,
                screen_descriptor.size_in_pixels,
            );
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        if let Ok(passes) = self.passes.lock() {
            passes.paint(render_pass);
        }
    }
}
