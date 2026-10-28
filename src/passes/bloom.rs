//! The bloom pass: a classic 5-level pyramid (highpass -> horizontal
//! blur -> vertical blur per level, each level half the resolution of the
//! previous) flattened into one bloom texture, ported from
//! `js/regl/bloomPass.js` / `shaders/glsl/bloomPass.*.frag.glsl` at
//! https://github.com/Rezmason/matrix (MIT licensed).

use egui_wgpu::wgpu;

use crate::config::MatrixConfig;
use crate::gpu::{
    self, PassPipeline, RenderTarget, Samplers, TargetFormat, UniformBuffer, Uniforms,
};

const HIGHPASS_SRC: &str = include_str!("../shaders/bloom_highpass.wgsl");
const BLUR_SRC: &str = include_str!("../shaders/bloom_blur.wgsl");
const COMBINE_SRC: &str = include_str!("../shaders/bloom_combine.wgsl");

const PYRAMID_HEIGHT: usize = 5;

const FRAGMENT: wgpu::ShaderStages = wgpu::ShaderStages::FRAGMENT;

/// One pyramid level's 3 render targets: highpass -> horizontal blur ->
/// vertical blur, each at the same (progressively smaller) resolution, plus
/// the blur uniforms (which only depend on the level's size) and the bind
/// groups wiring the three passes together.
struct PyramidLevel {
    highpass: RenderTarget,
    h_blur: RenderTarget,
    v_blur: RenderTarget,
    h_uniforms: UniformBuffer,
    v_uniforms: UniformBuffer,
    highpass_group: Option<wgpu::BindGroup>,
    h_group: Option<wgpu::BindGroup>,
    v_group: Option<wgpu::BindGroup>,
}

impl PyramidLevel {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let target = || RenderTarget::new(device, width, height, TargetFormat::Rgba8);
        Self {
            highpass: target(),
            h_blur: target(),
            v_blur: target(),
            h_uniforms: UniformBuffer::new(device, "matrix blur h", 16),
            v_uniforms: UniformBuffer::new(device, "matrix blur v", 16),
            highpass_group: None,
            h_group: None,
            v_group: None,
        }
    }
}

/// Level `i`'s resolution: `floor(screen_size * bloom_size / 2^i)`,
/// clamped to at least 1 pixel so a very small `bloom_size` can't produce
/// an invalid 0x0 render target.
fn level_size(screen_size: u32, bloom_size: f32, level: usize) -> u32 {
    (((screen_size as f32 * bloom_size) as u32) >> level).max(1)
}

pub struct BloomPasses {
    highpass_pass: PassPipeline,
    highpass_uniforms: UniformBuffer,
    blur_pass: PassPipeline,
    combine_pass: PassPipeline,
    combine_uniforms: UniformBuffer,
    combine_output: RenderTarget,
    combine_group: Option<wgpu::BindGroup>,

    levels: [PyramidLevel; PYRAMID_HEIGHT],
    screen_width: u32,
    screen_height: u32,
    bloom_size: f32,
}

