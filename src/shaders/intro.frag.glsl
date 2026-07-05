// Ports shaders/glsl/rainPass.intro.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Writes the "intro"
// reveal state to the R channel of a 1-row-tall (1 x numColumns) buffer:
// how far the initial stream of rain has progressed down each column.

#ifdef GL_ES
precision highp float;
#endif

#if NEW_SHADER_INTERFACE
    out vec4 fragColor;
    #define gl_FragColor fragColor
    #define texture2D texture
#endif

#define PI 3.14159265359
#define SQRT_2 1.4142135623730951
#define SQRT_5 2.23606797749979

uniform sampler2D previousIntroState;
uniform float numColumns, numRows;
uniform float time, tick;
uniform float animationSpeed, fallSpeed;

uniform bool skipIntro;

highp float randomFloat(const in vec2 uv) {
    const highp float a = 12.9898, b = 78.233, c = 43758.5453;
    highp float dt = dot(uv.xy, vec2(a, b)), sn = mod(dt, PI);
    return fract(sin(sn) * c);
}

float wobble(float x) {
    return x + 0.3 * sin(SQRT_2 * x) + 0.2 * sin(SQRT_5 * x);
}

vec4 computeResult(float simTime, vec2 glyphPos) {
    if (skipIntro) {
        return vec4(2., 0., 0., 0.);
    }

    float columnTimeOffset;
    int column = int(glyphPos.x);
    if (column == int(numColumns / 2.)) {
        columnTimeOffset = -1.;
    } else if (column == int(numColumns * 0.75)) {
        columnTimeOffset = -2.;
    } else {
        columnTimeOffset = randomFloat(vec2(glyphPos.x, 0.)) * -4.;
        columnTimeOffset += (sin(glyphPos.x / numColumns * PI) - 1.) * 2. - 2.5;
    }
    float introTime = (simTime + columnTimeOffset) * fallSpeed / numRows * 100.;

    return vec4(introTime, 0., 0., 0.);
}

void main() {
    float simTime = time * animationSpeed;
    vec2 glyphPos = gl_FragCoord.xy;
    gl_FragColor = computeResult(simTime, glyphPos);
}
