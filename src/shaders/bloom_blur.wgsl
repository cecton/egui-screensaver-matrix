// A cheap 3-tap separable blur; drawn once with `direction = (1,0)`
// (horizontal) and once with `direction = (0,1)` (vertical) to approximate
// a 2D gaussian. `width`/`height` are the current render target's
// resolution (not the screen's), used to keep the blur radius
// aspect-correct. Ports shaders/glsl/bloomPass.blur.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed).

struct BlurParams {
    width : f32,
    height : f32,
    direction : vec2<f32>,
};
@group(0) @binding(0) var<uniform> blur : BlurParams;
@group(0) @binding(1) var tex : texture_2d<f32>;
@group(0) @binding(2) var samp : sampler;

@fragment
fn fs_blur(in : FullscreenOut) -> @location(0) vec4<f32> {
    var size = vec2<f32>(1.0, blur.height / blur.width);
    if (blur.width > blur.height) {
        size = vec2<f32>(blur.width / blur.height, 1.0);
    }
    let step = blur.direction / max(blur.width, blur.height) * size;
    return textureSampleLevel(tex, samp, in.uv, 0.0) * 0.442 +
        (
            textureSampleLevel(tex, samp, in.uv + step, 0.0) +
            textureSampleLevel(tex, samp, in.uv - step, 0.0)
        ) * 0.279;
}