impl BloomPasses {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        samplers: &Samplers,
        primary: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        bloom_size: f32,
    ) -> Self {
        let format = TargetFormat::Rgba8.wgpu();
        let single = |label: &str, src: &str, entry: &str| {
            let module = gpu::shader_module(device, label, src);
            PassPipeline::new(
                device,
                label,
                &module,
                "vs_offscreen",
                entry,
                &[
                    gpu::uniform_entry(0, FRAGMENT),
                    gpu::texture_entry(1, FRAGMENT),
                    gpu::sampler_entry(2, FRAGMENT),
                ],
                &[],
                format,
                None,
            )
        };
        let highpass_pass = single("matrix highpass", HIGHPASS_SRC, "fs_highpass");
        let blur_pass = single("matrix blur", BLUR_SRC, "fs_blur");

        let combine_module = gpu::shader_module(device, "matrix combine", COMBINE_SRC);
        let mut combine_entries = vec![gpu::uniform_entry(0, FRAGMENT)];
        combine_entries
            .extend((1..=PYRAMID_HEIGHT as u32).map(|i| gpu::texture_entry(i, FRAGMENT)));
        combine_entries.push(gpu::sampler_entry(PYRAMID_HEIGHT as u32 + 1, FRAGMENT));
        let combine_pass = PassPipeline::new(
            device,
            "matrix combine",
            &combine_module,
            "vs_offscreen",
            "fs_combine",
            &combine_entries,
            &[],
            format,
            None,
        );

        let levels = std::array::from_fn(|i| {
            PyramidLevel::new(
                device,
                level_size(screen_width, bloom_size, i),
                level_size(screen_height, bloom_size, i),
            )
        });

        let mut this = Self {
            highpass_pass,
            highpass_uniforms: UniformBuffer::new(device, "matrix highpass", 16),
            blur_pass,
            combine_pass,
            combine_uniforms: UniformBuffer::new(device, "matrix combine", 16),
            combine_output: RenderTarget::new(
                device,
                screen_width,
                screen_height,
                TargetFormat::Rgba8,
            ),
            combine_group: None,
            levels,
            screen_width,
            screen_height,
            bloom_size,
        };
        this.rebuild_groups(device, queue, samplers, primary);
        this
    }

    /// Recreates the pyramid when the screen size or `bloom_size` changed.
    /// `primary` must be the (already resized) texture `run` reads from.
    #[allow(clippy::too_many_arguments)]
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        samplers: &Samplers,
        primary: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        bloom_size: f32,
    ) {
        if screen_width == self.screen_width
            && screen_height == self.screen_height
            && bloom_size == self.bloom_size
        {
            return;
        }
        self.combine_output
            .resize(device, screen_width, screen_height);
        for (i, level) in self.levels.iter_mut().enumerate() {
            *level = PyramidLevel::new(
                device,
                level_size(screen_width, bloom_size, i),
                level_size(screen_height, bloom_size, i),
            );
        }
        self.screen_width = screen_width;
        self.screen_height = screen_height;
        self.bloom_size = bloom_size;
        self.rebuild_groups(device, queue, samplers, primary);
    }

    /// (Re)writes the size-dependent blur uniforms and builds every bind
    /// group; all of them only change when a texture is recreated.
    fn rebuild_groups(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        samplers: &Samplers,
        primary: &wgpu::TextureView,
    ) {
        let mut level_groups = Vec::with_capacity(PYRAMID_HEIGHT);
        for i in 0..PYRAMID_HEIGHT {
            let input = if i == 0 {
                primary
            } else {
                &self.levels[i - 1].highpass.view
            };
            let level = &self.levels[i];
            let (width, height) = (level.highpass.width as f32, level.highpass.height as f32);

            let mut u = Uniforms::new();
            u.f32(width).f32(height).vec2([1.0, 0.0]);
            level.h_uniforms.write(queue, u);
            let mut u = Uniforms::new();
            u.f32(width).f32(height).vec2([0.0, 1.0]);
            level.v_uniforms.write(queue, u);

            let highpass_group = self.highpass_pass.bind_group(
                device,
                &[
                    gpu::entry(0, self.highpass_uniforms.binding()),
                    gpu::entry(1, wgpu::BindingResource::TextureView(input)),
                    gpu::entry(2, wgpu::BindingResource::Sampler(&samplers.linear)),
                ],
            );
            let h_group = self.blur_pass.bind_group(
                device,
                &[
                    gpu::entry(0, level.h_uniforms.binding()),
                    gpu::entry(1, wgpu::BindingResource::TextureView(&level.highpass.view)),
                    gpu::entry(2, wgpu::BindingResource::Sampler(&samplers.linear)),
                ],
            );
            let v_group = self.blur_pass.bind_group(
                device,
                &[
                    gpu::entry(0, level.v_uniforms.binding()),
                    gpu::entry(1, wgpu::BindingResource::TextureView(&level.h_blur.view)),
                    gpu::entry(2, wgpu::BindingResource::Sampler(&samplers.linear)),
                ],
            );
            level_groups.push((highpass_group, h_group, v_group));
        }
        for (level, (highpass, h, v)) in self.levels.iter_mut().zip(level_groups) {
            level.highpass_group = Some(highpass);
            level.h_group = Some(h);
            level.v_group = Some(v);
        }

        let mut entries = vec![gpu::entry(0, self.combine_uniforms.binding())];
        for (i, level) in self.levels.iter().enumerate() {
            entries.push(gpu::entry(
                i as u32 + 1,
                wgpu::BindingResource::TextureView(&level.v_blur.view),
            ));
        }
        entries.push(gpu::entry(
            PYRAMID_HEIGHT as u32 + 1,
            wgpu::BindingResource::Sampler(&samplers.linear),
        ));
        self.combine_group = Some(self.combine_pass.bind_group(device, &entries));
    }

    /// Records the full highpass -> blur -> blur -> combine pyramid (reading
    /// the `primary` texture handed to `new`/`resize`) and returns the
    /// resulting bloom texture.
    pub fn run(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        config: &MatrixConfig,
    ) -> &wgpu::TextureView {
        let mut u = Uniforms::new();
        u.f32(config.high_pass_threshold);
        self.highpass_uniforms.write(queue, u);
        let mut u = Uniforms::new();
        u.f32(config.bloom_strength);
        self.combine_uniforms.write(queue, u);

        for level in &self.levels {
            let (Some(highpass), Some(h), Some(v)) =
                (&level.highpass_group, &level.h_group, &level.v_group)
            else {
                continue;
            };
            gpu::draw_fullscreen(encoder, &level.highpass.view, &self.highpass_pass, highpass);
            gpu::draw_fullscreen(encoder, &level.h_blur.view, &self.blur_pass, h);
            gpu::draw_fullscreen(encoder, &level.v_blur.view, &self.blur_pass, v);
        }

        if let Some(group) = &self.combine_group {
            gpu::draw_fullscreen(
                encoder,
                &self.combine_output.view,
                &self.combine_pass,
                group,
            );
        }
        &self.combine_output.view
    }

    pub fn output(&self) -> &wgpu::TextureView {
        &self.combine_output.view
    }
}
