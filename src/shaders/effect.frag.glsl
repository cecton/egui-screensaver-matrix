// Ports shaders/glsl/rainPass.effect.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Computes the
// non-canon "spice" effects — thunder flashes and ripple distortion —
// that multiply/add to a cell's brightness. Ripple is not wired up yet
// (rippleType is always "none" until a later stage exposes it), but the
// thunder flash is already fully functional.

#ifdef GL_ES
precision highp float;
#endif

#if NEW_SHADER_INTERFACE
    out vec4 fragColor;
    #define gl_FragColor fragColor
    #define texture2D texture
#endif

#define SQRT_2 1.4142135623730951
#define SQRT_5 2.23606797749979

// Note: the original declares/samples a `previousEffectState` uniform here
// too, but never actually uses the result (`computeResult` ignores it) —
// this pass has no frame-to-frame accumulation, so it's omitted here.
uniform float numColumns, numRows;
uniform float time, tick;
uniform float animationSpeed;

uniform bool hasThunder, loops;
uniform float glyphHeightToWidth;
uniform int rippleType;
uniform float rippleScale, rippleSpeed, rippleThickness;

vec2 randomVec2(const in vec2 uv) {
    return fract(vec2(sin(uv.x * 591.32 + uv.y * 154.077), cos(uv.x * 391.32 + uv.y * 49.077)));
}

float wobble(float x) {
    return x + 0.3 * sin(SQRT_2 * x) + 0.2 * sin(SQRT_5 * x);
}

float getThunder(float simTime, vec2 screenPos) {
    if (!hasThunder) {
        return 0.;
    }

    float thunderTime = simTime * 0.5;
    float thunder = 1. - fract(wobble(thunderTime));
    if (loops) {
        thunder = 1. - fract(thunderTime + 0.3);
    }

    thunder = log(thunder * 1.5) * 4.;
    thunder = clamp(thunder, 0., 1.) * 10. * pow(screenPos.y, 2.);
    return thunder;
}

float getRipple(float simTime, vec2 screenPos) {
    if (rippleType == -1) {
        return 0.;
    }

    float rippleTime = (simTime * 0.5 + sin(simTime) * 0.2) * rippleSpeed + 1.;
    if (loops) {
        rippleTime = (simTime * 0.5) * rippleSpeed + 1.;
    }

    vec2 offset = randomVec2(vec2(floor(rippleTime), 0.)) - 0.5;
    if (loops) {
        offset = vec2(0.);
    }
    vec2 ripplePos = screenPos * 2. - 1. + offset;
    float rippleDistance;
    if (rippleType == 0) {
        vec2 boxDistance = abs(ripplePos) * vec2(1., glyphHeightToWidth);
        rippleDistance = max(boxDistance.x, boxDistance.y);
    } else {
        rippleDistance = length(ripplePos);
    }

    float rippleValue = fract(rippleTime) * rippleScale - rippleDistance;

    if (rippleValue > 0. && rippleValue < rippleThickness) {
        return 0.75;
    }

    return 0.;
}

vec4 computeResult(float simTime, vec2 screenPos) {
    float multipliedEffects = 1. + getThunder(simTime, screenPos);
    float addedEffects = getRipple(simTime, screenPos);
    return vec4(multipliedEffects, addedEffects, 0., 0.);
}

void main() {
    float simTime = time * animationSpeed;
    vec2 glyphPos = gl_FragCoord.xy;
    vec2 screenPos = glyphPos / vec2(numColumns, numRows);
    gl_FragColor = computeResult(simTime, screenPos);
}
