// Ports shaders/glsl/rainPass.symbol.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Governs which glyph
// appears in each cell and how long it's been showing that glyph, so
// glyphs cycle to a new random symbol periodically rather than every
// frame.

#ifdef GL_ES
precision highp float;
#endif

#if NEW_SHADER_INTERFACE
    out vec4 fragColor;
    #define gl_FragColor fragColor
    #define texture2D texture
#endif

#define PI 3.14159265359

uniform sampler2D previousSymbolState, raindropState;
uniform float numColumns, numRows;
uniform float time, tick, cycleFrameSkip;
uniform float animationSpeed, cycleSpeed;
// How many times this gate would have fired had ticks advanced one at a
// time instead of being batched (see the Rust-side `count_gate_events` in
// `passes/compute.rs`), so the age increment stays correct in real time
// even when a call represents more than one elapsed logical tick.
uniform float gateEventCount;
uniform bool loops;
uniform float glyphSequenceLength;

highp float randomFloat(const in vec2 uv) {
    const highp float a = 12.9898, b = 78.233, c = 43758.5453;
    highp float dt = dot(uv.xy, vec2(a, b)), sn = mod(dt, PI);
    return fract(sin(sn) * c);
}

vec4 computeResult(float simTime, bool isFirstFrame, vec2 screenPos, vec4 previous, vec4 raindrop) {
    float previousSymbol = previous.r;
    float previousAge = previous.g;
    bool resetGlyph = isFirstFrame;
    if (loops) {
        resetGlyph = resetGlyph || raindrop.r <= 0.;
    }
    if (resetGlyph) {
        previousAge = randomFloat(screenPos + 0.5);
        previousSymbol = floor(glyphSequenceLength * randomFloat(screenPos));
    }
    float localCycleSpeed = animationSpeed * cycleSpeed;
    float age = previousAge;
    float symbol = previousSymbol;
    // `age` always stays in [0, 1) on entry (via `fract()` below or the
    // reset path above), so adding `gateEventCount` gate-events' worth of
    // increment in one shot and checking the threshold once is exactly
    // equivalent to applying a single gate-event's increment
    // `gateEventCount` times in a row — no loop needed. When
    // `gateEventCount` is 0 (no gate crossed this call), this is a no-op.
    age += localCycleSpeed * cycleFrameSkip * gateEventCount;
    if (age >= 1.) {
        symbol = floor(glyphSequenceLength * randomFloat(screenPos + simTime));
        age = fract(age);
    }

    return vec4(symbol, age, 0., 0.);
}

void main() {
    float simTime = time * animationSpeed;
    bool isFirstFrame = tick <= 1.;
    vec2 glyphPos = gl_FragCoord.xy;
    vec2 screenPos = glyphPos / vec2(numColumns, numRows);
    vec4 previous = texture2D(previousSymbolState, screenPos);
    vec4 raindrop = texture2D(raindropState, screenPos);
    gl_FragColor = computeResult(simTime, isFirstFrame, screenPos, previous, raindrop);
}
