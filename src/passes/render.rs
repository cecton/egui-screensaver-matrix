//! The main render pass: samples the compute-state textures plus MSDF
//! glyph (and, for fonts that have one, glint) atlases and draws the
//! falling glyphs, additively, into an offscreen target. Holds two
//! pipeline variants — a 2D fullscreen-triangle path and a volumetric
//! per-cell-quad-grid path — selected per draw call by `config.volumetric`
//! (see `shaders/render.wgsl`'s header comment for why these are two
//! pipelines rather than one branching on a uniform).

use egui_wgpu::wgpu;

use crate::camera;
use crate::config::MatrixConfig;
use crate::fonts::FontAtlas;
use crate::geometry::{self, QuadGrid};
use crate::gpu::{self, PassPipeline, Samplers, TargetFormat, UniformBuffer, Uniforms};
use crate::textures::DecorativeTexture;

const RENDER_SRC: &str = include_str!("../shaders/render.wgsl");

/// Size in bytes of the `Params` uniform block in `render.wgsl`.
const UNIFORM_SIZE: u64 = 224;

/// The signed distance range (in atlas texels) the bundled MSDF atlases
/// were generated with — matches the original's hardcoded `msdfPxRange`.
const MSDF_PX_RANGE: f32 = 4.0;

/// The original's `glyphVerticalSpacing` default, never overridden by any
/// bundled preset — not exposed as a `MatrixConfig` field since nothing
/// uses a different value, but still a real part of the vertex math.
const GLYPH_VERTICAL_SPACING: f32 = 1.0;

/// A texture together with its default view and pixel size.
struct GpuImage {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [f32; 2],
}

impl GpuImage {
    fn from_texture(texture: wgpu::Texture, size: [f32; 2]) -> Self {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
            size,
        }
    }

    fn from_png(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        png_bytes: &[u8],
    ) -> Result<Self, String> {
        let (texture, size) = gpu::upload_png_texture(device, queue, png_bytes)?;
        Ok(Self::from_texture(texture, size))
    }
}

/// The glyph (and optional glint) MSDF atlases of the current font, plus
/// the metadata of its glyph grid.
struct FontTextures {
    glyph_msdf: GpuImage,
    glint_msdf: Option<GpuImage>,
    grid_size: [f32; 2],
    glyph_sequence_length: f32,
}

impl FontTextures {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, font: &FontAtlas) -> Result<Self, String> {
        let glyph_msdf = GpuImage::from_png(device, queue, font.png_bytes)?;
        let glint_msdf = font
            .glint_png_bytes
            .map(|bytes| GpuImage::from_png(device, queue, bytes))
            .transpose()?;
        Ok(Self {
            glyph_msdf,
            glint_msdf,
            grid_size: [font.grid_cols as f32, font.grid_rows as f32],
            glyph_sequence_length: font.glyph_sequence_length as f32,
        })
    }
}

pub struct RenderPass {
    pass_2d: PassPipeline,
    pass_volumetric: PassPipeline,
    uniforms: UniformBuffer,
    quad_grid: QuadGrid,

    font: FontTextures,
    white: GpuImage,
    base_texture: Option<(DecorativeTexture, GpuImage)>,
    glint_texture: Option<(DecorativeTexture, GpuImage)>,
}

/// Column-major 2x2 matrix combining the glyph-flip and glyph-rotation
/// tunables, ported from the original's
/// `mat2.rotate(mat2.fromScaling(..., [flip ? -1 : 1, 1]), ..., rotationRadians)`.
fn glyph_transform(flip: bool, rotation_degrees: f32) -> [f32; 4] {
    let sx = if flip { -1.0 } else { 1.0 };
    let (s, c) = rotation_degrees.to_radians().sin_cos();
    [sx * c, s, -sx * s, c]
}

