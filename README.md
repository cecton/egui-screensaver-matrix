[![crates.io](https://img.shields.io/crates/v/egui-screensaver-matrix.svg)](https://crates.io/crates/egui-screensaver-matrix)
[![docs.rs](https://docs.rs/egui-screensaver-matrix/badge.svg)](https://docs.rs/egui-screensaver-matrix)
[![Rust version](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![dependency status](https://deps.rs/repo/github/cecton/egui-screensaver-matrix/status.svg)](https://deps.rs/repo/github/cecton/egui-screensaver-matrix)
[![CI](https://github.com/cecton/egui-screensaver-matrix/actions/workflows/ci.yml/badge.svg)](https://github.com/cecton/egui-screensaver-matrix/actions/workflows/ci.yml)
[![demo](https://img.shields.io/badge/demo-live-blue)](https://cecton.github.io/egui-screensaver-matrix/)

# egui-screensaver-matrix

Matrix digital rain screensaver for [egui](https://github.com/emilk/egui),
ported from [Rezmason's Matrix](https://github.com/Rezmason/matrix) WebGL
original.

Like the original, this runs the whole simulation on the GPU: ping-pong
"compute" render targets drive the falling-glyph animation, an MSDF font
atlas renders the glyphs, and a bloom pass plus a palette/stripe/pride/trans
color-effect pass finish the look — all through an [`egui::PaintCallback`]
via [`egui_glow`]. This means the host `eframe::App` must use the `glow`
rendering backend (not `wgpu`). All 13 named presets from the original are
included, plus the 4 volumetric (3D perspective) presets.

## Usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
egui-screensaver-matrix = "0.1"
```

Then call `paint` every frame from your `eframe::App::ui` implementation,
before drawing any UI windows:

```rust
use egui_screensaver_matrix::MatrixBackground;

struct MyApp {
    matrix: MatrixBackground,
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.matrix.paint(&ctx, frame.gl());
        // … rest of your UI …
    }
}
```

## Attribution

The bundled font atlases and decorative textures are taken from
[Rezmason/matrix](https://github.com/Rezmason/matrix), MIT licensed. See
`ASSETS-LICENSE.md`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
