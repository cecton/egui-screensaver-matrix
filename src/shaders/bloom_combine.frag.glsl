// Ports shaders/glsl/bloomPass.combine.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Flattens the 5-level
// blur pyramid (each level a different resolution, so sampling with
// linear filtering upsamples it) into one bloom texture.

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

uniform sampler2D pyr_0;
uniform sampler2D pyr_1;
uniform sampler2D pyr_2;
uniform sampler2D pyr_3;
uniform sampler2D pyr_4;
uniform float bloomStrength;

void main() {
    vec4 total = vec4(0.);
    total += texture2D(pyr_0, v_uv) * 0.96549;
    total += texture2D(pyr_1, v_uv) * 0.92832;
    total += texture2D(pyr_2, v_uv) * 0.88790;
    total += texture2D(pyr_3, v_uv) * 0.84343;
    total += texture2D(pyr_4, v_uv) * 0.79370;
    gl_FragColor = total * bloomStrength;
}
