//! The volumetric-mode quad-grid mesh: one quad per glyph cell (rather
//! than the single fullscreen triangle the 2D render path uses), so each
//! glyph can be positioned independently in 3D and projected through the
//! volumetric camera. Ported from the vertex-buffer construction in
//! `js/regl/rainPass.js` at https://github.com/Rezmason/matrix (MIT
//! licensed).

use egui_wgpu::wgpu;
use wgpu::util::DeviceExt as _;

/// The two triangles making up one quad, as `(aPosition-relative corner)`
/// pairs — ported verbatim from the original's `tlVert`/`trVert`/
/// `blVert`/`brVert` / `quadVertices` ordering.
const CORNERS: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [0.0, 1.0],
    [1.0, 1.0],
    [0.0, 0.0],
    [1.0, 1.0],
    [1.0, 0.0],
];

fn build_vertex_data(num_quad_columns: u32, num_quad_rows: u32) -> Vec<u8> {
    let mut data =
        Vec::with_capacity((num_quad_columns * num_quad_rows * 6 * 4) as usize * size_of::<f32>());
    for y in 0..num_quad_rows {
        for x in 0..num_quad_columns {
            for corner in CORNERS {
                for value in [x as f32, y as f32, corner[0], corner[1]] {
                    data.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
    }
    data
}

/// Vertex attributes of [`QuadGrid`]: `a_position` (the cell's grid
/// coordinate, constant across the quad's 6 vertices) at location 0 and
/// `a_corner` (which corner of the quad this vertex is) at location 1.
pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

pub const VERTEX_STRIDE: u64 = 4 * size_of::<f32>() as u64;

/// One quad per glyph cell.
pub struct QuadGrid {
    buffer: wgpu::Buffer,
    vertex_count: u32,
    num_quad_columns: u32,
    num_quad_rows: u32,
}

impl QuadGrid {
    pub fn new(device: &wgpu::Device, num_quad_columns: u32, num_quad_rows: u32) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("egui-screensaver-matrix quad grid"),
            contents: &build_vertex_data(num_quad_columns, num_quad_rows),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Self {
            buffer,
            vertex_count: num_quad_columns * num_quad_rows * 6,
            num_quad_columns,
            num_quad_rows,
        }
    }

    /// Rebuilds the mesh if the grid dimensions changed.
    pub fn resize(&mut self, device: &wgpu::Device, num_quad_columns: u32, num_quad_rows: u32) {
        if num_quad_columns != self.num_quad_columns || num_quad_rows != self.num_quad_rows {
            *self = Self::new(device, num_quad_columns, num_quad_rows);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}
