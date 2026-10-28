//! The final compositing pass: maps rain+bloom brightness to color, either
//! through a smooth gradient (`palette`) or a repeating/sweeping custom
//! color sequence (`stripes`/`pride`/`trans`), ported from
//! `js/regl/palettePass.js` / `js/regl/stripePass.js` and
//! `shaders/glsl/palettePass.frag.glsl` / `shaders/glsl/stripePass.frag.glsl`
//! at https://github.com/Rezmason/matrix (MIT licensed). Unlike every
//! other pass, this one is drawn straight into egui's own render pass (see
//! `lib.rs`), so it is split into `prepare` (uniforms and bind group) and
//! `paint` (the draw call).

use egui_wgpu::wgpu;

use crate::config::{Effect, MatrixConfig, PaletteStop};
use crate::gpu::{self, PassPipeline, Samplers, UniformBuffer, Uniforms};

const PALETTE_SRC: &str = include_str!("../shaders/palette.wgsl");

/// Resolution of the generated palette gradient texture, matching the
/// original's `PALETTE_SIZE`.
const PALETTE_SIZE: usize = 2048;

/// Size in bytes of the `Params` uniform block in `palette.wgsl`.
const UNIFORM_SIZE: u64 = 64;

/// Builds the `PALETTE_SIZE`-texel 1D gradient texture from a set of
/// (possibly unsorted) color stops, ported from `palettePass.js`'s
/// `makePalette`: stops are sorted by `at`, the first/last colors are
/// extended to the texture's edges, and intermediate texels are linearly
/// interpolated between neighboring stops.
fn build_palette_rgba(stops: &[PaletteStop]) -> Vec<u8> {
    let mut sorted: Vec<&PaletteStop> = stops.iter().collect();
    sorted.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap());

    let mut entries: Vec<(usize, [f32; 3])> = sorted
        .iter()
        .map(|stop| {
            let index = ((stop.at.clamp(0.0, 1.0)) * (PALETTE_SIZE - 1) as f32) as usize;
            (index, stop.color)
        })
        .collect();
    entries.insert(0, (0, entries[0].1));
    entries.push((PALETTE_SIZE - 1, entries[entries.len() - 1].1));

    let mut texels = vec![[0.0f32; 3]; PALETTE_SIZE];
    for pair in entries.windows(2) {
        let (start_index, start_color) = pair[0];
        let (end_index, end_color) = pair[1];
        texels[start_index] = start_color;
        let diff = end_index - start_index;
        for i in 0..diff {
            let ratio = i as f32 / diff as f32;
            texels[start_index + i] = [
                start_color[0] * (1.0 - ratio) + end_color[0] * ratio,
                start_color[1] * (1.0 - ratio) + end_color[1] * ratio,
                start_color[2] * (1.0 - ratio) + end_color[2] * ratio,
            ];
        }
    }
    texels[PALETTE_SIZE - 1] = entries.last().unwrap().1;

    let mut rgba = Vec::with_capacity(PALETTE_SIZE * 4);
    for color in texels {
        rgba.extend_from_slice(&[
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            255,
        ]);
    }
    rgba
}

fn build_stripe_rgba(colors: &[[f32; 3]]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(colors.len() * 4);
    for color in colors {
        rgba.extend_from_slice(&[
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            255,
        ]);
    }
    rgba
}

/// A 1-texel-tall RGBA8 lookup texture (palette gradient or stripe
/// sequence).
struct ColorTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl ColorTexture {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, rgba: &[u8], width: usize) -> Self {
        let texture = gpu::upload_rgba(device, queue, rgba, width as u32, 1);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
        }
    }
}

fn stripe_colors_for(effect: &Effect) -> Option<Vec<[f32; 3]>> {
    match effect {
        Effect::Stripes(colors) => Some(colors.clone()),
        Effect::Pride => Some(crate::config::pride_stripe_colors()),
        Effect::Trans => Some(crate::config::trans_stripe_colors()),
        Effect::Palette(_) => None,
    }
}

