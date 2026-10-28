// Ports shaders/glsl/rainPass.raindrop.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). The core of the rain
// simulation: writes each cell's falling brightness, whether it's the
// "cursor" (brightest glyph at the bottom of a raindrop), and whether it's
// been activated yet by the intro reveal.

struct Params {
    time : f32,
    tick : f32,
    animation_speed : f32,
    fall_speed : f32,
    num_columns : f32,
    num_rows : f32,
    brightness_decay : f32,
    raindrop_length : f32,
    loops : u32,
    skip_intro : u32,
};

@group(0) @binding(0) var<uniform> p : Params;
@group(0) @binding(1) var previous_raindrop_state : texture_2d<f32>;
@group(0) @binding(2) var intro_state : texture_2d<f32>;
@group(0) @binding(3) var state_sampler : sampler;

// The rain's key underlying concept: glyphs that share a column light
// simultaneously and are brighter toward the bottom; those bright areas
// are truncated into raindrops.
fn get_rain_brightness(sim_time : f32, glyph_pos : vec2<f32>) -> f32 {
    let column_time_offset = random_float(vec2<f32>(glyph_pos.x, 0.0)) * 1000.0;
    var column_speed_offset = random_float(vec2<f32>(glyph_pos.x + 0.1, 0.0)) * 0.5 + 0.5;
    if (p.loops != 0u) {
        column_speed_offset = 0.5;
    }
    let column_time = column_time_offset + sim_time * p.fall_speed * column_speed_offset;
    var rain_time = (glyph_pos.y * 0.01 + column_time) / p.raindrop_length;
    if (p.loops == 0u) {
        rain_time = wobble(rain_time);
    }
    return 1.0 - fract(rain_time);
}

fn compute_result(
    sim_time : f32,
    is_first_frame : bool,
    glyph_pos : vec2<f32>,
    previous : vec4<f32>,
    intro : vec4<f32>,
) -> vec4<f32> {
    var brightness = get_rain_brightness(sim_time, glyph_pos);
    let brightness_below = get_rain_brightness(sim_time, glyph_pos + vec2<f32>(0.0, -1.0));

    let intro_progress = intro.r - (1.0 - glyph_pos.y / p.num_rows);
    let intro_progress_below = intro.r - (1.0 - (glyph_pos.y - 1.0) / p.num_rows);

    let skip_intro = p.skip_intro != 0u;
    let activated = previous.b != 0.0 || skip_intro || intro_progress > 0.0;
    let activated_below = skip_intro || intro_progress_below > 0.0;

    let cursor = brightness > brightness_below || (activated && !activated_below);

    // Blend with the previous frame's brightness so it winks on and off
    // organically instead of snapping instantly.
    if (!is_first_frame) {
        let previous_brightness = previous.r;
        brightness = mix(previous_brightness, brightness, p.brightness_decay);
    }

    return vec4<f32>(brightness, select(0.0, 1.0, cursor), select(0.0, 1.0, activated), intro_progress);
}

@fragment
fn fs_main(in : FullscreenOut) -> @location(0) vec4<f32> {
    let sim_time = p.time * p.animation_speed;
    let is_first_frame = p.tick <= 1.0;
    let glyph_pos = in.position.xy;
    let screen_pos = glyph_pos / vec2<f32>(p.num_columns, p.num_rows);
    let previous = textureSampleLevel(previous_raindrop_state, state_sampler, screen_pos, 0.0);
    let intro = textureSampleLevel(intro_state, state_sampler, vec2<f32>(screen_pos.x, 0.0), 0.0);
    return compute_result(sim_time, is_first_frame, glyph_pos, previous, intro);
}
