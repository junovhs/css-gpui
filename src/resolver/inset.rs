//! Source bounds for physical offsets lowered to GPUI's absolute positioning.

use crate::extractor::{TRANSPARENT_TAGS, input_attribute};
use crate::{HtmlNode, HtmlNodeKind};

use super::cascade::edge_values;
use super::values::LengthContext;
use super::{ComputedStyle, ResolvedScene};

pub(crate) const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

const VALUE_LOSS: &str =
    "expected auto or a resolved length in px, em, rem, vw, vh, svw, svh, dvw, or dvh";

#[derive(Debug, Clone, Copy, PartialEq)]
struct Origin {
    source: (usize, usize),
    new_property: bool,
    supported: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Cascade {
    origins: [Option<Origin>; 4],
    box_loss: Option<&'static str>,
}

pub(crate) fn is_new_property(property: &str) -> bool {
    matches!(property, "inset" | "right" | "bottom")
}

pub(crate) fn pixels(lengths: LengthContext, value: &str) -> Option<f32> {
    // Require a dimension token or unitless zero. The general length resolver
    // also accepts calculations and percentages for the used-box path.
    let lower = value.to_ascii_lowercase();
    let number = lower.trim_end_matches(|ch: char| ch.is_ascii_alphabetic());
    let magnitude: f32 = number.parse().ok()?;
    if !magnitude.is_finite() || number.ends_with('.') {
        return None;
    }
    lengths.pixels(&lower)
}

fn supported(lengths: LengthContext, value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("auto") || pixels(lengths, value).is_some()
}

pub(crate) fn supports_syntax(property: &str, value: &str) -> bool {
    if value.contains("var(") {
        return true;
    }
    let lengths = LengthContext {
        viewport: Some((100.0, 100.0)),
        ..LengthContext::default()
    };
    if property == "inset" {
        edge_values(value).is_some_and(|edges| edges.iter().all(|edge| supported(lengths, edge)))
    } else {
        supported(lengths, value)
    }
}

/// Track the same winners the sorted cascade applies to the box and paint
/// views. Keep raw values available to projection, which resolves a wider set.
pub(super) fn track<'a>(
    style: &mut ComputedStyle,
    property: &str,
    value: Option<&'a str>,
    lengths: LengthContext,
    source: (usize, usize),
) -> Option<&'a str> {
    let new_property = is_new_property(property);
    let inset = &mut style.used_box_style.inset;
    let origin = |value: Option<&str>| Origin {
        source,
        new_property,
        supported: value.is_some_and(|value| supported(lengths, value)),
    };
    if property == "inset" {
        let edges = value.and_then(edge_values).map(|edges| edges.map(Some));
        inset.get_or_insert_with(Default::default).origins =
            edges.unwrap_or([None; 4]).map(|edge| Some(origin(edge)));
    } else if let Some(side) = SIDES.iter().position(|side| *side == property) {
        inset.get_or_insert_with(Default::default).origins[side] = Some(origin(value));
    } else {
        return value;
    }
    value.or(Some("auto"))
}

pub(super) fn finish_cascade(style: &mut ComputedStyle, lengths: LengthContext) {
    let loss = validate_box_values(style, lengths).err();
    if let Some(inset) = &mut style.used_box_style.inset {
        inset.box_loss = loss;
    }
}

