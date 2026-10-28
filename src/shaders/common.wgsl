// Helpers shared by every pass, prepended to each pass's own WGSL (see
// `gpu::shader_module`). Ported from the GLSL shaders of
// https://github.com/Rezmason/matrix (MIT licensed).

const PI : f32 = 3.14159265359;
const SQRT_2 : f32 = 1.4142135623730951;
const SQRT_5 : f32 = 2.23606797749979;

// GLSL's `mod`: `x - y * floor(x / y)`, unlike WGSL's truncating `%`.
fn glsl_mod(x : f32, y : f32) -> f32 {
    return x - y * floor(x / y);
}

fn random_float(uv : vec2<f32>) -> f32 {
    let dt = dot(uv, vec2<f32>(12.9898, 78.233));
    let sn = glsl_mod(dt, PI);
    return fract(sin(sn) * 43758.5453);
}

fn wobble(x : f32) -> f32 {
    return x + 0.3 * sin(SQRT_2 * x) + 0.2 * sin(SQRT_5 * x);
}

struct FullscreenOut {
    @builtin(position) position : vec4<f32>,
    @location(0) uv : vec2<f32>,
};

// A single triangle covering the whole clip-space square and beyond
// (vertices at (-1,-1), (3,-1), (-1,3)); the parts outside [-1,1] are
// clipped away, leaving a fullscreen quad with no extra draw call.
fn fullscreen_corner(index : u32) -> vec2<f32> {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return corners[index];
}

// Offscreen passes negate clip-space y so that framebuffer row 0 is the
// "bottom", as in GL (see the module docs of `gpu.rs`). `uv` keeps the GL
// convention (0 at the bottom) either way.
@vertex
fn vs_offscreen(@builtin(vertex_index) index : u32) -> FullscreenOut {
    let pos = fullscreen_corner(index);
    var out : FullscreenOut;
    out.uv = pos * 0.5 + 0.5;
    out.position = vec4<f32>(pos.x, -pos.y, 0.0, 1.0);
    return out;
}

// The last pass draws straight into egui's render pass, which is unflipped.
@vertex
fn vs_screen(@builtin(vertex_index) index : u32) -> FullscreenOut {
    let pos = fullscreen_corner(index);
    var out : FullscreenOut;
    out.uv = pos * 0.5 + 0.5;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    return out;
}
