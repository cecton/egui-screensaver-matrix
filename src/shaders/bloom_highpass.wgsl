// Ports shaders/glsl/bloomPass.{highPass,blur,combine}.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed).

@group(0) @binding(1) var tex : texture_2d<f32>;
@group(0) @binding(2) var samp : sampler;

// Zeroes out any channel dimmer than `threshold`, so only bright areas
// contribute to the blur that follows.
struct HighPassParams {
    threshold : f32,
};
@group(0) @binding(0) var<uniform> high_pass : HighPassParams;

@fragment
fn fs_highpass(in : FullscreenOut) -> @location(0) vec4<f32> {
    var color = textureSampleLevel(tex, samp, in.uv, 0.0);
    if (color.r < high_pass.threshold) { color.r = 0.0; }
    if (color.g < high_pass.threshold) { color.g = 0.0; }
    if (color.b < high_pass.threshold) { color.b = 0.0; }
    return color;
}
