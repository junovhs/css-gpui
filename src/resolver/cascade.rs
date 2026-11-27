use crate::ir::FlexWrap;

use super::model::ComputedStyle;
use super::values::{LengthContext, value_parts};

/// Apply a single CSS declaration to `style`, expanding shorthands in place.
///
/// Unknown or unsupported properties are silently ignored; diagnostics are
/// the responsibility of `DIA-01`. The `inherit` keyword is skipped here
/// and handled by the inheritance pass in `resolve_scene`.
#[cfg(test)]
pub fn apply_declaration(style: &mut ComputedStyle, property: &str, value: &str) {
    apply_with_lengths(style, property, value, LengthContext::default());
}

pub(super) fn apply_with_lengths(
    style: &mut ComputedStyle,
    property: &str,
    value: &str,
    lengths: LengthContext,
) {
    let value = value.trim();
    if value == "inherit" {
        return;
    }

    if matches!(
        property,
        "left"
            | "top"
            | "right"
            | "bottom"
            | "width"
            | "height"
            | "aspect-ratio"
            | "z-index"
            | "border-top-right-radius"
            | "border-bottom-left-radius"
    ) {
        let positioned = lengths
            .pixels(value)
            .map_or_else(|| value.to_owned(), |value| format!("{value}px"));
        style
            .paint_placement
            .insert(property.to_owned(), positioned);
    } else if property == "inset" {
        record_inset(&mut style.paint_placement, value, lengths);
    }

    match property {
        // --- Shorthands ---
        "margin" => expand_edges(
            &mut style.margin_top,
            &mut style.margin_right,
            &mut style.margin_bottom,
            &mut style.margin_left,
            value,
            lengths,
        ),
        "padding" => expand_edges(
            &mut style.padding_top,
            &mut style.padding_right,
            &mut style.padding_bottom,
            &mut style.padding_left,
            value,
            lengths,
        ),
        "background" => {
            style.background_color = parse_color(value);
            style
                .paint_effects
                .retain(|name, _| !name.starts_with("background"));
            if style.background_color.is_none() && value != "none" {
                style
                    .paint_effects
                    .insert(property.to_owned(), value.to_owned());
            }
        }
        "border" => expand_border(style, value, lengths),
        // `appearance: none` is an accepted source-level reset. It has no IR
        // field because controls are always emitted with the canonical reset.
        "appearance" => {}

        // --- Position / layout longhands ---
        "position" => style.position = value.to_owned(),
        "display" => style.display = value.to_owned(),
        "box-sizing" => style.box_sizing = value.to_owned(),
        "top" => style.top = lengths.pixels(value),
        "left" => style.left = lengths.pixels(value),
        "width" => style.width = lengths.pixels(value),
        "height" => style.height = lengths.pixels(value),
        "min-width" => style.min_width = lengths.pixels(value),
        "min-height" => style.min_height = lengths.pixels(value),

        // --- Box model longhands ---
        "margin-top" => style.margin_top = lengths.pixels(value).unwrap_or(0.0),
        "margin-right" => style.margin_right = lengths.pixels(value).unwrap_or(0.0),
        "margin-bottom" => style.margin_bottom = lengths.pixels(value).unwrap_or(0.0),
        "margin-left" => style.margin_left = lengths.pixels(value).unwrap_or(0.0),
        "padding-top" => style.padding_top = lengths.pixels(value).unwrap_or(0.0),
        "padding-right" => style.padding_right = lengths.pixels(value).unwrap_or(0.0),
        "padding-bottom" => style.padding_bottom = lengths.pixels(value).unwrap_or(0.0),
        "padding-left" => style.padding_left = lengths.pixels(value).unwrap_or(0.0),

        // --- Flex ---
        "flex-direction" => style.flex_direction = Some(value.to_owned()),
        "flex-wrap" => {
            if let Some(wrap) = FlexWrap::from_css_keyword(value) {
                style.flex_wrap = Some(wrap);
            }
        }
        "align-items" => style.align_items = Some(value.to_owned()),
        "justify-content" => style.justify_content = Some(value.to_owned()),
        "align-self" => style.align_self = Some(value.to_owned()),
        "gap" => style.gap = lengths.pixels(value),
        "grid-template" => style.grid_template = Some(value.to_owned()),
        "grid-area" => style.grid_area = Some(value.to_owned()),

        // --- Paint longhands ---
        "background-color" => style.background_color = parse_color(value),
        "background-image"
        | "background-position"
        | "background-size"
        | "background-repeat"
        | "background-blend-mode"
        | "backdrop-filter"
        | "box-shadow"
        | "filter"
        | "opacity"
        | "content"
        | "mask"
        | "mask-size"
        | "mask-mode"
        | "mask-clip"
        | "mask-composite"
        | "mask-repeat"
        | "mix-blend-mode" => {
            style
                .paint_effects
                .insert(property.to_owned(), value.to_owned());
        }
        "border-width" => style.border_width = lengths.pixels(value).unwrap_or(0.0),
        "border-color" => style.border_color = parse_color(value),
        "border-radius" => style.border_radius = lengths.pixels(value).unwrap_or(0.0),
        "cursor" => style.cursor = Some(value.to_owned()),

        // --- Typography — inheritable ---
        "color" => style.color = parse_color(value),
        "font-family" => style.font_family = Some(value.to_owned()),
        "font-size" => style.font_size = lengths.pixels(value),
        "font-weight" => {
            style.font_weight = match value {
                "normal" => Some(400),
                "bold" => Some(700),
                _ => value.parse::<u16>().ok(),
            }
        }
        "line-height" => {
            style.line_height = Some(
                lengths
                    .pixels(value)
                    .map_or_else(|| value.to_owned(), |height| format!("{height}px")),
            );
        }

        // Unknown properties are silently ignored (DIA-01 handles diagnostics)
        _ => {}
    }
}

