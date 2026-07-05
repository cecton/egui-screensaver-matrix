// Ports shaders/glsl/bloomPass.highPass.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). Zeroes out any
// channel dimmer than `highPassThreshold`, so only bright areas
// contribute to the blur that follows.

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

uniform sampler2D tex;
uniform float highPassThreshold;

void main() {
    vec4 color = texture2D(tex, v_uv);
    if (color.r < highPassThreshold) color.r = 0.0;
    if (color.g < highPassThreshold) color.g = 0.0;
    if (color.b < highPassThreshold) color.b = 0.0;
    gl_FragColor = color;
}
