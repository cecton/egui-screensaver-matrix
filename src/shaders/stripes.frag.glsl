// Ports shaders/glsl/stripePass.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Alternative to
// palette.frag.glsl: instead of mapping brightness through a gradient,
// multiplies it by a custom repeating/sweeping color sequence sampled
// directly at screen UV — used for the pride/trans/custom-stripes
// effects (see `passes/effect.rs`).

#ifdef GL_ES
precision mediump float;
#endif

#if NEW_SHADER_INTERFACE
    in vec2 v_uv;
    out vec4 fragColor;
    #define gl_FragColor fragColor
    #define texture2D texture
#else
    varying vec2 v_uv;
#endif

#define PI 3.14159265359

uniform sampler2D tex;
uniform sampler2D bloomTex;
uniform sampler2D stripeTex;
uniform float ditherMagnitude;
uniform float time;
uniform vec3 backgroundColor, cursorColor, glintColor;
uniform float cursorIntensity, glintIntensity;

highp float rand(const in vec2 uv, const in float t) {
    const highp float a = 12.9898, b = 78.233, c = 43758.5453;
    highp float dt = dot(uv.xy, vec2(a, b)), sn = mod(dt, PI);
    return fract(sin(sn) * c + t);
}

vec4 getBrightness(vec2 uv) {
    vec4 primary = texture2D(tex, uv);
    vec4 bloom = texture2D(bloomTex, uv);
    return primary + bloom;
}

void main() {
    vec3 color = texture2D(stripeTex, v_uv).rgb;

    vec4 brightness = getBrightness(v_uv);

    // Dither: subtract a random value from the brightness.
    brightness -= rand(gl_FragCoord.xy, time) * ditherMagnitude / 3.0;

    gl_FragColor = vec4(
        color * brightness.r
            + min(cursorColor * cursorIntensity * brightness.g, vec3(1.0))
            + min(glintColor * glintIntensity * brightness.b, vec3(1.0))
            + backgroundColor,
        1.0
    );
}
