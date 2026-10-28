//! Dev tool: render the screensaver headlessly (no window, no compositor)
//! for a few seconds of simulated time, then save a PNG of the last frame
//! to the path given as the first argument. The optional second argument is
//! a preset name (`Classic`, `Megacity`, `Trinity`, `ThreeD`, ...), the
//! optional third the number of frames to simulate at 30 fps (default 240).

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use egui_screensaver_matrix::{MatrixBackground, MatrixConfig, Preset};
    use egui_wgpu::{RenderState, RendererOptions, ScreenDescriptor, WgpuConfiguration, wgpu};

    const WIDTH: u32 = 960;
    const HEIGHT: u32 = 600;

    env_logger::init();

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "screenshot.png".to_owned());
    let preset = match std::env::args().nth(2).as_deref() {
        Some("Megacity") => Preset::Megacity,
        Some("Neomatrixology") => Preset::Neomatrixology,
        Some("Operator") => Preset::Operator,
        Some("Nightmare") => Preset::Nightmare,
        Some("Paradise") => Preset::Paradise,
        Some("Resurrections") => Preset::Resurrections,
        Some("Palimpsest") => Preset::Palimpsest,
        Some("Twilight") => Preset::Twilight,
        Some("Trinity") => Preset::Trinity,
        Some("Morpheus") => Preset::Morpheus,
        Some("Bugs") => Preset::Bugs,
        Some("ThreeD") => Preset::ThreeD,
        _ => Preset::Classic,
    };
    let frames: u32 = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(240);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let state = pollster::block_on(RenderState::create(
        &WgpuConfiguration::default(),
        &instance,
        None,
        RendererOptions::default(),
    ))
    .expect("no usable GPU adapter");
    let (device, queue) = (&state.device, &state.queue);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: state.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let ctx = egui::Context::default();
    let mut matrix = MatrixBackground::new(MatrixConfig::from_preset(preset));
    let screen = ScreenDescriptor {
        size_in_pixels: [WIDTH, HEIGHT],
        pixels_per_point: 1.0,
    };

    for frame in 0..frames {
        let input = egui::RawInput {
            time: Some(frame as f64 / 30.0),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH as f32, HEIGHT as f32),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            matrix.paint(ui.ctx(), Some(&state));
        });
        let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);

        let mut renderer = state.renderer.write();
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let extra = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("screenshot pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        queue.submit(extra.into_iter().chain(Some(encoder.finish())));
        drop(renderer);
        for id in &output.textures_delta.free {
            state.renderer.write().free_texture(id);
        }
    }

    // Read the last frame back.
    let bytes_per_row = (WIDTH * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: (bytes_per_row * HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    readback.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("failed to map the readback buffer")
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll failed");

    let data = readback
        .slice(..)
        .get_mapped_range()
        .expect("failed to read the mapped buffer");
    let bgra = matches!(
        state.target_format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    let mut out = image::RgbaImage::new(WIDTH, HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * bytes_per_row + x * 4) as usize;
            let (r, b) = if bgra { (2, 0) } else { (0, 2) };
            out.put_pixel(
                x,
                y,
                image::Rgba([data[i + r], data[i + 1], data[i + b], 255]),
            );
        }
    }
    out.save(&path).expect("failed to save screenshot");
    println!("saved {path}");
}

#[cfg(target_arch = "wasm32")]
fn main() {}