pub struct EffectPasses {
    palette_pass: PassPipeline,
    stripes_pass: PassPipeline,
    uniforms: UniformBuffer,

    palette_texture: ColorTexture,
    built_palette: Vec<PaletteStop>,
    stripe_texture: ColorTexture,
    built_stripes: Vec<[f32; 3]>,

    /// Built by `prepare`, drawn by `paint`.
    draw: Option<(bool, wgpu::BindGroup)>,
}

impl EffectPasses {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        config: &MatrixConfig,
    ) -> Self {
        let module = gpu::shader_module(device, "matrix palette", PALETTE_SRC);
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let entries = [
            gpu::uniform_entry(0, fragment),
            gpu::texture_entry(1, fragment),
            gpu::texture_entry(2, fragment),
            gpu::texture_entry(3, fragment),
            gpu::sampler_entry(4, fragment),
        ];
        let make = |fragment_entry| {
            PassPipeline::new(
                device,
                "matrix palette",
                &module,
                "vs_screen",
                fragment_entry,
                &entries,
                &[],
                target_format,
                Some(wgpu::BlendState::REPLACE),
            )
        };

        let built_palette = match &config.effect {
            Effect::Palette(stops) => stops.clone(),
            _ => vec![PaletteStop {
                color: [0.0, 1.0, 0.0],
                at: 1.0,
            }],
        };
        let palette_texture = ColorTexture::new(
            device,
            queue,
            &build_palette_rgba(&built_palette),
            PALETTE_SIZE,
        );

        let built_stripes =
            stripe_colors_for(&config.effect).unwrap_or_else(|| vec![[0.0, 1.0, 0.0]]);
        let stripe_texture = ColorTexture::new(
            device,
            queue,
            &build_stripe_rgba(&built_stripes),
            built_stripes.len(),
        );

        Self {
            palette_pass: make("fs_palette"),
            stripes_pass: make("fs_stripes"),
            uniforms: UniformBuffer::new(device, "matrix palette", UNIFORM_SIZE),
            palette_texture,
            built_palette,
            stripe_texture,
            built_stripes,
            draw: None,
        }
    }

    fn sync_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, effect: &Effect) {
        if let Effect::Palette(stops) = effect
            && stops.as_slice() != self.built_palette.as_slice()
        {
            self.palette_texture =
                ColorTexture::new(device, queue, &build_palette_rgba(stops), PALETTE_SIZE);
            self.built_palette = stops.clone();
        }

        if let Some(colors) = stripe_colors_for(effect)
            && colors != self.built_stripes
        {
            self.stripe_texture =
                ColorTexture::new(device, queue, &build_stripe_rgba(&colors), colors.len());
            self.built_stripes = colors;
        }
    }

    /// Writes this frame's uniforms and builds the bind group `paint` will
    /// draw with.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        samplers: &Samplers,
        config: &MatrixConfig,
        time: f32,
        primary: &wgpu::TextureView,
        bloom: &wgpu::TextureView,
    ) {
        self.sync_textures(device, queue, &config.effect);

        let mut u = Uniforms::new();
        u.f32(config.dither_magnitude)
            .f32(time)
            .f32(config.cursor_intensity)
            .f32(config.glint_intensity)
            .vec3(config.background_color)
            .vec3(config.cursor_color)
            .vec3(config.glint_color);
        self.uniforms.write(queue, u);

        let is_palette = matches!(config.effect, Effect::Palette(_));
        let (pass, color) = if is_palette {
            (&self.palette_pass, &self.palette_texture)
        } else {
            (&self.stripes_pass, &self.stripe_texture)
        };
        let group = pass.bind_group(
            device,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(primary),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&samplers.linear),
                },
            ],
        );
        self.draw = Some((is_palette, group));
    }

    /// Draws the final composited image into egui's render pass.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        let Some((is_palette, group)) = &self.draw else {
            return;
        };
        let pass = if *is_palette {
            &self.palette_pass
        } else {
            &self.stripes_pass
        };
        render_pass.set_pipeline(&pass.pipeline);
        render_pass.set_bind_group(0, group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
