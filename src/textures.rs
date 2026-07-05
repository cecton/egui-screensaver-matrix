//! Bundled decorative base/glint textures used by the volumetric presets
//! (`trinity`, `morpheus`, `bugs`). Each is sampled as a grayscale mask
//! (only the red channel is read) multiplying the base/glint brightness.

/// Selects one of the bundled decorative textures. The original's
/// `baseTexture`/`glintTexture` (`"sand"` / `"pixels"` / `"mesh"` /
/// `"metal"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DecorativeTexture {
    Sand,
    Pixels,
    Mesh,
    Metal,
}

impl DecorativeTexture {
    pub fn png_bytes(self) -> &'static [u8] {
        match self {
            DecorativeTexture::Sand => include_bytes!("../assets/textures/sand.png"),
            DecorativeTexture::Pixels => include_bytes!("../assets/textures/pixel_grid.png"),
            DecorativeTexture::Mesh => include_bytes!("../assets/textures/mesh.png"),
            DecorativeTexture::Metal => include_bytes!("../assets/textures/metal.png"),
        }
    }
}
