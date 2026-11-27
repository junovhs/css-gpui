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

/// Paint refinements, relative to the element's resting paint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatePaint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<Color>,
}

impl StatePaint {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The three active combinations of local pointer hover and keyboard focus.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionPaint {
    /// Resting foreground when the scene has no complete typography record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resting_text_color: Option<Color>,
    #[serde(default, skip_serializing_if = "StatePaint::is_empty")]
    pub hover: StatePaint,
    #[serde(default, skip_serializing_if = "StatePaint::is_empty")]
    pub focus: StatePaint,
    #[serde(default, skip_serializing_if = "StatePaint::is_empty")]
    pub hover_focus: StatePaint,
}

impl InteractionPaint {
    pub fn uses_focus(&self) -> bool {
        !self.focus.is_empty() || self.hover_focus != self.hover
    }

    /// Reinstate the resting color when both states match and the cascade cancels
    /// a change from a single state.
    pub(crate) fn combined(&self, resting: &Paint, resting_text: Option<&Color>) -> StatePaint {
        let select = |combined: &Option<Color>,
                      hover: &Option<Color>,
                      focus: &Option<Color>,
                      resting: Option<&Color>,
                      initial: &str| {
            combined.clone().or_else(|| {
                (hover.is_some() || focus.is_some()).then(|| {
                    resting
                        .cloned()
                        .unwrap_or_else(|| Color(initial.to_owned()))
                })
            })
        };
        StatePaint {
            background_color: select(
                &self.hover_focus.background_color,
                &self.hover.background_color,
                &self.focus.background_color,
                resting.background_color.as_ref(),
                "#00000000",
            ),
            border_color: select(
                &self.hover_focus.border_color,
                &self.hover.border_color,
                &self.focus.border_color,
                resting.border.as_ref().map(|border| &border.color),
                "#000000",
            ),
            text_color: select(
                &self.hover_focus.text_color,
                &self.hover.text_color,
                &self.focus.text_color,
                self.resting_text_color.as_ref().or(resting_text),
                "#000000",
            ),
        }
    }
}

/// Resolved paint properties for an [`super::IrNode`].
///
/// The resting text color lives in [`super::Typography`]. Interaction paint
/// holds foreground as well as surface changes.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<InteractionPaint>,
}
