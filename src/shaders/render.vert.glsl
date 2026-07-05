// Ports both branches of shaders/glsl/rainPass.vert.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Compiled twice, once
// with `VOLUMETRIC` defined as 0 and once as 1 (same header-define trick
// as `NEW_SHADER_INTERFACE`), producing two separate programs selected in
// Rust by `config.volumetric` — the geometry and per-vertex outputs
// differ structurally enough (fullscreen triangle + no varyings vs.
// per-quad grid + raindrop/symbol/effect/depth varyings) that branching
// on a uniform inside one program isn't a better fit here.
//
// Non-volumetric: a plain fullscreen triangle, `screenSize`-stretched to
// map the always-square compute grid onto the (possibly non-square)
// viewport (see `passes/compute.rs`).
//
// Volumetric: one quad per glyph cell (`aPosition` = cell coordinate,
// `aCorner` = which corner of the quad), each given a per-column receding
// depth and projected through `viewProjection` (a fixed perspective
// camera times a `translate(0,0,-1)`, precomputed on the Rust side — see
// `camera.rs` — since neither depends on per-frame state, only aspect
// ratio). The compute-state textures are sampled once per vertex here
// (not per-fragment), matching the original.

#if NEW_SHADER_INTERFACE
    #if VOLUMETRIC
        in vec2 aPosition, aCorner;
        out vec4 vRaindrop, vSymbol, vEffect;
        out float vDepth;
        #define texture2D texture
    #else
        in vec2 a_pos;
    #endif
    out vec2 v_uv;
#else
    #if VOLUMETRIC
        attribute vec2 aPosition, aCorner;
        varying vec4 vRaindrop, vSymbol, vEffect;
        varying float vDepth;
    #else
        attribute vec2 a_pos;
    #endif
    varying vec2 v_uv;
#endif

uniform float glyphHeightToWidth;

#if VOLUMETRIC
uniform sampler2D raindropState, symbolState, effectState;
uniform float density;
uniform vec2 quadSize;
uniform float glyphVerticalSpacing;
uniform mat4 viewProjection;
uniform float time, animationSpeed, forwardSpeed;

#define PI 3.14159265359
highp float rand(const in vec2 uv) {
    const highp float a = 12.9898, b = 78.233, c = 43758.5453;
    highp float dt = dot(uv.xy, vec2(a, b)), sn = mod(dt, PI);
    return fract(sin(sn) * c);
}
#else
uniform vec2 screenSize;
#endif

void main() {
#if VOLUMETRIC
    v_uv = (aPosition + aCorner) * quadSize;
    vRaindrop = texture2D(raindropState, aPosition * quadSize);
    vSymbol = texture2D(symbolState, aPosition * quadSize);
    vEffect = texture2D(effectState, aPosition * quadSize);

    float startDepth = rand(vec2(aPosition.x, 0.));
    float quadDepth = fract(startDepth + time * animationSpeed * forwardSpeed);
    vDepth = quadDepth;

    vec2 position = (aPosition * vec2(1., glyphVerticalSpacing) + aCorner * vec2(density, 1.)) * quadSize;
    position.y += rand(vec2(aPosition.x, 1.)) * quadSize.y;

    vec4 pos = vec4((position - 0.5) * 2.0, quadDepth, 1.0);
    pos.x /= glyphHeightToWidth;
    pos = viewProjection * pos;
    gl_Position = pos;
#else
    v_uv = a_pos * 0.5 + 0.5;
    gl_Position = vec4(a_pos * screenSize, 0.0, 1.0);
#endif
}
