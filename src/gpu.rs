//! Shared wgpu helpers used by every pass: offscreen render targets,
//! ping-pong pairs, bind-group-layout shorthands, the fullscreen-triangle
//! draw, PNG/RGBA texture uploads and a tiny std140-style uniform writer.
//!
//! # Orientation
//!
//! The shaders are ported from GLSL, where a framebuffer's row 0 is its
//! *bottom* and `gl_FragCoord.y` counts up. wgpu's framebuffer row 0 is its
//! *top*. Rather than re-derive every pass's math, every offscreen pass in
//! this crate draws with its clip-space `y` negated (the `vs_*_offscreen`
//! entry points in `shaders/`), which puts GL's "bottom" at wgpu's row 0
//! again: `@builtin(position).y`, texture `v` and stored rows then mean
//! exactly what `gl_FragCoord.y`, `texture2D` and GL rows did. Only the last
//! pass, which draws into egui's own (unflipped) render pass, does not
//! negate `y`.

use egui_wgpu::wgpu;
use wgpu::util::DeviceExt as _;

/// Prepended to every pass's WGSL: the shared RNG/`mod` helpers and the
/// fullscreen-triangle vertex stages.
pub const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");

/// Builds a WGSL module from [`COMMON_WGSL`] plus a pass's own source.
pub fn shader_module(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(format!("{COMMON_WGSL}\n{source}").into()),
    })
}

/// Little-endian writer producing the byte layout WGSL expects for a
/// `var<uniform>` struct whose fields are pushed in declaration order.
#[derive(Default)]
pub struct Uniforms(Vec<u8>);

impl Uniforms {
    pub fn new() -> Self {
        Self::default()
    }

    fn align(&mut self, align: usize) {
        while !self.0.len().is_multiple_of(align) {
            self.0.push(0);
        }
    }

    pub fn f32(&mut self, value: f32) -> &mut Self {
        self.align(4);
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.align(4);
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn i32(&mut self, value: i32) -> &mut Self {
        self.align(4);
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn bool(&mut self, value: bool) -> &mut Self {
        self.u32(value as u32)
    }

    pub fn vec2(&mut self, value: [f32; 2]) -> &mut Self {
        self.align(8);
        for v in value {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self
    }

    pub fn vec3(&mut self, value: [f32; 3]) -> &mut Self {
        self.align(16);
        for v in value {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self
    }

    /// `mat2x2<f32>`, column-major.
    pub fn mat2(&mut self, value: [f32; 4]) -> &mut Self {
        self.align(8);
        for v in value {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self
    }

    /// `mat4x4<f32>`, column-major.
    pub fn mat4(&mut self, value: [f32; 16]) -> &mut Self {
        self.align(16);
        for v in value {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self
    }

    /// Pads to the 16-byte multiple uniform buffers are sized in.
    pub fn finish(mut self) -> Vec<u8> {
        self.align(16);
        self.0
    }
}

/// A uniform buffer sized once and rewritten with [`UniformBuffer::write`].
pub struct UniformBuffer {
    pub buffer: wgpu::Buffer,
    size: u64,
}

impl UniformBuffer {
    pub fn new(device: &wgpu::Device, label: &str, size: u64) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, size }
    }

    pub fn write(&self, queue: &wgpu::Queue, uniforms: Uniforms) {
        let bytes = uniforms.finish();
        debug_assert_eq!(bytes.len() as u64, self.size, "uniform layout mismatch");
        queue.write_buffer(&self.buffer, 0, &bytes);
    }

    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }
}

/// Texture formats used by the pipeline. The rain "compute" buffers need
/// [`TargetFormat::Rgba16F`] (real fractional values, not just 0..255) to
/// hold brightness/age/decay state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetFormat {
    Rgba16F,
    Rgba8,
}

impl TargetFormat {
    pub fn wgpu(self) -> wgpu::TextureFormat {
        match self {
            TargetFormat::Rgba16F => wgpu::TextureFormat::Rgba16Float,
            TargetFormat::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

/// A texture that can be rendered into and then sampled in a later pass.
pub struct RenderTarget {
    _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub format: TargetFormat,
}

impl RenderTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, format: TargetFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("egui-screensaver-matrix target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format.wgpu(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
            width: width.max(1),
            height: height.max(1),
            format,
        }
    }

    /// Recreates this target at a new size if it differs from the current
    /// one. Transient contents are lost (matches the original's own
    /// resize behavior, which recreates its compute buffers from scratch).
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width.max(1) && self.height == height.max(1) {
            return;
        }
        *self = Self::new(device, width, height, self.format);
    }
}

/// A ping-ponged pair of [`RenderTarget`]s for a value that's computed each
/// frame from its own previous frame's output (the rain simulation's
/// "compute" buffers). `write()` is where this frame's pass draws into;
/// after [`PingPong::swap`] it becomes `read()`, which is what later passes
/// of the same frame sample. Before the swap, `read()` is last frame's
/// output, fed in as `previousXState`.
pub struct PingPong {
    a: RenderTarget,
    b: RenderTarget,
    write_is_a: bool,
}

impl PingPong {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, format: TargetFormat) -> Self {
        Self {
            a: RenderTarget::new(device, width, height, format),
            b: RenderTarget::new(device, width, height, format),
            write_is_a: true,
        }
    }

    pub fn write(&self) -> &RenderTarget {
        if self.write_is_a { &self.a } else { &self.b }
    }

    pub fn read(&self) -> &RenderTarget {
        if self.write_is_a { &self.b } else { &self.a }
    }

    pub fn swap(&mut self) {
        self.write_is_a = !self.write_is_a;
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.a.resize(device, width, height);
        self.b.resize(device, width, height);
    }
}

/// The two samplers every pass shares: `nearest` for the rain state
/// buffers (exact indices/booleans that must not be interpolated) and
/// `linear` for MSDF atlases, decorative textures and the bloom pyramid.
/// Both clamp to the edge.
pub struct Samplers {
    pub nearest: wgpu::Sampler,
    pub linear: wgpu::Sampler,
}

impl Samplers {
    pub fn new(device: &wgpu::Device) -> Self {
        let make = |filter, label| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        Self {
            nearest: make(wgpu::FilterMode::Nearest, "egui-screensaver-matrix nearest"),
            linear: make(wgpu::FilterMode::Linear, "egui-screensaver-matrix linear"),
        }
    }
}

/// A bind group entry (a function rather than a closure so each call gets
/// its own resource lifetime).
pub fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

pub fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn texture_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub fn sampler_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

/// Everything needed to draw one pass: its pipeline and bind group layout.
pub struct PassPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
}

