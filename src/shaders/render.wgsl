// Ports shaders/glsl/rainPass.vert.glsl and rainPass.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed): MSDF glyph (and, when
// isolateGlint is set, a second "glint" MSDF) sampling plus per-cell
// brightness computed from the raindrop/symbol/effect state. Output
// channels: R = non-cursor base brightness, G = cursor brightness, B =
// glint brightness.
//
// Two variants share these helpers, selected in Rust by `config.volumetric`:
//
// * 2D (`vs_2d`/`fs_2d`): a plain fullscreen triangle, `screen_size`-
//   stretched to map the always-square compute grid onto the (possibly
//   non-square) viewport (see `passes/compute.rs`); the state textures are
//   sampled per fragment.
// * Volumetric (`vs_volumetric`/`fs_volumetric`): one quad per glyph cell
//   (`a_position` = cell coordinate, `a_corner` = which corner of the
//   quad), each given a per-column receding depth and projected through
//   `view_projection`. The state textures are sampled once per vertex (not
//   per fragment), matching the original, and brightness fades by distance.

struct Params {
    num_columns : f32,
    num_rows : f32,
    msdf_px_range : f32,
    glyph_height_to_width : f32,
    glyph_sequence_length : f32,
    glyph_edge_crop : f32,
    base_contrast : f32,
    base_brightness : f32,
    glint_contrast : f32,
    glint_brightness : f32,
    brightness_override : f32,
    brightness_threshold : f32,
    glyph_msdf_size : vec2<f32>,
    glint_msdf_size : vec2<f32>,
    glyph_texture_grid_size : vec2<f32>,
    has_base_texture : u32,
    has_glint_texture : u32,
    isolate_cursor : u32,
    isolate_glint : u32,
    glyph_transform : mat2x2<f32>,
    slant_vec : vec2<f32>,
    slant_scale : f32,
    is_polar : u32,
    screen_size : vec2<f32>,
    density : f32,
    glyph_vertical_spacing : f32,
    quad_size : vec2<f32>,
    time : f32,
    animation_speed : f32,
    forward_speed : f32,
    view_projection : mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> p : Params;
@group(0) @binding(1) var raindrop_state : texture_2d<f32>;
@group(0) @binding(2) var symbol_state : texture_2d<f32>;
@group(0) @binding(3) var effect_state : texture_2d<f32>;
@group(0) @binding(4) var glyph_msdf : texture_2d<f32>;
@group(0) @binding(5) var glint_msdf : texture_2d<f32>;
@group(0) @binding(6) var base_texture : texture_2d<f32>;
@group(0) @binding(7) var glint_texture : texture_2d<f32>;
@group(0) @binding(8) var nearest_sampler : sampler;
@group(0) @binding(9) var linear_sampler : sampler;

fn median3(i : vec3<f32>) -> f32 {
    return max(min(i.r, i.g), min(max(i.r, i.g), i.b));
}

fn mod_i(a : f32, b : f32) -> f32 {
    let m = a - floor((a + 0.5) / b) * b;
    return floor(m + 0.5);
}

// Applies the slant and scales space so the viewport is fully covered, or
// the polar warp that makes letters appear to radiate from up above.
fn get_uv_2d(uv_in : vec2<f32>) -> vec2<f32> {
    var uv = uv_in;
    if (p.is_polar != 0u) {
        uv -= 0.5;
        uv *= 0.5;
        uv.y -= 0.5;
        let radius = length(uv);
        let angle = atan2(uv.y, uv.x) / (2.0 * PI) + 0.5;
        uv = vec2<f32>(fract(angle * 4.0 - 0.5), 1.5 * (1.0 - sqrt(radius)));
    } else {
        uv = vec2<f32>(
            (uv.x - 0.5) * p.slant_vec.x + (uv.y - 0.5) * p.slant_vec.y,
            (uv.y - 0.5) * p.slant_vec.x - (uv.x - 0.5) * p.slant_vec.y
        ) * p.slant_scale + 0.5;
    }
    uv.y /= p.glyph_height_to_width;
    return uv;
}

fn get_brightness(raindrop : vec4<f32>, effect : vec4<f32>, quad_depth : f32, uv : vec2<f32>, volumetric : bool) -> vec3<f32> {
    var base = raindrop.r + max(0.0, 1.0 - raindrop.a * 5.0);
    let is_cursor = raindrop.g != 0.0 && p.isolate_cursor != 0u;
    var glint = base;
    let multiplied_effects = effect.r;
    let added_effects = effect.g;

    let texture_uv = fract(uv * vec2<f32>(p.num_columns, p.num_rows));
    base = base * p.base_contrast + p.base_brightness;
    if (p.has_base_texture != 0u) {
        base *= textureSampleLevel(base_texture, linear_sampler, texture_uv, 0.0).r;
    }
    glint = glint * p.glint_contrast + p.glint_brightness;
    if (p.has_glint_texture != 0u) {
        glint *= textureSampleLevel(glint_texture, linear_sampler, texture_uv, 0.0).r;
    }

    // Modes that don't fade glyphs set their actual brightness here.
    if (p.brightness_override > 0.0 && base > p.brightness_threshold && !is_cursor) {
        base = p.brightness_override;
    }

    base = base * multiplied_effects + added_effects;
    glint = glint * multiplied_effects + added_effects;

    if (volumetric) {
        // In volumetric mode, distant glyphs are dimmer.
        base = base * min(1.0, quad_depth);
        glint = glint * min(1.0, quad_depth);
    }

    var channels = vec2<f32>(1.0, 0.0);
    if (is_cursor) {
        channels = vec2<f32>(0.0, 1.0);
    }
    return vec3<f32>(channels * base, glint) * raindrop.b;
}

fn get_symbol_uv(index : f32) -> vec2<f32> {
    let symbol_x = mod_i(index, p.glyph_texture_grid_size.x);
    var symbol_y = (index - symbol_x) / p.glyph_texture_grid_size.x;
    symbol_y = p.glyph_texture_grid_size.y - symbol_y - 1.0;
    return vec2<f32>(symbol_x, symbol_y);
}

fn msdf_coverage(tex : texture_2d<f32>, size : vec2<f32>, uv : vec2<f32>, uv_width : vec2<f32>) -> f32 {
    let unit_range = vec2<f32>(p.msdf_px_range) / size;
    let screen_tex_size = vec2<f32>(1.0) / uv_width;
    let screen_px_range = max(0.5 * dot(unit_range, screen_tex_size), 1.0);

    let signed_distance = median3(textureSampleLevel(tex, linear_sampler, uv, 0.0).rgb);
    let screen_px_distance = screen_px_range * (signed_distance - 0.5);
    return clamp(screen_px_distance + 0.5, 0.0, 1.0);
}

// `fwidth` must run in uniform control flow, so the derivative is taken
// here and handed down.
fn get_symbol(uv_width : vec2<f32>, uv : vec2<f32>) -> vec2<f32> {
    var symbol = vec2<f32>(0.0);
    symbol.x = msdf_coverage(glyph_msdf, p.glyph_msdf_size, uv, uv_width);
    if (p.isolate_glint != 0u) {
        symbol.y = msdf_coverage(glint_msdf, p.glint_msdf_size, uv, uv_width);
    }
    return symbol;
}

// Resolves a cell-relative UV to the cropped position of the glyph in the
// MSDF atlas.
fn glyph_atlas_uv(uv_in : vec2<f32>, index : f32) -> vec2<f32> {
    var uv = fract(uv_in * vec2<f32>(p.num_columns, p.num_rows));
    uv -= 0.5;
    uv = p.glyph_transform * uv;
    uv *= clamp(1.0 - p.glyph_edge_crop, 0.0, 1.0);
    uv += 0.5;
    return (uv + get_symbol_uv(index)) / p.glyph_texture_grid_size;
}

struct Vertex2dOut {
    @builtin(position) position : vec4<f32>,
    @location(0) uv : vec2<f32>,
};

@vertex
fn vs_2d(@builtin(vertex_index) index : u32) -> Vertex2dOut {
    let pos = fullscreen_corner(index);
    var out : Vertex2dOut;
    out.uv = pos * 0.5 + 0.5;
    let stretched = pos * p.screen_size;
    // Offscreen target: negate clip-space y (see `gpu.rs`).
    out.position = vec4<f32>(stretched.x, -stretched.y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_2d(in : Vertex2dOut) -> @location(0) vec4<f32> {
    let uv = get_uv_2d(in.uv);

    let raindrop_data = textureSampleLevel(raindrop_state, nearest_sampler, uv, 0.0);
    let symbol_data = textureSampleLevel(symbol_state, nearest_sampler, uv, 0.0);
    let effect_data = textureSampleLevel(effect_state, nearest_sampler, uv, 0.0);

    let brightness = get_brightness(raindrop_data, effect_data, 0.0, uv, false);
    let atlas_uv = glyph_atlas_uv(uv, symbol_data.r);
    let symbol = get_symbol(fwidth(atlas_uv), atlas_uv);

    return vec4<f32>(brightness.rg * symbol.x, brightness.b * symbol.y, 0.0);
}

struct VolumetricOut {
    @builtin(position) position : vec4<f32>,
    @location(0) uv : vec2<f32>,
    @location(1) raindrop : vec4<f32>,
    @location(2) symbol : vec4<f32>,
    @location(3) effect : vec4<f32>,
    @location(4) depth : f32,
};

@vertex
fn vs_volumetric(
    @location(0) a_position : vec2<f32>,
    @location(1) a_corner : vec2<f32>,
) -> VolumetricOut {
    var out : VolumetricOut;
    out.uv = (a_position + a_corner) * p.quad_size;
    let cell_uv = a_position * p.quad_size;
    out.raindrop = textureSampleLevel(raindrop_state, nearest_sampler, cell_uv, 0.0);
    out.symbol = textureSampleLevel(symbol_state, nearest_sampler, cell_uv, 0.0);
    out.effect = textureSampleLevel(effect_state, nearest_sampler, cell_uv, 0.0);

    let start_depth = random_float(vec2<f32>(a_position.x, 0.0));
    let quad_depth = fract(start_depth + p.time * p.animation_speed * p.forward_speed);
    out.depth = quad_depth;

    var position = (a_position * vec2<f32>(1.0, p.glyph_vertical_spacing) + a_corner * vec2<f32>(p.density, 1.0)) * p.quad_size;
    position.y += random_float(vec2<f32>(a_position.x, 1.0)) * p.quad_size.y;

    var pos = vec4<f32>((position - 0.5) * 2.0, quad_depth, 1.0);
    pos.x /= p.glyph_height_to_width;
    pos = p.view_projection * pos;
    // Offscreen target: negate clip-space y (see `gpu.rs`).
    pos.y = -pos.y;
    out.position = pos;
    return out;
}

@fragment
fn fs_volumetric(in : VolumetricOut) -> @location(0) vec4<f32> {
    // The slant/polar warp and aspect correction of `get_uv_2d` only apply
    // to the 2D path; volumetric quads already carry their own geometry.
    let uv = in.uv;

    let brightness = get_brightness(in.raindrop, in.effect, in.depth, uv, true);
    let atlas_uv = glyph_atlas_uv(uv, in.symbol.r);
    let symbol = get_symbol(fwidth(atlas_uv), atlas_uv);

    return vec4<f32>(brightness.rg * symbol.x, brightness.b * symbol.y, 0.0);
}
