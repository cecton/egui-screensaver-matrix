# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2] - 2026-10-29

### Fixed

- Frames are now binned into fixed `REPAINT_MS`-wide buckets of animation
  time, and only the first frame of each bucket re-runs the offscreen
  chain, instead of recording whenever 3/4 of a tick had passed since the
  last record. That relaxation existed to survive the host's timer jitter,
  but it also admitted a host repainting every ~8 ms (which pointer motion
  causes, since every mouse event wakes egui's loop), making the chain run
  about 40 times a second instead of 30. Because every pass animates from
  the `time` value directly, that visibly sped the animation up while the
  pointer moved. The record rate is now the tick rate whatever the host
  does above it, on an even cadence.

## [0.5.1] - 2026-10-29

### Fixed

- A repaint that arrives less than one animation tick (`REPAINT_MS`) after
  the last one now re-draws the previous frame instead of re-running the
  whole offscreen chain. The animation only advances once per tick, while
  hosts repaint at whatever rate their own timers and the compositor ask
  for — commonly ~60 fps, i.e. twice per tick — so roughly half of the
  per-frame GPU and main-thread CPU work was producing an image identical
  to the one already on screen. A resize, a `bloom_size` change, a font
  switch, or a host clock that restarts all force an unconditional
  re-record, so the cached frame can never be shown stale.

## [0.5.0] - 2026-10-28

### Changed

- **Breaking:** the renderer moved from `glow` to `wgpu`. Every shader is
  now WGSL, the offscreen passes run on the encoder egui hands to
  `egui_wgpu::CallbackTrait::prepare` and the final composite is drawn in
  egui's render pass, so the host `eframe::App` must use the `wgpu`
  backend. `MatrixBackground::paint` takes
  `Option<&egui_wgpu::RenderState>` (from
  `eframe::Frame::wgpu_render_state()`) instead of
  `Option<&Arc<glow::Context>>`.
- The volumetric projection now maps depth to wgpu's `0..1` clip range.
- The `egui_glow` and `glow` dependencies are gone, along with the
  RGBA16F-to-RGBA8 render-target fallback (RGBA16F is always renderable
  under wgpu).

### Added

- Headless `screenshot` example rendering any preset through
  `egui_wgpu` into a PNG, with no window or compositor.

## [0.1.3] - 2026-07-05

### Fixed

- Switching presets/fonts at runtime (e.g. Megacity → Operator → back to
  Megacity) left most on-screen glyphs showing near-identical characters
  clustered in one corner of the font atlas, instead of the usual varied
  mix. Each cell's stored glyph index was generated against the
  *previous* font's `glyphSequenceLength` range and never invalidated on
  a font switch, so old (possibly much smaller-range) indices got
  reinterpreted against the new atlas's larger grid, landing in its
  low-index corner until each cell happened to cycle again on its own.
  The compute pass now resets on any font switch (and, for the same
  underlying reason, on any grid resize) so every cell immediately
  re-rolls its glyph against whatever font/atlas is currently active.

## [0.1.2] - 2026-07-05

### Fixed

- Individual glyphs no longer cycled to a new character independently;
  the whole screen visibly changed characters in synchronized waves
  instead. Caused by a rate mismatch introduced by the 0.1.1 fix: the
  simulation's tick rate (60fps, matching the original) was batching
  roughly 2 logical ticks into every rendered frame, since this port
  only repaints at ~30fps — collapsing each cell's independent
  phase-based desync into a single, simultaneous mass reassignment every
  frame. The simulation's nominal tick rate now matches this port's
  actual render cadence (30fps) instead of the original's 60fps, so each
  rendered frame represents at most one logical tick again, at the cost
  of running roughly half as fast, absolute-speed-wise, as the original.

## [0.1.1] - 2026-07-05

### Fixed

- Glyph-cycling and brightness-decay simulation state advanced once per
  call to the paint callback instead of once per elapsed simulation tick,
  making the rain's pacing speed up whenever the host forced extra
  repaints (e.g. `eframe`'s native backend does this on every mouse-move
  event). The compute pass now tracks elapsed wall-clock time and skips
  its state-mutating work entirely on calls where no new tick has
  elapsed, matching the original's `shouldRender`-gated pipeline
  stepping.

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

[Unreleased]: https://github.com/cecton/egui-screensaver-matrix/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/cecton/egui-screensaver-matrix/compare/v0.4.0...v0.5.0
[0.1.3]: https://github.com/cecton/egui-screensaver-matrix/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/cecton/egui-screensaver-matrix/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/cecton/egui-screensaver-matrix/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/cecton/egui-screensaver-matrix/releases/tag/v0.1.0
