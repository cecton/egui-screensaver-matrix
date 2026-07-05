//! The volumetric-mode quad-grid mesh: one quad per glyph cell (rather
//! than the single fullscreen triangle the 2D render path uses), so each
//! glyph can be positioned independently in 3D and projected through the
//! volumetric camera. Ported from the vertex-buffer construction in
//! `js/regl/rainPass.js` at https://github.com/Rezmason/matrix (MIT
//! licensed).

use glow::HasContext as _;

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

fn f32_slice_as_bytes(floats: &[f32]) -> &[u8] {
    // SAFETY: `f32` has no padding/invalid bit patterns, and the resulting
    // slice's lifetime and length are derived directly from `floats`.
    unsafe {
        std::slice::from_raw_parts(floats.as_ptr().cast::<u8>(), std::mem::size_of_val(floats))
    }
}

fn build_vertex_data(num_quad_columns: u32, num_quad_rows: u32) -> Vec<f32> {
    let mut data = Vec::with_capacity((num_quad_columns * num_quad_rows * 6 * 4) as usize);
    for y in 0..num_quad_rows {
        for x in 0..num_quad_columns {
            for corner in CORNERS {
                data.push(x as f32);
                data.push(y as f32);
                data.push(corner[0]);
                data.push(corner[1]);
            }
        }
    }
    data
}

/// One quad per glyph cell, each with an `aPosition` (the cell's grid
/// coordinate, constant across the quad's 6 vertices) and `aCorner`
/// (which corner of the quad this vertex is, varying per-vertex).
pub struct QuadGrid {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    vertex_count: i32,
    num_quad_columns: u32,
    num_quad_rows: u32,
}

impl QuadGrid {
    pub unsafe fn new(
        gl: &glow::Context,
        program: glow::Program,
        num_quad_columns: u32,
        num_quad_rows: u32,
    ) -> Result<Self, String> {
        unsafe {
            let data = build_vertex_data(num_quad_columns, num_quad_rows);
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                f32_slice_as_bytes(&data),
                glow::STATIC_DRAW,
            );

            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let stride = 4 * size_of::<f32>() as i32;
            if let Some(loc) = gl.get_attrib_location(program, "aPosition") {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, 2, glow::FLOAT, false, stride, 0);
            }
            if let Some(loc) = gl.get_attrib_location(program, "aCorner") {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(
                    loc,
                    2,
                    glow::FLOAT,
                    false,
                    stride,
                    2 * size_of::<f32>() as i32,
                );
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            Ok(Self {
                vao,
                vbo,
                vertex_count: (num_quad_columns * num_quad_rows * 6) as i32,
                num_quad_columns,
                num_quad_rows,
            })
        }
    }

    /// Rebuilds the mesh if the grid dimensions changed.
    pub unsafe fn resize(
        &mut self,
        gl: &glow::Context,
        program: glow::Program,
        num_quad_columns: u32,
        num_quad_rows: u32,
    ) -> Result<(), String> {
        unsafe {
            if num_quad_columns == self.num_quad_columns && num_quad_rows == self.num_quad_rows {
                return Ok(());
            }
            let new = Self::new(gl, program, num_quad_columns, num_quad_rows)?;
            self.destroy(gl);
            *self = new;
            Ok(())
        }
    }

    pub unsafe fn draw(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, self.vertex_count);
            gl.bind_vertex_array(None);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
        }
    }
}
