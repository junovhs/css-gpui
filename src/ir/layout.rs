use serde::{Deserialize, Serialize};

/// CSS `position` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    Static,
    Absolute,
    Relative,
    Fixed,
}

/// CSS `display` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    Block,
    Flex,
    /// CSS `inline-flex`.
    InlineFlex,
    Grid,
}

/// CSS `box-sizing` value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxSizing {
    BorderBox,
    ContentBox,
}

/// CSS `flex-direction` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexDirection {
    Row,
    Column,
}

/// CSS `flex-wrap` line wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub(crate) fn from_css_keyword(value: &str) -> Option<Self> {
        [Self::Nowrap, Self::Wrap, Self::WrapReverse]
            .into_iter()
            .find(|variant| value.eq_ignore_ascii_case(variant.css_keyword()))
    }

    pub(crate) fn css_keyword(self) -> &'static str {
        match self {
            Self::Nowrap => "nowrap",
            Self::Wrap => "wrap",
            Self::WrapReverse => "wrap-reverse",
        }
    }
}

/// Horizontal alignment of inline content within its containing block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Start,
    Center,
    End,
    Left,
    Right,
}

impl TextAlign {
    pub(crate) fn from_css_keyword(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("initial") {
            return Some(Self::Start);
        }
        [
            Self::Start,
            Self::Center,
            Self::End,
            Self::Left,
            Self::Right,
        ]
        .into_iter()
        .find(|variant| value.eq_ignore_ascii_case(variant.css_keyword()))
    }

    pub(crate) fn css_keyword(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// CSS `align-items` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignItems {
    Center,
}

/// CSS `justify-content` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JustifyContent {
    Center,
    SpaceBetween,
}

/// CSS `align-self` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignSelf {
    Auto,
    FlexStart,
    Center,
    End,
}

/// CSS `cursor` value supported in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cursor {
    Auto,
    Pointer,
}

/// Resolved pixel lengths for all four sides of a box model edge.
///
/// All values are in CSS pixels with no unit suffix. Default is zero on all sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeInset {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeInset {
    /// Zero inset on all sides.
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    /// Uniform inset on all sides.
    pub fn uniform(px: f32) -> Self {
        Self {
            top: px,
            right: px,
            bottom: px,
            left: px,
        }
    }
}

/// Equal-sized grid tracks and explicit start lines for items.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridLayout {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_start: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_start: Option<i16>,
}

/// Resolved layout properties for an [`super::IrNode`].
///
/// All lengths are in CSS pixels. `None` means the property was not explicitly
/// set; the downstream emitter should apply its own default.
///
/// Flex properties are `None` on non-flex nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub position: Position,
    pub display: Display,
    pub box_sizing: BoxSizing,
    /// CSS `top`, for an absolute or fixed box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<f32>,
    /// CSS `left`, for an absolute or fixed box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// Preferred width-to-height ratio applied on the box named by box-sizing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridLayout>,
    pub margin: EdgeInset,
    pub padding: EdgeInset,
    /// Present only when `display` is [`Display::Flex`] or [`Display::InlineFlex`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flex_direction: Option<FlexDirection>,
    /// Authored line wrapping. An omitted value uses the CSS initial value `nowrap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flex_wrap: Option<FlexWrap>,
    /// Computed CSS text alignment, including values inherited from ancestors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_items: Option<AlignItems>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify_content: Option<JustifyContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_self: Option<AlignSelf>,
    /// CSS `gap` in pixels, present only on flex containers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
}

/// Resolve one auto axis from a ratio, then express both sizes as border-box sizes.
/// GPUI sizing uses the border box, even when the source ratio uses the content box.
pub(crate) fn ratio_border_size(
    sizes: [Option<f32>; 2],
    ratio: f64,
    insets: [f32; 2],
    box_sizing: &BoxSizing,
) -> Option<[f32; 2]> {
    if !ratio.is_finite()
        || ratio <= 0.0
        || !insets
            .iter()
            .all(|inset| inset.is_finite() && *inset >= 0.0)
    {
        return None;
    }
    let [horizontal, vertical] = insets.map(f64::from);
    let border_box = *box_sizing == BoxSizing::BorderBox;
    let [width, height] = match sizes {
        [Some(width), None] if width >= 0.0 => {
            let width = f64::from(width);
            let width = if border_box {
                width.max(horizontal)
            } else {
                width
            };
            [width, width / ratio]
        }
        [None, Some(height)] if height >= 0.0 => {
            let height = f64::from(height);
            let height = if border_box {
                height.max(vertical)
            } else {
                height
            };
            [height * ratio, height]
        }
        _ => return None,
    };
    let outer = |size: f64, inset: f64| {
        (if border_box {
            size.max(inset)
        } else {
            size + inset
        }) as f32
    };
    let (width, height) = (outer(width, horizontal), outer(height, vertical));
    (width.is_finite() && height.is_finite()).then_some([width, height])
}
