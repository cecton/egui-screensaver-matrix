// Ports both branches of shaders/glsl/rainPass.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed): MSDF glyph (and,
// when isolateGlint is set, a second "glint" MSDF) sampling plus per-cell
// brightness computed from the raindrop/symbol/effect state. Compiled
// twice (VOLUMETRIC 0/1, see render.vert.glsl's header comment) since the
// volumetric branch reads its raindrop/symbol/effect/depth data from
// per-vertex varyings instead of sampling the state textures directly,
// and fades brightness by distance. Output channels: R = non-cursor base
// brightness, G = cursor brightness, B = glint brightness.

#define PI 3.14159265359
#ifdef GL_OES_standard_derivatives
#extension GL_OES_standard_derivatives : enable
#endif
#ifdef GL_ES
precision lowp float;
#endif

#if NEW_SHADER_INTERFACE
    in vec2 v_uv;
    out vec4 fragColor;
    #define gl_FragColor fragColor
    #define texture2D texture
    #if VOLUMETRIC
        in vec4 vRaindrop, vSymbol, vEffect;
        in float vDepth;
    #endif
#else
    varying vec2 v_uv;
    #if VOLUMETRIC
        varying vec4 vRaindrop, vSymbol, vEffect;
        varying float vDepth;
    #endif
#endif

#if !VOLUMETRIC
uniform sampler2D raindropState, symbolState, effectState;
#endif
uniform float numColumns, numRows;
uniform sampler2D glyphMSDF, glintMSDF;
uniform sampler2D baseTexture, glintTexture;
uniform bool hasBaseTexture, hasGlintTexture;
uniform float msdfPxRange;
uniform vec2 glyphMSDFSize, glintMSDFSize;
uniform float glyphHeightToWidth, glyphSequenceLength, glyphEdgeCrop;
uniform float baseContrast, baseBrightness, glintContrast, glintBrightness;
uniform float brightnessOverride, brightnessThreshold;
uniform vec2 glyphTextureGridSize;
uniform bool isolateCursor, isolateGlint;
uniform mat2 glyphTransform;
uniform vec2 slantVec;
uniform float slantScale;
uniform bool isPolar;

float median3(vec3 i) {
    return max(min(i.r, i.g), min(max(i.r, i.g), i.b));
}

float modI(float a, float b) {
    float m = a - floor((a + 0.5) / b) * b;
    return floor(m + 0.5);
}

vec2 getUV(vec2 uv) {
#if !VOLUMETRIC
    if (isPolar) {
        // Curved space that makes letters appear to radiate from up above.
        uv -= 0.5;
        uv *= 0.5;
        uv.y -= 0.5;
        float radius = length(uv);
        float angle = atan(uv.y, uv.x) / (2. * PI) + 0.5;
        uv = vec2(fract(angle * 4. - 0.5), 1.5 * (1. - sqrt(radius)));
    } else {
        // Applies the slant and scales space so the viewport is fully covered.
        uv = vec2(
            (uv.x - 0.5) * slantVec.x + (uv.y - 0.5) * slantVec.y,
            (uv.y - 0.5) * slantVec.x - (uv.x - 0.5) * slantVec.y
        ) * slantScale + 0.5;
    }
    uv.y /= glyphHeightToWidth;
#endif
    return uv;
}

vec3 getBrightness(vec4 raindrop, vec4 effect, float quadDepth, vec2 uv) {
    float base = raindrop.r + max(0., 1.0 - raindrop.a * 5.0);
    bool isCursor = bool(raindrop.g) && isolateCursor;
    float glint = base;
    float multipliedEffects = effect.r;
    float addedEffects = effect.g;

    vec2 textureUV = fract(uv * vec2(numColumns, numRows));
    base = base * baseContrast + baseBrightness;
    if (hasBaseTexture) {
        base *= texture2D(baseTexture, textureUV).r;
    }
    glint = glint * glintContrast + glintBrightness;
    if (hasGlintTexture) {
        glint *= texture2D(glintTexture, textureUV).r;
    }

    // Modes that don't fade glyphs set their actual brightness here.
    if (brightnessOverride > 0. && base > brightnessThreshold && !isCursor) {
        base = brightnessOverride;
    }

    base = base * multipliedEffects + addedEffects;
    glint = glint * multipliedEffects + addedEffects;

#if VOLUMETRIC
    // In volumetric mode, distant glyphs are dimmer.
    base = base * min(1.0, quadDepth);
    glint = glint * min(1.0, quadDepth);
#endif

    return vec3((isCursor ? vec2(0.0, 1.0) : vec2(1.0, 0.0)) * base, glint) * raindrop.b;
}

vec2 getSymbolUV(float index) {
    float symbolX = modI(index, glyphTextureGridSize.x);
    float symbolY = (index - symbolX) / glyphTextureGridSize.x;
    symbolY = glyphTextureGridSize.y - symbolY - 1.;
    return vec2(symbolX, symbolY);
}

vec2 getSymbol(vec2 uv, float index) {
    // resolve UV to cropped position of glyph in MSDF texture
    uv = fract(uv * vec2(numColumns, numRows));
    uv -= 0.5;
    uv = glyphTransform * uv;
    uv *= clamp(1. - glyphEdgeCrop, 0., 1.);
    uv += 0.5;
    uv = (uv + getSymbolUV(index)) / glyphTextureGridSize;

    // MSDF: calculate brightness of fragment based on distance to shape
    vec2 symbol = vec2(0.0);
    {
        vec2 unitRange = vec2(msdfPxRange) / glyphMSDFSize;
        vec2 screenTexSize = vec2(1.0) / fwidth(uv);
        float screenPxRange = max(0.5 * dot(unitRange, screenTexSize), 1.0);

        float signedDistance = median3(texture2D(glyphMSDF, uv).rgb);
        float screenPxDistance = screenPxRange * (signedDistance - 0.5);
        symbol.r = clamp(screenPxDistance + 0.5, 0.0, 1.0);
    }

    if (isolateGlint) {
        vec2 unitRange = vec2(msdfPxRange) / glintMSDFSize;
        vec2 screenTexSize = vec2(1.0) / fwidth(uv);
        float screenPxRange = max(0.5 * dot(unitRange, screenTexSize), 1.0);

        float signedDistance = median3(texture2D(glintMSDF, uv).rgb);
        float screenPxDistance = screenPxRange * (signedDistance - 0.5);
        symbol.g = clamp(screenPxDistance + 0.5, 0.0, 1.0);
    }

    return symbol;
}

void main() {
    vec2 uv = getUV(v_uv);

#if VOLUMETRIC
    vec4 raindropData = vRaindrop;
    vec4 symbolData = vSymbol;
    vec4 effectData = vEffect;
    float depth = vDepth;
#else
    vec4 raindropData = texture2D(raindropState, uv);
    vec4 symbolData = texture2D(symbolState, uv);
    vec4 effectData = texture2D(effectState, uv);
    float depth = 0.0;
#endif

    vec3 brightness = getBrightness(raindropData, effectData, depth, uv);
    vec2 symbol = getSymbol(uv, symbolData.r);

    gl_FragColor = vec4(brightness.rg * symbol.r, brightness.b * symbol.g, 0.0);
}