pub(crate) fn losses<'a>(
    scene: &'a ResolvedScene,
    nodes: &'a [HtmlNode],
) -> impl Iterator<Item = ((usize, usize), &'static str)> + 'a {
    let mut styles = Vec::new();
    if scene
        .nodes
        .iter()
        .any(|node| node.style.used_box_style.inset.is_some())
    {
        styles.resize(nodes.len(), None);
        for node in &scene.nodes {
            styles[node.node.id] = Some(&node.style);
        }
    }
    scene.nodes.iter().flat_map(move |node| {
        let Some(cascade) = &node.style.used_box_style.inset else {
            return Vec::new();
        };
        let origins = cascade.origins.iter().flatten();
        let Some(inset) = origins.clone().find(|origin| origin.new_property) else {
            return Vec::new();
        };
        let mut losses: Vec<_> = origins
            .filter(|origin| !origin.supported)
            .map(|origin| (origin.source, VALUE_LOSS))
            .collect();
        if losses.is_empty()
            && let Some(reason) = validate_box(&node.node, &node.style, nodes, &styles)
                .err()
                .or(cascade.box_loss)
        {
            losses.push((inset.source, reason));
        }
        losses
    })
}

fn validate_box(
    node: &HtmlNode,
    style: &ComputedStyle,
    nodes: &[HtmlNode],
    styles: &[Option<&ComputedStyle>],
) -> Result<(), &'static str> {
    let offsets = [style.top, style.right, style.bottom, style.left];
    match style.position.as_str() {
        "static" => return Ok(()),
        "relative" if offsets.iter().all(Option::is_none) => return Ok(()),
        "absolute" | "fixed" => {}
        _ => return Err("physical offsets require absolute or fixed positioning"),
    }
    if !is_box(node) || !matches!(style.display.as_str(), "block" | "flex" | "inline-flex") {
        return Err("expected a supported non-replaced box for positioned physical offsets");
    }
    let ancestors = std::iter::successors(Some(node), |node| {
        node.parent_id.and_then(|id| nodes.get(id))
    });
    if ancestors.clone().any(|ancestor| {
        input_attribute(ancestor, "dir").is_some_and(|dir| !dir.eq_ignore_ascii_case("ltr"))
    }) {
        return Err("physical offsets require left-to-right direction");
    }
    if style.position == "absolute" {
        let immediate_parent = ancestors
            .skip(1)
            .find(|node| node.kind == HtmlNodeKind::Element);
        if let Some(parent) = immediate_parent {
            let parent_style = styles[parent.id].unwrap();
            if TRANSPARENT_TAGS.contains(&parent.name.as_deref().unwrap_or("")) {
                if std::iter::successors(Some(parent), |node| {
                    node.parent_id.and_then(|id| nodes.get(id))
                })
                .filter_map(|node| styles[node.id])
                .any(|style| style.position != "static")
                {
                    return Err("absolute physical offsets require an emitted containing block");
                }
            } else if !is_box(parent)
                || !matches!(
                    parent_style.position.as_str(),
                    "relative" | "absolute" | "fixed"
                )
                || !matches!(
                    parent_style.display.as_str(),
                    "block" | "flex" | "inline-flex"
                )
                || (parent.name.as_deref() == Some("span")
                    && parent_style.position == "relative"
                    && !parent_style
                        .used_box_style
                        .properties
                        .contains_key("display"))
            {
                return Err(
                    "absolute physical offsets require a directly positioned block or flex parent",
                );
            }
        }
    }
    Ok(())
}

fn is_box(node: &HtmlNode) -> bool {
    matches!(
        node.name.as_deref(),
        Some(
            "article" | "aside" | "div" | "footer" | "header" | "main" | "nav" | "section" | "span"
        )
    )
}

fn validate_box_values(style: &ComputedStyle, lengths: LengthContext) -> Result<(), &'static str> {
    if !matches!(style.position.as_str(), "absolute" | "fixed") {
        return Ok(());
    }
    let raw = &style.used_box_style.properties;
    let has_value = |property, initial: &str| {
        raw.get(property)
            .is_some_and(|value| !value.eq_ignore_ascii_case(initial))
    };
    if raw.iter().any(|(property, value)| {
        if property.starts_with("margin-") {
            lengths.pixels(value).is_none()
        } else if property.starts_with("padding-") {
            lengths.pixels(value).is_none_or(|px| px < 0.0)
        } else {
            false
        }
    }) || style
        .align_self
        .as_deref()
        .is_some_and(|value| value != "auto")
    {
        return Err(
            "physical offsets require resolved margins and padding, without auto margins or explicit self-alignment",
        );
    }
    for (axis, minimum, first, last, size) in [
        ("width", "min-width", style.left, style.right, style.width),
        (
            "height",
            "min-height",
            style.top,
            style.bottom,
            style.height,
        ),
    ] {
        if first.is_none() && last.is_none() {
            return Err("physical offsets require at least one non-auto edge on each axis");
        }
        if size.is_some_and(|size| size >= 0.0) {
            continue;
        }
        if size.is_some()
            || first.is_none()
            || last.is_none()
            || has_value(axis, "auto")
            || style.aspect_ratio.is_some()
        {
            return Err(
                "physical offsets require a resolved size or opposite edges on an auto axis",
            );
        }
        if has_value(minimum, "auto") {
            return Err("opposite-edge stretch requires an unconstrained auto size");
        }
    }
    Ok(())
}