impl RenderPass {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font: &FontAtlas,
    ) -> Result<Self, String> {
        let module = gpu::shader_module(device, "matrix render", RENDER_SRC);
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let entries = [
            gpu::uniform_entry(0, both),
            gpu::texture_entry(1, both),
            gpu::texture_entry(2, both),
            gpu::texture_entry(3, both),
            gpu::texture_entry(4, fragment),
            gpu::texture_entry(5, fragment),
            gpu::texture_entry(6, fragment),
            gpu::texture_entry(7, fragment),
            gpu::sampler_entry(8, both),
            gpu::sampler_entry(9, fragment),
        ];
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let blend = Some(wgpu::BlendState {
            color: additive,
            alpha: additive,
        });
        let format = TargetFormat::Rgba8.wgpu();

        let pass_2d = PassPipeline::new(
            device,
            "matrix render 2d",
            &module,
            "vs_2d",
            "fs_2d",
            &entries,
            &[],
            format,
            blend,
        );
        let pass_volumetric = PassPipeline::new(
            device,
            "matrix render volumetric",
            &module,
            "vs_volumetric",
            "fs_volumetric",
            &entries,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: geometry::VERTEX_STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &geometry::VERTEX_ATTRIBUTES,
            })],
            format,
            blend,
        );

        let white = GpuImage::from_texture(
            gpu::upload_rgba(device, queue, &[255, 255, 255, 255], 1, 1),
            [1.0, 1.0],
        );

        Ok(Self {
            pass_2d,
            pass_volumetric,
            uniforms: UniformBuffer::new(device, "matrix render", UNIFORM_SIZE),
            quad_grid: QuadGrid::new(device, 1, 1),
            font: FontTextures::new(device, queue, font)?,
            white,
            base_texture: None,
            glint_texture: None,
        })
    }

    /// Swaps in a different font's glyph (and, if present, glint) atlas —
    /// the pipelines stay the same across fonts, only the textures and
    /// their grid metadata change.
    pub fn set_font(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font: &FontAtlas,
    ) -> Result<(), String> {
        self.font = FontTextures::new(device, queue, font)?;
        Ok(())
    }

    pub fn glyph_sequence_length(&self) -> u32 {
        self.font.glyph_sequence_length as u32
    }

    fn sync_decorative_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: &mut Option<(DecorativeTexture, GpuImage)>,
        wanted: Option<DecorativeTexture>,
    ) {
        if slot.as_ref().map(|(id, _)| *id) == wanted {
            return;
        }
        *slot = None;
        if let Some(id) = wanted {
            match GpuImage::from_png(device, queue, id.png_bytes()) {
                Ok(image) => *slot = Some((id, image)),
                Err(err) => {
                    log::error!("egui-screensaver-matrix: failed to load decorative texture: {err}")
                }
            }
        }
    }

    /// Draws additively into `target`, clearing it to opaque black first.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        samplers: &Samplers,
        config: &MatrixConfig,
        time: f32,
        num_columns: u32,
        num_rows: u32,
        states: [&wgpu::TextureView; 3],
        target: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
    ) {
        Self::sync_decorative_texture(device, queue, &mut self.base_texture, config.base_texture);
        Self::sync_decorative_texture(device, queue, &mut self.glint_texture, config.glint_texture);

        let aspect_ratio = screen_width as f32 / screen_height.max(1) as f32;
        let screen_size = if aspect_ratio > 1.0 {
            [1.0, aspect_ratio]
        } else {
            [1.0 / aspect_ratio, 1.0]
        };
        let slant_vec = [config.slant.cos(), config.slant.sin()];
        let slant_scale = 1.0 / ((2.0 * config.slant).sin().abs() * (2.0f32.sqrt() - 1.0) + 1.0);
        let view_projection = camera::volumetric_view_projection(aspect_ratio);

        let mut u = Uniforms::new();
        u.f32(num_columns as f32)
            .f32(num_rows as f32)
            .f32(MSDF_PX_RANGE)
            .f32(config.glyph_height_to_width)
            .f32(self.font.glyph_sequence_length)
            .f32(config.glyph_edge_crop)
            .f32(config.base_contrast)
            .f32(config.base_brightness)
            .f32(config.glint_contrast)
            .f32(config.glint_brightness)
            .f32(config.brightness_override)
            .f32(config.brightness_threshold)
            .vec2(self.font.glyph_msdf.size)
            .vec2(
                self.font
                    .glint_msdf
                    .as_ref()
                    .map_or([1.0, 1.0], |image| image.size),
            )
            .vec2(self.font.grid_size)
            .bool(self.base_texture.is_some())
            .bool(self.glint_texture.is_some())
            .bool(config.isolate_cursor)
            .bool(config.isolate_glint)
            .mat2(glyph_transform(config.glyph_flip, config.glyph_rotation))
            .vec2(slant_vec)
            .f32(slant_scale)
            .bool(config.is_polar)
            .vec2(screen_size)
            .f32(config.density)
            .f32(GLYPH_VERTICAL_SPACING)
            .vec2([1.0 / num_columns as f32, 1.0 / num_rows as f32])
            .f32(time)
            .f32(config.animation_speed)
            .f32(config.forward_speed)
            .mat4(view_projection);
        self.uniforms.write(queue, u);

        let pass = if config.volumetric {
            self.quad_grid.resize(device, num_columns, num_rows);
            &self.pass_volumetric
        } else {
            &self.pass_2d
        };

        let view = |image: &Option<(DecorativeTexture, GpuImage)>| match image {
            Some((_, image)) => image.view.clone(),
            None => self.white.view.clone(),
        };
        let base_view = view(&self.base_texture);
        let glint_view = view(&self.glint_texture);
        let glint_msdf_view = self
            .font
            .glint_msdf
            .as_ref()
            .map_or(&self.white.view, |image| &image.view);

        let bind_group = pass.bind_group(
            device,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(states[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(states[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(states[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.font.glyph_msdf.view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(glint_msdf_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&base_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&glint_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&samplers.nearest),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&samplers.linear),
                },
            ],
        );

        let mut rpass = gpu::begin_pass(encoder, target, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
        rpass.set_pipeline(&pass.pipeline);
        rpass.set_bind_group(0, &bind_group, &[]);
        if config.volumetric {
            self.quad_grid.draw(&mut rpass);
        } else {
            rpass.draw(0..3, 0..1);
        }
    }
}