impl PassPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        label: &str,
        module: &wgpu::ShaderModule,
        vertex_entry: &str,
        fragment_entry: &str,
        entries: &[wgpu::BindGroupLayoutEntry],
        vertex_buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
        format: wgpu::TextureFormat,
        blend: Option<wgpu::BlendState>,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(vertex_entry),
                compilation_options: Default::default(),
                buffers: vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }

    pub fn bind_group(
        &self,
        device: &wgpu::Device,
        entries: &[wgpu::BindGroupEntry<'_>],
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("egui-screensaver-matrix bind group"),
            layout: &self.layout,
            entries,
        })
    }
}

/// Draws one fullscreen triangle into `target` (clearing it first).
pub fn draw_fullscreen(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    pass: &PassPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut rpass = begin_pass(encoder, target, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
    rpass.set_pipeline(&pass.pipeline);
    rpass.set_bind_group(0, bind_group, &[]);
    rpass.draw(0..3, 0..1);
}

pub fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("egui-screensaver-matrix pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// Uploads tightly packed RGBA8 pixels as a linear-filtered 2D texture.
pub fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("egui-screensaver-matrix texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    )
}

/// Uploads a PNG's bytes as an `RGBA8` 2D texture. Flips the image
/// vertically before upload: the original's texture-coordinate math (e.g.
/// `getSymbolUV`'s row flip in `render.wgsl`) assumes texture `v = 0` is the
/// *bottom* of the source image, as WebGL's `UNPACK_FLIP_Y_WEBGL`-style
/// upload gives it. Uploading the file's top-to-bottom row order unchanged
/// would invert which half of the atlas each glyph index resolves to. This
/// is invisible for glyph sets that fill nearly their whole grid (e.g.
/// `matrixcode`), since almost every row has *some* glyph either way, but is
/// fatal for sparser fonts (e.g. `gothic`, `coptic`), whose real glyphs sit
/// only in the top rows and render as fully blank without this flip.
pub fn upload_png_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    png_bytes: &[u8],
) -> Result<(wgpu::Texture, [f32; 2]), String> {
    let img = image::load_from_memory(png_bytes)
        .map_err(|err| format!("failed to decode bundled PNG: {err}"))?
        .flipv()
        .to_rgba8();
    let (width, height) = img.dimensions();
    let texture = upload_rgba(device, queue, img.as_raw(), width, height);
    Ok((texture, [width as f32, height as f32]))
}
