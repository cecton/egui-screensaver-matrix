//! Tunable parameters, ported from the original's `config.js` `defaults`
//! object, plus all 13 named presets from its `versions` object (9
//! standard 2D presets and 4 volumetric-3D presets).

use crate::fonts::FontId;
use crate::textures::DecorativeTexture;

/// One color stop in a [`Effect::Palette`] gradient. `at` is the
/// brightness position (0..1) this color sits at; stops are sorted and
/// interpolated between when the gradient texture is built (see
/// `passes/effect.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PaletteStop {
    pub color: [f32; 3],
    pub at: f32,
}

/// Selects how rain brightness gets mapped to final on-screen color.
/// Independent of [`Preset`]/font selection, matching the original (any
/// font/version can be combined with any effect).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Effect {
    /// Maps brightness through a smooth gradient of color stops. The
    /// original's `effect: "palette"`.
    Palette(Vec<PaletteStop>),
    /// Multiplies brightness by a custom repeating/sweeping color
    /// sequence instead of a gradient. The original's `effect: "stripes"`
    /// with a custom `stripeColors` array.
    Stripes(Vec<[f32; 3]>),
    /// The pride-flag color sweep. The original's `effect: "pride"`.
    Pride,
    /// The trans-flag color sweep. The original's `effect: "trans"`.
    Trans,
}

/// Which screen-space ripple distortion effect is active, if any. The
/// original's `rippleTypeName` (`null` / `"box"` / `"circle"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RippleType {
    Box,
    Circle,
}

