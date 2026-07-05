// Ports shaders/glsl/bloomPass.blur.frag.glsl from
// https://github.com/Rezmason/matrix (MIT licensed). A cheap 3-tap
// separable blur; drawn once with `direction = (1,0)` (horizontal) and
// once with `direction = (0,1)` (vertical) to approximate a 2D gaussian.
// `width`/`height` are the current render target's resolution (not the
// screen's), used to keep the blur radius aspect-correct.

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

uniform float width, height;
uniform sampler2D tex;
uniform vec2 direction;

void main() {
    vec2 size = width > height ? vec2(width / height, 1.) : vec2(1., height / width);
    gl_FragColor =
        texture2D(tex, v_uv) * 0.442 +
        (
            texture2D(tex, v_uv + direction / max(width, height) * size) +
            texture2D(tex, v_uv - direction / max(width, height) * size)
        ) * 0.279;
}
