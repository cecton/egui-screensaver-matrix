// Ports shaders/glsl/rainPass.raindrop.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). The core of the rain
// simulation: writes each cell's falling brightness, whether it's the
// "cursor" (brightest glyph at the bottom of a raindrop), and whether it's
// been activated yet by the intro reveal.

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

uniform sampler2D previousRaindropState, introState;
uniform float numColumns, numRows;
uniform float time, tick;
uniform float animationSpeed, fallSpeed;

uniform bool loops, skipIntro;
uniform float brightnessDecay;
uniform float raindropLength;

highp float randomFloat(const in vec2 uv) {
    const highp float a = 12.9898, b = 78.233, c = 43758.5453;
    highp float dt = dot(uv.xy, vec2(a, b)), sn = mod(dt, PI);
    return fract(sin(sn) * c);
}

float wobble(float x) {
    return x + 0.3 * sin(SQRT_2 * x) + 0.2 * sin(SQRT_5 * x);
}

// The rain's key underlying concept: glyphs that share a column light
// simultaneously and are brighter toward the bottom; those bright areas
// are truncated into raindrops.
float getRainBrightness(float simTime, vec2 glyphPos) {
    float columnTimeOffset = randomFloat(vec2(glyphPos.x, 0.)) * 1000.;
    float columnSpeedOffset = randomFloat(vec2(glyphPos.x + 0.1, 0.)) * 0.5 + 0.5;
    if (loops) {
        columnSpeedOffset = 0.5;
    }
    float columnTime = columnTimeOffset + simTime * fallSpeed * columnSpeedOffset;
    float rainTime = (glyphPos.y * 0.01 + columnTime) / raindropLength;
    if (!loops) {
        rainTime = wobble(rainTime);
    }
    return 1.0 - fract(rainTime);
}

vec4 computeResult(float simTime, bool isFirstFrame, vec2 glyphPos, vec4 previous, vec4 intro) {
    float brightness = getRainBrightness(simTime, glyphPos);
    float brightnessBelow = getRainBrightness(simTime, glyphPos + vec2(0., -1.));

    float introProgress = intro.r - (1. - glyphPos.y / numRows);
    float introProgressBelow = intro.r - (1. - (glyphPos.y - 1.) / numRows);

    bool activated = bool(previous.b) || skipIntro || introProgress > 0.;
    bool activatedBelow = skipIntro || introProgressBelow > 0.;

    bool cursor = brightness > brightnessBelow || (activated && !activatedBelow);

    // Blend with the previous frame's brightness so it winks on and off
    // organically instead of snapping instantly.
    if (!isFirstFrame) {
        float previousBrightness = previous.r;
        brightness = mix(previousBrightness, brightness, brightnessDecay);
    }

    return vec4(brightness, cursor, activated, introProgress);
}

void main() {
    float simTime = time * animationSpeed;
    bool isFirstFrame = tick <= 1.;
    vec2 glyphPos = gl_FragCoord.xy;
    vec2 screenPos = glyphPos / vec2(numColumns, numRows);
    vec4 previous = texture2D(previousRaindropState, screenPos);
    vec4 intro = texture2D(introState, vec2(screenPos.x, 0.));
    gl_FragColor = computeResult(simTime, isFirstFrame, glyphPos, previous, intro);
}
