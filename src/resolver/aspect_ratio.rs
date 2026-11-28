//! Source-side bounds for lowering a preferred ratio to definite GPUI sizes.

use crate::ir::{BoxSizing, layout::ratio_border_size};
use crate::{HtmlNode, HtmlNodeKind};

use super::{
    ComputedStyle,
    cascade::parse_color,
    values::{LengthContext, Variables, parse_aspect_ratio, value_parts},
};

const SYNTAX_LOSS: &str = "expected auto or a positive finite numeric ratio";

pub(super) fn apply(style: &mut ComputedStyle, origin: (usize, usize), value: Option<&str>) {
    let parsed = value.and_then(parse_aspect_ratio);
    style.aspect_ratio = parsed.flatten();
    style.used_box_style.ratio_origin = Some(origin);
    style.used_box_style.ratio_loss = parsed.is_none().then_some(SYNTAX_LOSS);
}

pub(super) fn validate_layout(
    nodes: &[HtmlNode],
    styles: &mut [Option<(ComputedStyle, Variables)>],
    viewport: Option<(f32, f32)>,
    rem: f32,
) {
    if !styles
        .iter()
        .flatten()
        .any(|(style, _)| style.aspect_ratio.is_some())
    {
        return;
    }
    let mut has_flow_content = vec![false; nodes.len()];
    for node in nodes {
        let Some(parent) = node.parent_id else {
            continue;
        };
        let in_flow = match node.kind {
            HtmlNodeKind::Document => false,
            HtmlNodeKind::Text => node
                .text
                .as_ref()
                .is_some_and(|text| !text.trim_ascii().is_empty()),
            HtmlNodeKind::Element => {
                let (style, _) = styles[node.id].as_ref().unwrap();
                style.display != "none"
                    && !matches!(style.position.as_str(), "absolute" | "fixed")
                    && !crate::extractor::INVISIBLE_TAGS
                        .contains(&node.name.as_deref().unwrap_or(""))
            }
        };
        has_flow_content[parent] |= in_flow;
    }
    for node in nodes {
        let Some((style, _)) = styles[node.id].as_ref() else {
            continue;
        };
        let Some(ratio) = style.aspect_ratio else {
            continue;
        };
        let parent = node
            .parent_id
            .and_then(|id| styles[id].as_ref().map(|(style, _)| (&nodes[id], style)));
        let lengths = LengthContext {
            em: style.used_box_style.font_size(),
            rem,
            viewport,
            percent: None,
        };
        if let Err(reason) = validate_box(
            node,
            style,
            parent,
            has_flow_content[node.id],
            lengths,
            ratio,
        ) {
            let (style, _) = styles[node.id].as_mut().unwrap();
            style.aspect_ratio = None;
            style.used_box_style.ratio_loss = Some(reason);
        }
    }
}

fn validate_box(
    node: &HtmlNode,
    style: &ComputedStyle,
    parent: Option<(&HtmlNode, &ComputedStyle)>,
    has_flow_content: bool,
    lengths: LengthContext,
    ratio: f64,
) -> Result<(), &'static str> {
    let tag = node.name.as_deref().unwrap_or("");
    let raw = &style.used_box_style.properties;
    let is_initial = |name, initial: &str| {
        raw.get(name)
            .is_none_or(|value| value.eq_ignore_ascii_case(initial))
    };
    if style.display != "block"
        || !(is_block_tag(tag)
            || tag == "label" && (style.position == "absolute" || raw.contains_key("display")))
        || has_flow_content
        || !style.decorations.is_empty()
    {
        return Err("expected a non-replaced block without in-flow content");
    }
    match style.position.as_str() {
        "absolute" => {
            if !parent.is_some_and(|(node, style)| {
                !crate::extractor::TRANSPARENT_TAGS.contains(&node.name.as_deref().unwrap_or(""))
                    && matches!(style.position.as_str(), "absolute" | "relative")
            }) || style.top.is_none()
                || style.left.is_none()
                || !is_initial("right", "auto")
                || !is_initial("bottom", "auto")
            {
                return Err(
                    "absolute boxes require top and left offsets and a directly positioned parent",
                );
            }
        }
        "static" | "relative" => {
            if style.position == "relative"
                && ["top", "right", "bottom", "left"]
                    .iter()
                    .any(|side| !is_initial(side, "auto"))
            {
                return Err("relative boxes with offsets are outside the supported ratio subset");
            }
            if parent.is_some_and(|(node, style)| {
                style.display != "block"
                    || !(is_block_tag(node.name.as_deref().unwrap_or(""))
                        || crate::extractor::TRANSPARENT_TAGS
                            .contains(&node.name.as_deref().unwrap_or(""))
                        || style.used_box_style.properties.contains_key("display"))
            }) {
                return Err("in-flow boxes require a block container");
            }
        }
        _ => return Err("expected block flow or a supported absolute position"),
    }
    let sizes = [style.width, style.height];
    if !matches!(sizes, [Some(_), None] | [None, Some(_)])
        || ["width", "height"]
            .into_iter()
            .zip(sizes)
            .any(|(axis, size)| {
                if size.is_some() {
                    raw.get(axis).and_then(|value| lengths.pixels(value)) != size
                } else {
                    !is_initial(axis, "auto")
                }
            })
    {
        return Err("expected one resolved length and one auto dimension");
    }
    if !is_initial("min-width", "auto")
        || !is_initial("min-height", "auto")
        || !is_initial("max-width", "none")
        || !is_initial("max-height", "none")
    {
        return Err("min/max size constraints are outside the supported ratio subset");
    }
    let box_sizing = match style.box_sizing.as_str() {
        "content-box" => BoxSizing::ContentBox,
        "border-box" => BoxSizing::BorderBox,
        _ => return Err("expected content-box or border-box sizing"),
    };
    if [
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
    ]
    .iter()
    .any(|edge| {
        raw.get(*edge)
            .is_some_and(|value| lengths.pixels(value).is_none_or(|px| px < 0.0))
    }) {
        return Err("padding requires resolved nonnegative lengths");
    }
    if raw
        .keys()
        .any(|name| name.starts_with("border") && name != "border")
        || raw.get("border").is_some_and(|value| {
            if matches!(value.as_str(), "0" | "none") {
                return false;
            }
            let mut parts = value_parts(value);
            !matches!((parts.next(), parts.next(), parts.next(), parts.next()),
                (Some(width), Some("solid"), Some(color), None)
                if lengths.pixels(width).is_some_and(|width| width >= 0.0 && width.fract() == 0.0)
                    && parse_color(color).is_some())
        })
    {
        return Err("expected a uniform solid border with a whole-pixel width");
    }
    let border = 2.0 * style.border_width;
    let insets = [
        style.padding_left + style.padding_right + border,
        style.padding_top + style.padding_bottom + border,
    ];
    ratio_border_size(sizes, ratio, insets, &box_sizing)
        .ok_or("ratio dimensions must be finite and nonnegative")?;
    Ok(())
}

fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "div" | "main" | "section" | "article" | "aside" | "header" | "footer" | "nav"
    )
}