/// Rain simulation, glyph-rendering, bloom, and color tunables.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MatrixConfig {
    /// Which bundled glyph atlas to render with. The original's `font`.
    pub font: FontId,
    /// Number of glyph columns across the screen width. The original's
    /// `numColumns`; row count is derived from this and the aspect ratio.
    pub num_columns: u32,
    /// Global multiplier on every time-driven animation in the simulation.
    /// The original's `animationSpeed`.
    pub animation_speed: f32,
    /// How fast raindrops fall down their column. The original's
    /// `fallSpeed`.
    pub fall_speed: f32,
    /// Controls both the frequency and visual length of raindrops in a
    /// column (larger = longer, sparser drops). The original's
    /// `raindropLength`.
    pub raindrop_length: f32,
    /// How quickly a cell's brightness blends toward its newly computed
    /// value each frame — `1.0` snaps instantly, smaller values make
    /// brightness changes organic/gradual. The original's
    /// `brightnessDecay`.
    pub brightness_decay: f32,
    /// How fast glyphs cycle to a new random symbol. The original's
    /// `cycleSpeed`.
    pub cycle_speed: f32,
    /// Minimum number of frames between glyph-cycling checks. The
    /// original's `cycleFrameSkip`.
    pub cycle_frame_skip: u32,
    /// Skips the multi-second "rain reveals from a blank screen" intro
    /// animation, showing the fully-established effect immediately. The
    /// original's `skipIntro`; defaults to `true` here (unlike the
    /// original's `false`) since a screensaver activating shouldn't make
    /// the user wait for a reveal animation each time.
    pub skip_intro: bool,
    /// Adds dramatic lightning-flash brightness spikes. The original's
    /// `hasThunder`.
    pub has_thunder: bool,
    /// The angle rain falls at, and the orientation of the glyph grid, in
    /// radians. The original's `slant`.
    pub slant: f32,
    /// Renders the glyph grid arcing across the screen (glyphs radiating
    /// from above) instead of a flat grid. The original's `isPolar`.
    pub is_polar: bool,
    /// Screen-space ripple distortion, if any. The original's
    /// `rippleTypeName`.
    pub ripple_type: Option<RippleType>,
    /// Size of the ripple effect. The original's `rippleScale`.
    pub ripple_scale: f32,
    /// Speed the ripple effect travels at. The original's `rippleSpeed`.
    pub ripple_speed: f32,
    /// Thickness of the ripple effect's visible ring/edge. The original's
    /// `rippleThickness`.
    pub ripple_thickness: f32,
    /// Aspect ratio of individual glyphs. The original's
    /// `glyphHeightToWidth`.
    pub glyph_height_to_width: f32,
    /// Crops a border around each glyph in the font atlas. The original's
    /// `glyphEdgeCrop`.
    pub glyph_edge_crop: f32,
    /// Horizontally mirrors glyphs. The original's `glyphFlip`.
    pub glyph_flip: bool,
    /// Rotates glyphs, in degrees (typically 90° increments). The
    /// original's `glyphRotation`.
    pub glyph_rotation: f32,
    /// Brightness offset applied before contrast. The original's
    /// `baseBrightness`.
    pub base_brightness: f32,
    /// Contrast multiplier for glyph brightness. The original's
    /// `baseContrast`.
    pub base_contrast: f32,
    /// Minimum brightness for a glyph to still be considered visible. The
    /// original's `brightnessThreshold`.
    pub brightness_threshold: f32,
    /// If greater than 0, overrides computed brightness with this fixed
    /// value (for cells above `brightness_threshold`). The original's
    /// `brightnessOverride`.
    pub brightness_override: f32,
    /// Whether the brightest glyph at the bottom of a raindrop ("the
    /// cursor") is rendered as its own isolated highlight color. The
    /// original's `isolateCursor`.
    pub isolate_cursor: bool,
    /// The cursor's highlight color. The original's `cursorColor`.
    pub cursor_color: [f32; 3],
    /// Intensity multiplier for the cursor color. The original's
    /// `cursorIntensity`.
    pub cursor_intensity: f32,
    /// The glint highlight color (not yet visible — glint textures land
    /// in a later stage). The original's `glintColor`.
    pub glint_color: [f32; 3],
    /// Intensity multiplier for the glint color. The original's
    /// `glintIntensity`.
    pub glint_intensity: f32,
    /// The color "behind" the glyphs. The original's `backgroundColor`.
    pub background_color: [f32; 3],
    /// Magnitude of random per-pixel dimming, to hide gradient banding.
    /// The original's `ditherMagnitude`.
    pub dither_magnitude: f32,
    /// Intensity of the glow/bloom effect. `0.0` disables bloom entirely.
    /// The original's `bloomStrength`.
    pub bloom_strength: f32,
    /// Resolution scale (relative to the screen) of the bloom blur
    /// pyramid — smaller is cheaper and blurrier. The original's
    /// `bloomSize`.
    pub bloom_size: f32,
    /// Minimum per-channel brightness that still contributes to the
    /// bloom; dimmer areas are excluded before blurring. The original's
    /// `highPassThreshold`.
    pub high_pass_threshold: f32,
    /// How brightness maps to final color. The original's `effect` +
    /// `palette`/`stripeColors`.
    pub effect: Effect,
    /// Renders glyphs in perspective, receding into the distance, instead
    /// of a flat 2D grid. The original's `volumetric`.
    pub volumetric: bool,
    /// In volumetric mode, the ratio of actual columns to the grid width
    /// (allows overlapping columns for a denser look). The original's
    /// `density`.
    pub density: f32,
    /// In volumetric mode, how fast columns approach the camera. The
    /// original's `forwardSpeed`.
    pub forward_speed: f32,
    /// Whether glint highlights (bright accents on certain glyphs, using
    /// `glint_texture`) render as their own isolated color. The
    /// original's `isolateGlint`.
    pub isolate_glint: bool,
    /// Brightness offset applied to the glint channel before contrast.
    /// The original's `glintBrightness`.
    pub glint_brightness: f32,
    /// Contrast multiplier for the glint channel. The original's
    /// `glintContrast`.
    pub glint_contrast: f32,
    /// Decorative grayscale mask multiplying base glyph brightness, if
    /// any. The original's `baseTexture`.
    pub base_texture: Option<DecorativeTexture>,
    /// Decorative grayscale mask multiplying glint brightness, if any.
    /// The original's `glintTexture`.
    pub glint_texture: Option<DecorativeTexture>,
}

/// `hsl(h, s, l)` -> linear `[r, g, b]` in 0..1, matching the original's
/// `colorToRGB.js` HSL conversion (used so preset color constants below
/// can be copied verbatim from `config.js` instead of hand-converted).
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s == 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue_to_rgb = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [
        hue_to_rgb(h + 1.0 / 3.0),
        hue_to_rgb(h),
        hue_to_rgb(h - 1.0 / 3.0),
    ]
}

