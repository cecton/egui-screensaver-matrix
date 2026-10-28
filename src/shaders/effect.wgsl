// Ports shaders/glsl/rainPass.effect.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Computes the
// non-canon "spice" effects — thunder flashes and ripple distortion —
// that multiply/add to a cell's brightness.
//
// Note: the original declares/samples a `previousEffectState` uniform here
// too, but never actually uses the result (`computeResult` ignores it) —
// this pass has no frame-to-frame accumulation, so it's omitted here.

struct Params {
    time : f32,
    tick : f32,
    animation_speed : f32,
    num_columns : f32,
    num_rows : f32,
    glyph_height_to_width : f32,
    ripple_scale : f32,
    ripple_speed : f32,
    ripple_thickness : f32,
    ripple_type : i32,
    has_thunder : u32,
    loops : u32,
};

@group(0) @binding(0) var<uniform> p : Params;

fn random_vec2(uv : vec2<f32>) -> vec2<f32> {
    return fract(vec2<f32>(sin(uv.x * 591.32 + uv.y * 154.077), cos(uv.x * 391.32 + uv.y * 49.077)));
}

fn get_thunder(sim_time : f32, screen_pos : vec2<f32>) -> f32 {
    if (p.has_thunder == 0u) {
        return 0.0;
    }

    let thunder_time = sim_time * 0.5;
    var thunder = 1.0 - fract(wobble(thunder_time));
    if (p.loops != 0u) {
        thunder = 1.0 - fract(thunder_time + 0.3);
    }

    thunder = log(thunder * 1.5) * 4.0;
    thunder = clamp(thunder, 0.0, 1.0) * 10.0 * pow(screen_pos.y, 2.0);
    return thunder;
}

fn get_ripple(sim_time : f32, screen_pos : vec2<f32>) -> f32 {
    if (p.ripple_type == -1) {
        return 0.0;
    }

    var ripple_time = (sim_time * 0.5 + sin(sim_time) * 0.2) * p.ripple_speed + 1.0;
    if (p.loops != 0u) {
        ripple_time = (sim_time * 0.5) * p.ripple_speed + 1.0;
    }

    var offset = random_vec2(vec2<f32>(floor(ripple_time), 0.0)) - 0.5;
    if (p.loops != 0u) {
        offset = vec2<f32>(0.0);
    }
    let ripple_pos = screen_pos * 2.0 - 1.0 + offset;
    var ripple_distance : f32;
    if (p.ripple_type == 0) {
        let box_distance = abs(ripple_pos) * vec2<f32>(1.0, p.glyph_height_to_width);
        ripple_distance = max(box_distance.x, box_distance.y);
    } else {
        ripple_distance = length(ripple_pos);
    }

    let ripple_value = fract(ripple_time) * p.ripple_scale - ripple_distance;

    if (ripple_value > 0.0 && ripple_value < p.ripple_thickness) {
        return 0.75;
    }

    return 0.0;
}

fn compute_result(sim_time : f32, screen_pos : vec2<f32>) -> vec4<f32> {
    let multiplied_effects = 1.0 + get_thunder(sim_time, screen_pos);
    let added_effects = get_ripple(sim_time, screen_pos);
    return vec4<f32>(multiplied_effects, added_effects, 0.0, 0.0);
}

@fragment
fn fs_main(in : FullscreenOut) -> @location(0) vec4<f32> {
    let sim_time = p.time * p.animation_speed;
    let screen_pos = in.position.xy / vec2<f32>(p.num_columns, p.num_rows);
    return compute_result(sim_time, screen_pos);
}
