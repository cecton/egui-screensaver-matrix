//! Bundled MSDF glyph atlas metadata for all 8 fonts used by the 9
//! non-volumetric presets (the `gtarg_*` fonts from the original aren't
//! used by any named preset, so they're not bundled).

/// A multi-channel signed distance field glyph atlas: a grid of
/// `grid_cols * grid_rows` glyph cells, of which the first
/// `glyph_sequence_length` are valid symbols (the rest are unused padding
/// in the grid).
pub struct FontAtlas {
    /// Not read yet — used starting Stage 5, when doneward's settings UI
    /// needs to label a font selector.
    #[allow(dead_code)]
    pub name: &'static str,
    pub png_bytes: &'static [u8],
    /// A second MSDF atlas, at the same grid layout, for the "glint"
    /// highlight layer — only `resurrections` (used by the volumetric
    /// `trinity`/`morpheus`/`bugs` presets) has one.
    pub glint_png_bytes: Option<&'static [u8]>,
    pub glyph_sequence_length: u32,
    pub grid_cols: u32,
    pub grid_rows: u32,
}

/// Selects one of the bundled fonts. Each named [`crate::config::Preset`]
/// picks one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FontId {
    #[default]
    Matrixcode,
    Megacity,
    Neomatrixology,
    Gothic,
    Coptic,
    HuberfishA,
    HuberfishD,
    Resurrections,
}

impl FontId {
    pub fn atlas(self) -> &'static FontAtlas {
        match self {
            FontId::Matrixcode => &MATRIXCODE,
            FontId::Megacity => &MEGACITY,
            FontId::Neomatrixology => &NEOMATRIXOLOGY,
            FontId::Gothic => &GOTHIC,
            FontId::Coptic => &COPTIC,
            FontId::HuberfishA => &HUBERFISH_A,
            FontId::HuberfishD => &HUBERFISH_D,
            FontId::Resurrections => &RESURRECTIONS,
        }
    }
}

pub const MATRIXCODE: FontAtlas = FontAtlas {
    name: "matrixcode",
    png_bytes: include_bytes!("../assets/fonts/matrixcode_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 57,
    grid_cols: 8,
    grid_rows: 8,
};

pub const MEGACITY: FontAtlas = FontAtlas {
    name: "megacity",
    png_bytes: include_bytes!("../assets/fonts/megacity_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 64,
    grid_cols: 8,
    grid_rows: 8,
};

pub const NEOMATRIXOLOGY: FontAtlas = FontAtlas {
    name: "neomatrixology",
    png_bytes: include_bytes!("../assets/fonts/neomatrixology_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 12,
    grid_cols: 4,
    grid_rows: 4,
};

pub const GOTHIC: FontAtlas = FontAtlas {
    name: "gothic",
    png_bytes: include_bytes!("../assets/fonts/gothic_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 27,
    grid_cols: 8,
    grid_rows: 8,
};

pub const COPTIC: FontAtlas = FontAtlas {
    name: "coptic",
    png_bytes: include_bytes!("../assets/fonts/coptic_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 32,
    grid_cols: 8,
    grid_rows: 8,
};

pub const HUBERFISH_A: FontAtlas = FontAtlas {
    name: "huberfishA",
    png_bytes: include_bytes!("../assets/fonts/huberfish_a_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 34,
    grid_cols: 6,
    grid_rows: 6,
};

pub const HUBERFISH_D: FontAtlas = FontAtlas {
    name: "huberfishD",
    png_bytes: include_bytes!("../assets/fonts/huberfish_d_msdf.png"),
    glint_png_bytes: None,
    glyph_sequence_length: 34,
    grid_cols: 6,
    grid_rows: 6,
};

pub const RESURRECTIONS: FontAtlas = FontAtlas {
    name: "resurrections",
    png_bytes: include_bytes!("../assets/fonts/resurrections_msdf.png"),
    glint_png_bytes: Some(include_bytes!(
        "../assets/fonts/resurrections_glint_msdf.png"
    )),
    glyph_sequence_length: 135,
    grid_cols: 13,
    grid_rows: 12,
};
