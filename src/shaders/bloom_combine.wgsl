// Flattens the 5-level blur pyramid (each level a different resolution, so
// sampling with linear filtering upsamples it) into one bloom texture.
// Ports shaders/glsl/bloomPass.combine.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed).

struct CombineParams {
    bloom_strength : f32,
};
@group(0) @binding(0) var<uniform> combine : CombineParams;
@group(0) @binding(1) var pyr_0 : texture_2d<f32>;
@group(0) @binding(2) var pyr_1 : texture_2d<f32>;
@group(0) @binding(3) var pyr_2 : texture_2d<f32>;
@group(0) @binding(4) var pyr_3 : texture_2d<f32>;
@group(0) @binding(5) var pyr_4 : texture_2d<f32>;
@group(0) @binding(6) var samp : sampler;

@fragment
fn fs_combine(in : FullscreenOut) -> @location(0) vec4<f32> {
    var total = vec4<f32>(0.0);
    total += textureSampleLevel(pyr_0, samp, in.uv, 0.0) * 0.96549;
    total += textureSampleLevel(pyr_1, samp, in.uv, 0.0) * 0.92832;
    total += textureSampleLevel(pyr_2, samp, in.uv, 0.0) * 0.88790;
    total += textureSampleLevel(pyr_3, samp, in.uv, 0.0) * 0.84343;
    total += textureSampleLevel(pyr_4, samp, in.uv, 0.0) * 0.79370;
    return total * combine.bloom_strength;
}
