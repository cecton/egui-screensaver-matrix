// Ports shaders/glsl/rainPass.symbol.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Governs which glyph
// appears in each cell and how long it's been showing that glyph, so
// glyphs cycle to a new random symbol periodically rather than every
// frame.

struct Params {
    time : f32,
    tick : f32,
    cycle_frame_skip : f32,
    animation_speed : f32,
    cycle_speed : f32,
    // How many times this gate would have fired had ticks advanced one at a
    // time instead of being batched (see the Rust-side `count_gate_events`
    // in `passes/compute.rs`), so the age increment stays correct in real
    // time even when a call represents more than one elapsed logical tick.
    gate_event_count : f32,
    num_columns : f32,
    num_rows : f32,
    glyph_sequence_length : f32,
    loops : u32,
};

@group(0) @binding(0) var<uniform> p : Params;
@group(0) @binding(1) var previous_symbol_state : texture_2d<f32>;
@group(0) @binding(2) var raindrop_state : texture_2d<f32>;
@group(0) @binding(3) var state_sampler : sampler;

fn compute_result(
    sim_time : f32,
    is_first_frame : bool,
    screen_pos : vec2<f32>,
    previous : vec4<f32>,
    raindrop : vec4<f32>,
) -> vec4<f32> {
    var previous_symbol = previous.r;
    var previous_age = previous.g;
    var reset_glyph = is_first_frame;
    if (p.loops != 0u) {
        reset_glyph = reset_glyph || raindrop.r <= 0.0;
    }
    if (reset_glyph) {
        previous_age = random_float(screen_pos + 0.5);
        previous_symbol = floor(p.glyph_sequence_length * random_float(screen_pos));
    }
    let local_cycle_speed = p.animation_speed * p.cycle_speed;
    var age = previous_age;
    var symbol = previous_symbol;
    // `age` always stays in [0, 1) on entry (via `fract()` below or the
    // reset path above), so adding `gate_event_count` gate-events' worth of
    // increment in one shot and checking the threshold once is exactly
    // equivalent to applying a single gate-event's increment
    // `gate_event_count` times in a row — no loop needed. When
    // `gate_event_count` is 0 (no gate crossed this call), this is a no-op.
    age += local_cycle_speed * p.cycle_frame_skip * p.gate_event_count;
    if (age >= 1.0) {
        symbol = floor(p.glyph_sequence_length * random_float(screen_pos + sim_time));
        age = fract(age);
    }

    return vec4<f32>(symbol, age, 0.0, 0.0);
}

@fragment
fn fs_main(in : FullscreenOut) -> @location(0) vec4<f32> {
    let sim_time = p.time * p.animation_speed;
    let is_first_frame = p.tick <= 1.0;
    let glyph_pos = in.position.xy;
    let screen_pos = glyph_pos / vec2<f32>(p.num_columns, p.num_rows);
    let previous = textureSampleLevel(previous_symbol_state, state_sampler, screen_pos, 0.0);
    let raindrop = textureSampleLevel(raindrop_state, state_sampler, screen_pos, 0.0);
    return compute_result(sim_time, is_first_frame, screen_pos, previous, raindrop);
}
