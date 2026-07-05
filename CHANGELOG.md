# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial in-progress port of [Rezmason's Matrix](https://github.com/Rezmason/matrix)
  digital rain screensaver for egui.
- GPU-rendered ping-pong compute passes (intro reveal, raindrop fall/decay,
  glyph cycling, ripple/thunder effects) and a single-font 2D MSDF glyph
  render pass, via `egui::PaintCallback`/`egui_glow`.
- Native and web (WASM) demo examples.

Not yet implemented: bloom, palette/stripe/pride/trans color effects,
additional fonts, volumetric 3D mode.