fn classic_palette() -> Vec<PaletteStop> {
    vec![
        PaletteStop {
            color: hsl_to_rgb(0.3, 0.9, 0.0),
            at: 0.0,
        },
        PaletteStop {
            color: hsl_to_rgb(0.3, 0.9, 0.2),
            at: 0.2,
        },
        PaletteStop {
            color: hsl_to_rgb(0.3, 0.9, 0.7),
            at: 0.7,
        },
        PaletteStop {
            color: hsl_to_rgb(0.3, 0.9, 0.8),
            at: 0.8,
        },
    ]
}

impl Default for MatrixConfig {
    fn default() -> Self {
        Self {
            font: FontId::Matrixcode,
            num_columns: 80,
            animation_speed: 1.0,
            fall_speed: 0.3,
            raindrop_length: 0.75,
            brightness_decay: 1.0,
            cycle_speed: 0.03,
            cycle_frame_skip: 1,
            skip_intro: true,
            has_thunder: false,
            slant: 0.0,
            is_polar: false,
            ripple_type: None,
            ripple_scale: 30.0,
            ripple_speed: 0.2,
            ripple_thickness: 0.2,
            glyph_height_to_width: 1.0,
            glyph_edge_crop: 0.0,
            glyph_flip: false,
            glyph_rotation: 0.0,
            base_brightness: -0.5,
            base_contrast: 1.1,
            brightness_threshold: 0.0,
            brightness_override: 0.0,
            isolate_cursor: true,
            cursor_color: hsl_to_rgb(0.242, 1.0, 0.73),
            cursor_intensity: 2.0,
            glint_color: hsl_to_rgb(0.0, 0.0, 1.0),
            glint_intensity: 1.0,
            background_color: hsl_to_rgb(0.0, 0.0, 0.0),
            dither_magnitude: 0.05,
            bloom_strength: 0.7,
            bloom_size: 0.4,
            high_pass_threshold: 0.1,
            effect: Effect::Palette(classic_palette()),
            volumetric: false,
            density: 1.0,
            forward_speed: 0.25,
            isolate_glint: false,
            glint_brightness: -1.5,
            glint_contrast: 2.5,
            base_texture: None,
            glint_texture: None,
        }
    }
}

/// All 13 named presets from the original's `versions` object except
/// `holoplay` (hardware-specific Looking Glass display support, out of
/// scope). `1999`/`throwback` alias `Operator`, and `2021`/`updated`
/// alias `Resurrections` in the original — not represented as separate
/// variants here since they're identical configs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Preset {
    #[default]
    Classic,
    Megacity,
    Neomatrixology,
    Operator,
    Nightmare,
    Paradise,
    Resurrections,
    Palimpsest,
    Twilight,
    Trinity,
    Morpheus,
    Bugs,
    ThreeD,
}

impl MatrixConfig {
    pub fn from_preset(preset: Preset) -> Self {
        let mut config = Self::default();
        config.apply_preset(preset);
        config
    }

