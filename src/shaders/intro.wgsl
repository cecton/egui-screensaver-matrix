// Ports shaders/glsl/rainPass.intro.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Writes the "intro"
// reveal state to the R channel of a 1-row-tall (numColumns x 1) buffer:
// how far the initial stream of rain has progressed down each column.

struct Params {
    time : f32,
    tick : f32,
    animation_speed : f32,
    fall_speed : f32,
    num_columns : f32,
    num_rows : f32,
    skip_intro : u32,
};

@group(0) @binding(0) var<uniform> p : Params;

fn compute_result(sim_time : f32, glyph_pos : vec2<f32>) -> vec4<f32> {
    if (p.skip_intro != 0u) {
        return vec4<f32>(2.0, 0.0, 0.0, 0.0);
    }

    var column_time_offset : f32;
    let column = i32(glyph_pos.x);
    if (column == i32(p.num_columns / 2.0)) {
        column_time_offset = -1.0;
    } else if (column == i32(p.num_columns * 0.75)) {
        column_time_offset = -2.0;
    } else {
        column_time_offset = random_float(vec2<f32>(glyph_pos.x, 0.0)) * -4.0;
        column_time_offset += (sin(glyph_pos.x / p.num_columns * PI) - 1.0) * 2.0 - 2.5;
    }
    let intro_time = (sim_time + column_time_offset) * p.fall_speed / p.num_rows * 100.0;

    return vec4<f32>(intro_time, 0.0, 0.0, 0.0);
}

@fragment
fn fs_main(in : FullscreenOut) -> @location(0) vec4<f32> {
    let sim_time = p.time * p.animation_speed;
    return compute_result(sim_time, in.position.xy);
}
