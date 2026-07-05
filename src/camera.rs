//! A minimal column-major 4x4 matrix, just enough for the volumetric
//! camera: a perspective projection combined with a fixed translation
//! (the original's `camera * transform`, precomputed here into one
//! matrix since neither depends on per-frame state — only screen aspect
//! ratio, which changes on resize). Ported from the non-isometric,
//! non-Looking-Glass branch of `js/regl/rainPass.js`'s camera/transform
//! setup at https://github.com/Rezmason/matrix (MIT licensed). Hand-rolled
//! rather than pulling in a math crate: the original's volumetric camera
//! isn't free-flying, so the only operations needed are "build a
//! perspective matrix," "build a translation matrix," and "multiply two
//! 4x4 matrices" — about as much as `glam` would save isn't worth a new
//! dependency for.

/// Column-major, matching GLSL's `mat4` layout.
pub type Mat4 = [f32; 16];

fn mat4_identity() -> Mat4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_translation(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = mat4_identity();
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

/// Standard right-handed perspective projection, matching `gl-matrix`'s
/// `mat4.perspective` (used by the original via `mat4.perspective(camera,
/// (Math.PI/180)*90, aspectRatio, 0.0001, 1000)`).
fn mat4_perspective(fovy_radians: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy_radians / 2.0).tan();
    let range_inv = 1.0 / (near - far);
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (near + far) * range_inv;
    m[11] = -1.0;
    m[14] = 2.0 * near * far * range_inv;
    m
}

fn mat4_multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

/// The volumetric-mode view-projection matrix: a 90°-vertical-FOV
/// perspective projection (matching the original's hardcoded FOV) times a
/// fixed `translate(0, 0, -1)` (placing the rain's local z=0..1 depth
/// range in front of the camera). Recompute this whenever the aspect
/// ratio changes (i.e. on resize) — it doesn't depend on any other
/// per-frame state.
pub fn volumetric_view_projection(aspect_ratio: f32) -> Mat4 {
    let camera = mat4_perspective(90f32.to_radians(), aspect_ratio, 0.0001, 1000.0);
    let transform = mat4_translation(0.0, 0.0, -1.0);
    mat4_multiply(&camera, &transform)
}