    /// Overwrites every field this preset customizes, leaving the rest at
    /// their defaults (matching the original's `{...defaults, ...version}`
    /// merge).
    pub fn apply_preset(&mut self, preset: Preset) {
        *self = Self::default();
        match preset {
            Preset::Classic => {}
            Preset::Megacity => {
                self.font = FontId::Megacity;
                self.animation_speed = 0.5;
                self.num_columns = 40;
            }
            Preset::Neomatrixology => {
                self.font = FontId::Neomatrixology;
                self.animation_speed = 0.8;
                self.num_columns = 40;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.15, 0.9, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.15, 0.9, 0.2),
                        at: 0.2,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.15, 0.9, 0.7),
                        at: 0.7,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.15, 0.9, 0.8),
                        at: 0.8,
                    },
                ]);
                self.cursor_color = hsl_to_rgb(0.167, 1.0, 0.75);
                self.cursor_intensity = 2.0;
            }
            Preset::Operator => {
                self.cursor_color = hsl_to_rgb(0.375, 1.0, 0.66);
                self.cursor_intensity = 3.0;
                self.bloom_size = 0.6;
                self.bloom_strength = 0.75;
                self.high_pass_threshold = 0.0;
                self.cycle_speed = 0.01;
                self.cycle_frame_skip = 8;
                self.brightness_override = 0.22;
                self.brightness_threshold = 0.0;
                self.fall_speed = 0.6;
                self.glyph_edge_crop = 0.15;
                self.glyph_height_to_width = 1.35;
                self.ripple_type = Some(RippleType::Box);
                self.num_columns = 108;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.4, 0.8, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.4, 0.8, 0.5),
                        at: 0.5,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.4, 0.8, 1.0),
                        at: 1.0,
                    },
                ]);
                self.raindrop_length = 1.5;
            }
            Preset::Nightmare => {
                self.font = FontId::Gothic;
                self.isolate_cursor = false;
                self.high_pass_threshold = 0.7;
                self.base_brightness = -0.8;
                self.brightness_decay = 0.75;
                self.fall_speed = 1.2;
                self.has_thunder = true;
                self.num_columns = 60;
                self.cycle_speed = 0.35;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.0, 1.0, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.0, 1.0, 0.2),
                        at: 0.2,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.0, 1.0, 0.4),
                        at: 0.4,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.1, 1.0, 0.7),
                        at: 0.7,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.2, 1.0, 1.0),
                        at: 1.0,
                    },
                ]);
                self.raindrop_length = 0.5;
                self.slant = 22.5f32.to_radians();
            }
            Preset::Paradise => {
                self.font = FontId::Coptic;
                self.isolate_cursor = false;
                self.bloom_strength = 1.0;
                self.high_pass_threshold = 0.0;
                self.cycle_speed = 0.005;
                self.base_brightness = -1.3;
                self.base_contrast = 2.0;
                self.brightness_decay = 0.05;
                self.fall_speed = 0.02;
                self.is_polar = true;
                self.ripple_type = Some(RippleType::Circle);
                self.ripple_speed = 0.1;
                self.num_columns = 40;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.0, 0.0, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.0, 0.8, 0.3),
                        at: 0.3,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.1, 0.8, 0.5),
                        at: 0.5,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.1, 1.0, 0.6),
                        at: 0.6,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.1, 1.0, 0.9),
                        at: 0.9,
                    },
                ]);
                self.raindrop_length = 0.4;
            }
            Preset::Resurrections => {
                self.font = FontId::Resurrections;
                self.glyph_edge_crop = 0.1;
                self.cursor_color = hsl_to_rgb(0.292, 1.0, 0.8);
                self.cursor_intensity = 2.0;
                self.base_brightness = -0.7;
                self.base_contrast = 1.17;
                self.high_pass_threshold = 0.0;
                self.num_columns = 70;
                self.cycle_speed = 0.03;
                self.bloom_strength = 0.7;
                self.fall_speed = 0.3;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.375, 0.9, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.375, 1.0, 0.6),
                        at: 0.92,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.375, 1.0, 1.0),
                        at: 1.0,
                    },
                ]);
            }
            Preset::Palimpsest => {
                self.font = FontId::HuberfishA;
                self.isolate_cursor = false;
                self.bloom_strength = 0.2;
                self.num_columns = 40;
                self.raindrop_length = 1.2;
                self.cycle_frame_skip = 3;
                self.fall_speed = 0.5;
                self.slant = std::f32::consts::PI * -0.0625;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.15, 0.25, 0.9),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.6, 0.8, 0.1),
                        at: 0.4,
                    },
                ]);
            }
            Preset::Twilight => {
                self.font = FontId::HuberfishD;
                self.cursor_color = hsl_to_rgb(0.167, 1.0, 0.8);
                self.cursor_intensity = 1.5;
                self.bloom_strength = 0.1;
                self.num_columns = 50;
                self.raindrop_length = 0.9;
                self.fall_speed = 0.1;
                self.high_pass_threshold = 0.0;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.6, 1.0, 0.05),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.6, 0.8, 0.1),
                        at: 0.1,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.88, 0.8, 0.5),
                        at: 0.5,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.15, 1.0, 0.6),
                        at: 0.8,
                    },
                ]);
            }
            Preset::Trinity => {
                self.font = FontId::Resurrections;
                self.glint_texture = Some(DecorativeTexture::Metal);
                self.base_texture = Some(DecorativeTexture::Pixels);
                self.glyph_edge_crop = 0.1;
                self.cursor_color = hsl_to_rgb(0.292, 1.0, 0.8);
                self.cursor_intensity = 2.0;
                self.isolate_glint = true;
                self.glint_color = hsl_to_rgb(0.131, 1.0, 0.6);
                self.glint_intensity = 3.0;
                self.glint_brightness = -0.5;
                self.glint_contrast = 1.5;
                self.base_brightness = -0.4;
                self.base_contrast = 1.5;
                self.high_pass_threshold = 0.0;
                self.num_columns = 60;
                self.cycle_speed = 0.01;
                self.bloom_strength = 0.7;
                self.fall_speed = 0.3;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.37, 0.6, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.37, 0.6, 0.5),
                        at: 1.0,
                    },
                ]);
                self.volumetric = true;
                self.forward_speed = 0.2;
                self.raindrop_length = 0.3;
                self.density = 0.75;
            }
            Preset::Morpheus => {
                self.font = FontId::Resurrections;
                self.glint_texture = Some(DecorativeTexture::Mesh);
                self.base_texture = Some(DecorativeTexture::Metal);
                self.glyph_edge_crop = 0.1;
                self.cursor_color = hsl_to_rgb(0.333, 1.0, 0.85);
                self.cursor_intensity = 2.0;
                self.isolate_glint = true;
                self.glint_color = hsl_to_rgb(0.4, 1.0, 0.5);
                self.glint_intensity = 2.0;
                self.glint_brightness = -1.5;
                self.glint_contrast = 3.0;
                self.base_brightness = -0.3;
                self.base_contrast = 1.5;
                self.high_pass_threshold = 0.0;
                self.num_columns = 60;
                self.cycle_speed = 0.015;
                self.bloom_strength = 0.7;
                self.fall_speed = 0.3;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.97, 0.6, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.97, 0.6, 0.5),
                        at: 1.0,
                    },
                ]);
                self.volumetric = true;
                self.forward_speed = 0.1;
                self.raindrop_length = 0.4;
                self.density = 0.75;
            }
            Preset::Bugs => {
                self.font = FontId::Resurrections;
                self.glint_texture = Some(DecorativeTexture::Sand);
                self.base_texture = Some(DecorativeTexture::Metal);
                self.glyph_edge_crop = 0.1;
                self.cursor_color = hsl_to_rgb(0.619, 1.0, 0.65);
                self.cursor_intensity = 2.0;
                self.isolate_glint = true;
                self.glint_color = hsl_to_rgb(0.625, 1.0, 0.6);
                self.glint_intensity = 3.0;
                self.glint_brightness = -1.0;
                self.glint_contrast = 3.0;
                self.base_brightness = -0.3;
                self.base_contrast = 1.5;
                self.high_pass_threshold = 0.0;
                self.num_columns = 60;
                self.cycle_speed = 0.01;
                self.bloom_strength = 0.7;
                self.fall_speed = 0.3;
                self.effect = Effect::Palette(vec![
                    PaletteStop {
                        color: hsl_to_rgb(0.12, 0.6, 0.0),
                        at: 0.0,
                    },
                    PaletteStop {
                        color: hsl_to_rgb(0.14, 0.6, 0.5),
                        at: 1.0,
                    },
                ]);
                self.volumetric = true;
                self.forward_speed = 0.4;
                self.raindrop_length = 0.3;
                self.density = 0.75;
            }
            Preset::ThreeD => {
                self.volumetric = true;
                self.fall_speed = 0.5;
                self.cycle_speed = 0.03;
                self.base_brightness = -0.9;
                self.base_contrast = 1.5;
                self.raindrop_length = 0.3;
            }
        }
    }
}

/// Exact pride-flag stripe colors, ported verbatim from
/// `js/regl/stripePass.js`'s `prideStripeColors` (each of the 6 colors
/// repeated twice, giving softer/flatter transitions between bands).
pub fn pride_stripe_colors() -> Vec<[f32; 3]> {
    let colors = [
        [0.89, 0.01, 0.01],
        [1.0, 0.55, 0.0],
        [1.0, 0.93, 0.0],
        [0.0, 0.5, 0.15],
        [0.0, 0.3, 1.0],
        [0.46, 0.03, 0.53],
    ];
    colors.iter().flat_map(|&c| [c, c]).collect()
}

/// Exact trans-flag stripe colors, ported verbatim from
/// `js/regl/stripePass.js`'s `transPrideStripeColors` (each of the 5
/// colors repeated 3 times).
pub fn trans_stripe_colors() -> Vec<[f32; 3]> {
    let colors = [
        [0.36, 0.81, 0.98],
        [0.96, 0.66, 0.72],
        [1.0, 1.0, 1.0],
        [0.96, 0.66, 0.72],
        [0.36, 0.81, 0.98],
    ];
    colors.iter().flat_map(|&c| [c, c, c]).collect()
}
