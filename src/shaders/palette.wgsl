// Ports shaders/glsl/palettePass.frag.glsl and stripePass.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Maps the rendered
// rain+bloom brightness to color, either via a smooth gradient texture
// built from the config's palette stops (`fs_palette`) or by multiplying it
// with a custom repeating/sweeping color sequence sampled at screen UV —
// the pride/trans/custom-stripes effects (`fs_stripes`) — adding isolated
// cursor/glint highlight colors and a background color on top. A little
// per-pixel dither hides banding in subtle gradients.

struct Params {
    dither_magnitude : f32,
    time : f32,
    cursor_intensity : f32,
    glint_intensity : f32,
    background_color : vec3<f32>,
    cursor_color : vec3<f32>,
    glint_color : vec3<f32>,
};

@group(0) @binding(0) var<uniform> p : Params;
@group(0) @binding(1) var tex : texture_2d<f32>;
@group(0) @binding(2) var bloom_tex : texture_2d<f32>;
@group(0) @binding(3) var color_tex : texture_2d<f32>;
@group(0) @binding(4) var samp : sampler;

fn rand(uv : vec2<f32>, t : f32) -> f32 {
    let dt = dot(uv, vec2<f32>(12.9898, 78.233));
    let sn = glsl_mod(dt, PI);
    return fract(sin(sn) * 43758.5453 + t);
}

fn get_brightness(uv : vec2<f32>, frag_coord : vec2<f32>) -> vec4<f32> {
    let primary = textureSampleLevel(tex, samp, uv, 0.0);
    let bloom = textureSampleLevel(bloom_tex, samp, uv, 0.0);
    var brightness = primary + bloom;
    // Dither: subtract a random value from the brightness.
    brightness -= rand(frag_coord, p.time) * p.dither_magnitude / 3.0;
    return brightness;
}

fn highlights(brightness : vec4<f32>) -> vec3<f32> {
    return min(p.cursor_color * p.cursor_intensity * brightness.g, vec3<f32>(1.0))
        + min(p.glint_color * p.glint_intensity * brightness.b, vec3<f32>(1.0))
        + p.background_color;
}

@fragment
fn fs_palette(in : FullscreenOut) -> @location(0) vec4<f32> {
    let brightness = get_brightness(in.uv, in.position.xy);
    let base = textureSampleLevel(color_tex, samp, vec2<f32>(brightness.r, 0.0), 0.0).rgb;
    return vec4<f32>(base + highlights(brightness), 1.0);
}

@fragment
fn fs_stripes(in : FullscreenOut) -> @location(0) vec4<f32> {
    let color = textureSampleLevel(color_tex, samp, in.uv, 0.0).rgb;
    let brightness = get_brightness(in.uv, in.position.xy);
    return vec4<f32>(color * brightness.r + highlights(brightness), 1.0);
}
