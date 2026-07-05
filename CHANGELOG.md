# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-05

### Added

- Initial release: a GPU-shader port of
  [Rezmason's Matrix](https://github.com/Rezmason/matrix) digital rain
  screensaver for egui.
- Full rain simulation via ping-pong compute passes (intro reveal,
  raindrop fall/decay, glyph cycling, ripple/thunder effects) and MSDF
  glyph rendering, via `egui::PaintCallback`/`egui_glow`.
- A 5-level bloom pyramid (highpass + separable blur + combine).
- Palette-gradient, custom-stripes, pride, and trans color effects.
- All 8 bundled fonts and all 13 named presets from the original,
  including the 4 volumetric (3D perspective) presets, with a
  hand-rolled camera matrix, glint-MSDF highlights, and decorative
  base/glint textures.
- Native and web (WASM) demo examples.

[Unreleased]: https://github.com/cecton/egui-screensaver-matrix/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/cecton/egui-screensaver-matrix/releases/tag/v0.1.0
