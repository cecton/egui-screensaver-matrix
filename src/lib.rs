//! Matrix digital rain screensaver for [egui](https://github.com/emilk/egui),
//! ported from [Rezmason's Matrix](https://github.com/Rezmason/matrix)
//! WebGL original.
//!
//! Like the original, this runs the whole simulation on the GPU: ping-pong
//! "compute" render targets drive the falling-glyph animation, an MSDF
//! font atlas renders the glyphs, and a bloom pass plus a palette/stripe
//! color-effect pass finish the look — all through an
//! [`egui::PaintCallback`] via [`egui_glow`]. **This requires the host
//! `eframe::App` to use the `glow` rendering backend** (not `wgpu`) —
//! [`MatrixBackground::paint`] simply skips drawing if no `glow::Context`
//! is available. Volumetric 3D mode is not implemented yet.
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
//!         self.matrix.paint(&ctx, frame.gl());
//!     }
//! }
//! ```

mod camera;
mod config;
pub mod fonts;
mod geometry;
mod gl_util;
mod passes;
pub mod textures;

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use egui::{Color32, Context, LayerId, Painter, Shape};
use egui_glow::CallbackFn;

pub use config::{Effect, MatrixConfig, PaletteStop, Preset, RippleType};
pub use fonts::FontId;
pub use textures::DecorativeTexture;

const REPAINT_MS: u64 = 33;

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
    passes: Option<Arc<PassesHandle>>,
}

/// Wraps [`passes::Passes`] together with the `glow::Context` needed to
/// tear it down, mirroring `egui-screensaver-flame`'s `GlResources`. The
/// `RefCell` gives `Passes::run` (which needs `&mut self` to update its
/// ping-pong buffers) interior mutability despite only being reachable
/// through a shared `Arc` from inside the paint callback — sound because,
/// like [`AssertSendSync`] below, this only ever runs single-threaded.
struct PassesHandle {
    gl: Arc<glow::Context>,
    passes: RefCell<passes::Passes>,
}

impl Drop for PassesHandle {
    fn drop(&mut self) {
        unsafe {
            self.passes.borrow().destroy(&self.gl);
        }
    }
}

impl Default for MatrixBackground {
    fn default() -> Self {
        Self {
            config: MatrixConfig::default(),
            time: 0.0,
            last_time: None,
            passes: None,
        }
    }
}

impl MatrixBackground {
    /// Paint the screensaver onto the egui background layer for this frame.
    ///
    /// Call this once per frame **before** drawing any UI panels or windows
    /// so the animation appears behind all other content. `gl` should come
    /// from `eframe::Frame::gl()` — if it's `None` (e.g. the host app uses
    /// the `wgpu` backend instead of `glow`), this draws nothing but a black
    /// background.
    pub fn paint(&mut self, ctx: &Context, gl: Option<&Arc<glow::Context>>) {
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
            && let Some(gl) = gl
        {
            // Constructed at a placeholder 1x1 size — `Passes::run`'s
            // resize check fixes this up to the real screen size before
            // the first frame is drawn (real pixel dimensions are only
            // available from `PaintCallbackInfo::screen_size_px`, later).
            match unsafe { passes::Passes::new(gl, &self.config, 1, 1) } {
                // On wasm32, `Passes` (via `glow::Context`) isn't `Sync` —
                // see `AssertSendSync` below, applied where this `Arc` is
                // captured into the `Fn + Sync + Send` paint callback.
                #[allow(clippy::arc_with_non_send_sync)]
                Ok(passes) => {
                    self.passes = Some(Arc::new(PassesHandle {
                        gl: gl.clone(),
                        passes: RefCell::new(passes),
                    }))
                }
                Err(err) => log::error!("egui-screensaver-matrix: GL setup failed: {err}"),
            }
        }

        let screen_rect = ctx.viewport_rect();
        let painter = Painter::new(ctx.clone(), LayerId::background(), screen_rect);
        painter.rect_filled(screen_rect, 0.0, Color32::BLACK);

        if let Some(handle) = self.passes.clone() {
            let time = self.time;
            let config = self.config.clone();
            let handle = AssertSendSync(handle);
            painter.add(Shape::Callback(egui::PaintCallback {
                rect: screen_rect,
                callback: Arc::new(CallbackFn::new(move |info, _painter| {
                    let handle = handle.get();
                    unsafe {
                        handle.passes.borrow_mut().run(
                            &handle.gl,
                            &config,
                            time,
                            info.screen_size_px,
                        );
                    }
                })),
            }));
        }
    }
}

/// `CallbackFn` requires `Fn + Sync + Send` on every target, but on wasm32
/// (WebGL) `glow::Context` isn't `Sync` — it wraps `web_sys` handles behind
/// `RefCell`s, since JS objects aren't natively thread-safe. `wasm32`
/// without the `atomics` target feature (which eframe doesn't enable) never
/// actually runs on more than one thread, so this assertion is honest: the
/// wrapped value never crosses a real thread boundary on any target.
struct AssertSendSync<T>(T);
unsafe impl<T> Send for AssertSendSync<T> {}
unsafe impl<T> Sync for AssertSendSync<T> {}

impl<T> AssertSendSync<T> {
    /// A method (rather than direct `.0` field access) so closures capture
    /// this wrapper as a whole — Rust's disjoint closure captures would
    /// otherwise capture just the inner `T` from a bare `resources.0`,
    /// silently defeating the wrapper.
    fn get(&self) -> &T {
        &self.0
    }
}