/// Propagate inheritable typography fields from `parent` into `child` for any
/// field that is still `None` after cascade.
pub fn apply_inheritance(child: &mut ComputedStyle, parent: &ComputedStyle) {
    if child.color.is_none() {
        child.color = parent.color.clone();
    }
    if child.font_family.is_none() {
        child.font_family = parent.font_family.clone();
    }
    if child.font_size.is_none() {
        child.font_size = parent.font_size;
    }
    if child.font_weight.is_none() {
        child.font_weight = parent.font_weight;
    }
    if child.line_height.is_none() {
        child.line_height = parent.line_height.clone();
    }
}

// ---------------------------------------------------------------------------
// Value parsers
// ---------------------------------------------------------------------------

/// Parse a CSS pixel length into an `f32`. Accepts `"0"`, `"8px"`, `"320px"`.
/// Returns `None` for non-pixel values or parse failures.
#[cfg(test)]
pub fn parse_px(value: &str) -> Option<f32> {
    let s = value.trim();
    if s == "0" {
        return Some(0.0);
    }
    s.strip_suffix("px")
        .and_then(|n| n.trim().parse::<f32>().ok())
}

/// Convert supported solid CSS colors into hex, including optional alpha.
pub fn parse_color(value: &str) -> Option<String> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("white") {
        return Some("#ffffff".to_owned());
    }
    if value.eq_ignore_ascii_case("black") {
        return Some("#000000".to_owned());
    }
    if let Some(hex) = value.strip_prefix('#') {
        return (matches!(hex.len(), 3 | 4 | 6 | 8)
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase());
    }
    if value == "transparent" {
        return Some("#00000000".to_owned());
    }
    let args = value
        .strip_prefix("hsl(")
        .or_else(|| value.strip_prefix("hsla("))?
        .strip_suffix(')')?;
    let parts: Vec<_> = args
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',' || ch == '/')
        .filter(|part| !part.is_empty())
        .collect();
    let (hue, saturation, lightness, alpha) = match parts.as_slice() {
        [h, s, l] => (*h, *s, *l, "1"),
        [h, s, l, a] => (*h, *s, *l, *a),
        _ => return None,
    };
    let ratio = |value: &str| -> Option<f32> {
        Some(
            value
                .strip_suffix('%')
                .unwrap_or(value)
                .parse::<f32>()
                .ok()?
                / 100.0,
        )
    };
    let hue = hue.trim_end_matches("deg").parse::<f32>().ok()?;
    let saturation = ratio(saturation)?.clamp(0.0, 1.0);
    let lightness = ratio(lightness)?.clamp(0.0, 1.0);
    let alpha = alpha
        .parse::<f32>()
        .ok()
        .or_else(|| ratio(alpha))?
        .clamp(0.0, 1.0);
    if ![hue, saturation, lightness, alpha]
        .iter()
        .all(|channel| channel.is_finite())
    {
        return None;
    }
    let channel = |offset: f32| {
        let k = (offset + hue / 30.0).rem_euclid(12.0);
        let chroma = saturation * lightness.min(1.0 - lightness);
        let channel = lightness - chroma * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (channel * 255.0).round() as u8
    };
    let mut hex = format!(
        "#{:02x}{:02x}{:02x}",
        channel(0.0),
        channel(8.0),
        channel(4.0)
    );
    if alpha < 1.0 {
        use std::fmt::Write;
        write!(&mut hex, "{:02x}", (alpha * 255.0).round() as u8).unwrap();
    }
    Some(hex)
}

