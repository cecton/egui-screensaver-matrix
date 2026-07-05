#if NEW_SHADER_INTERFACE
    in vec2 a_pos;
    out vec2 v_uv;
#else
    attribute vec2 a_pos;
    varying vec2 v_uv;
#endif

// A single triangle covering the whole clip-space square and beyond
// (vertices at (-1,-1), (3,-1), (-1,3)); the parts outside [-1,1] are
// clipped away, leaving a fullscreen quad with no extra draw call. Shared
// by every fullscreen pass in this crate (compute passes ignore `v_uv` and
// use `gl_FragCoord` instead, matching the original; the render pass uses
// `v_uv` to sample the compute-state textures).
void main() {
    v_uv = a_pos * 0.5 + 0.5;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
