use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::layout::Cursor;

/// A normalized hex color string, e.g. `"#1a2b3c"` or `"#ffffff"`.
///
/// Values are stored in lowercase with a leading `#`. Callers are responsible
/// for normalizing before comparison; the struct itself does no canonicalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(pub String);

/// Uniform border applied to all four sides of a box (v0.1 does not track
/// per-side borders separately).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Border {
    /// Border width in CSS pixels.
    pub width: f32,
    pub color: Color,
}

/// Source-resolved placement and paint of a generated pseudo-element.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Decoration {
    pub pseudo: String,
    pub placement: BTreeMap<String, String>,
    pub effects: BTreeMap<String, String>,
    pub border_width: f32,
    pub radius: f32,
}

/// Resolved paint properties for an [`super::IrNode`].
///
/// Foreground text color is on [`super::Typography`], not here. `Paint` covers
/// the box-level visual surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<Color>,
    /// Uniform border. `None` means no visible border.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<Border>,
    /// CSS `border-radius` in pixels (uniform corner radius).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
    /// Cascaded paint effects with custom properties already substituted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub effects: BTreeMap<String, String>,
    /// Positioning of a composited decorative layer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub placement: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorations: Vec<Decoration>,
}