// ---------------------------------------------------------------------------
// Shorthand helpers
// ---------------------------------------------------------------------------

fn record_inset(
    placement: &mut std::collections::BTreeMap<String, String>,
    value: &str,
    lengths: LengthContext,
) {
    let values: Vec<_> = value_parts(value).collect();
    let edges = match values.as_slice() {
        [all] => [*all; 4],
        [tb, lr] => [*tb, *lr, *tb, *lr],
        [top, lr, bottom] => [*top, *lr, *bottom, *lr],
        [top, right, bottom, left] => [*top, *right, *bottom, *left],
        _ => return,
    };
    for (side, length) in ["top", "right", "bottom", "left"].into_iter().zip(edges) {
        let value = lengths
            .pixels(length)
            .map_or_else(|| length.to_owned(), |pixels| format!("{pixels}px"));
        placement.insert(side.to_owned(), value);
    }
}

fn expand_edges(
    top: &mut f32,
    right: &mut f32,
    bottom: &mut f32,
    left: &mut f32,
    value: &str,
    lengths: LengthContext,
) {
    let parts = value_parts(value)
        .map(|part| {
            if part == "auto" {
                Some(0.0)
            } else {
                lengths.pixels(part)
            }
        })
        .collect::<Option<Vec<_>>>();
    let Some(parts) = parts else { return };
    match parts.as_slice() {
        [all] => {
            *top = *all;
            *right = *all;
            *bottom = *all;
            *left = *all;
        }
        [tb, lr] => {
            *top = *tb;
            *right = *lr;
            *bottom = *tb;
            *left = *lr;
        }
        [t, lr, b] => {
            *top = *t;
            *right = *lr;
            *bottom = *b;
            *left = *lr;
        }
        [t, r, b, l] => {
            *top = *t;
            *right = *r;
            *bottom = *b;
            *left = *l;
        }
        _ => {}
    }
}

fn expand_border(style: &mut ComputedStyle, value: &str, lengths: LengthContext) {
    let s = value.trim();
    if s == "0" || s == "none" {
        style.border_width = 0.0;
        style.border_color = None;
        return;
    }
    for part in value_parts(s) {
        if let Some(px) = lengths.pixels(part) {
            style.border_width = px;
        } else if let Some(color) = parse_color(part) {
            style.border_color = Some(color);
        }
        // "solid", "dashed", border-style keywords are ignored for v0.1
    }
}
