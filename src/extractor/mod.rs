//! Extracts a [`SceneIr`] from a [`LaidOutScene`] + [`SceneSourceGraph`].
//!
//! The entry point is [`extract_ir`], which walks the HTML node list in
//! document order and maps each visible element or text run to an [`IrNode`].

mod map;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::ir::{
    BoxSizing, ControlKind, ExecutionMode, IrNode, LineHeight, NodeKind, RadioControl, SceneIr,
    SourceRef,
};
use crate::layout::{LaidOutNode, LaidOutScene};
use crate::source_graph::SceneSourceGraph;
use crate::{HtmlNode, HtmlNodeKind, radio::RadioMarkup};

/// Tags that are invisible structural boilerplate — skipped entirely along
/// with all of their descendants.
pub(crate) const INVISIBLE_TAGS: &[&str] = &["head", "meta", "link", "title", "style", "script"];

/// Tags that are transparent wrappers: we traverse their children but do not
/// emit an [`IrNode`] for the element itself. A transparent element's children
/// inherit their parent IR id from the transparent element's own parent.
/// This ensures `<html>` and `<body>` vanish from the IR so that re-parsing
/// the emitted HTML (which always has those wrappers) produces the same IR.
pub(crate) const TRANSPARENT_TAGS: &[&str] = &["html", "body"];

#[derive(Debug)]
pub enum ExtractorError {
    /// The laid-out scene and the source graph disagree on their scene ids.
    SceneIdMismatch { laid_out: String, graph: String },
}

impl std::fmt::Display for ExtractorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SceneIdMismatch { laid_out, graph } => write!(
                f,
                "scene id mismatch: laid_out={laid_out:?} graph={graph:?}"
            ),
        }
    }
}

impl std::error::Error for ExtractorError {}

/// Extract a [`SceneIr`] from the combination of a fully laid-out scene and its
/// source graph.
///
/// Nodes are emitted in pre-order (parent before children), mirroring document
/// order in the source HTML.
pub fn extract_ir(
    laid_out: &LaidOutScene,
    graph: &SceneSourceGraph,
) -> Result<SceneIr, ExtractorError> {
    if laid_out.scene_id != graph.scene_id {
        return Err(ExtractorError::SceneIdMismatch {
            laid_out: laid_out.scene_id.clone(),
            graph: graph.scene_id.clone(),
        });
    }

    let node_map: HashMap<usize, &LaidOutNode> =
        laid_out.nodes.iter().map(|n| (n.node.id, n)).collect();

    let radio_markup = RadioMarkup::new(&graph.html_nodes);
    let mut skipped_ids: HashSet<usize> = HashSet::new();
    let mut html_to_ir_id: HashMap<usize, String> = HashMap::new();
    let mut ir_nodes: Vec<IrNode> = Vec::new();

    for html_node in &graph.html_nodes {
        if let Some(pid) = html_node.parent_id
            && skipped_ids.contains(&pid)
        {
            skipped_ids.insert(html_node.id);
            continue;
        }

        match html_node.kind {
            HtmlNodeKind::Document => continue,
            HtmlNodeKind::Element => {
                process_element(
                    html_node,
                    &graph.html_nodes,
                    &node_map,
                    &radio_markup,
                    &mut html_to_ir_id,
                    &mut ir_nodes,
                    &mut skipped_ids,
                );
            }
            HtmlNodeKind::Text => {
                if let Some(node) = process_text(
                    html_node,
                    &node_map,
                    &html_to_ir_id,
                    ir_nodes.len(),
                    &skipped_ids,
                ) {
                    ir_nodes.push(node);
                }
            }
        }
    }

    for (label, target) in &radio_markup.labels {
        if let (Some(label_id), Some(target_id)) =
            (html_to_ir_id.get(label), html_to_ir_id.get(target))
        {
            let label = ir_nodes.iter_mut().find(|node| &node.id == label_id);
            if let Some(label) = label {
                label.kind = NodeKind::RadioLabel {
                    target: target_id.clone(),
                };
            }
        }
    }

    Ok(SceneIr {
        schema_version: if ir_nodes
            .iter()
            .any(|node| node.layout.aspect_ratio.is_some())
        {
            8
        } else if ir_nodes
            .iter()
            .any(|node| matches!(node.control_kind, Some(ControlKind::Radio(_))))
        {
            7
        } else if ir_nodes.iter().any(|node| node.paint.interaction.is_some()) {
            6
        } else if ir_nodes.iter().any(|node| node.letter_spacing.is_some()) {
            5
        } else if ir_nodes.iter().any(|node| node.layout.text_align.is_some()) {
            4
        } else if ir_nodes.iter().any(|node| node.layout.flex_wrap.is_some()) {
            3
        } else if ir_nodes
            .iter()
            .any(|node| matches!(node.control_kind, Some(ControlKind::TextInputWithValue(_))))
        {
            2
        } else {
            1
        },
        scene_id: laid_out.scene_id.clone(),
        corpus: "v0.1".to_owned(),
        execution_mode: ExecutionMode::Static,
        canvas: laid_out
            .nodes
            .iter()
            .find(|node| {
                node.node.name.as_deref() == Some("body")
                    && node.style.paint_effects.contains_key("background-image")
            })
            .map(|node| map::to_paint(&node.style)),
        nodes: ir_nodes,
    })
}

fn process_element(
    html_node: &HtmlNode,
    html_nodes: &[HtmlNode],
    node_map: &HashMap<usize, &LaidOutNode>,
    radio_markup: &RadioMarkup<'_>,
    html_to_ir_id: &mut HashMap<usize, String>,
    ir_nodes: &mut Vec<IrNode>,
    skipped_ids: &mut HashSet<usize>,
) {
    let tag = html_node.name.as_deref().unwrap_or("");
    if INVISIBLE_TAGS.contains(&tag) {
        skipped_ids.insert(html_node.id);
        return;
    }

    if TRANSPARENT_TAGS.contains(&tag) {
        if let Some(pir) = html_node
            .parent_id
            .and_then(|pid| html_to_ir_id.get(&pid))
            .cloned()
        {
            html_to_ir_id.insert(html_node.id, pir);
        }
        return;
    }

    let Some(lo) = node_map.get(&html_node.id) else {
        return;
    };

    let ir_id = format!("n{}", ir_nodes.len());
    html_to_ir_id.insert(html_node.id, ir_id.clone());

    let parent_ir_id = html_node
        .parent_id
        .and_then(|pid| html_to_ir_id.get(&pid))
        .cloned();

    let control_kind = if let Some(&name) = radio_markup.radios.get(&html_node.id) {
        Some(ControlKind::Radio(RadioControl {
            name: name.to_owned(),
            value: input_attribute(html_node, "value").map(str::to_owned),
            checked: radio_markup.initial.get(name) == Some(&html_node.id),
            checked_paint: lo.style.checked_paint.clone(),
        }))
    } else {
        control_kind(html_node)
    };
    let mut kind = if control_kind.is_some() {
        NodeKind::Control
    } else {
        NodeKind::Box
    };
    let mut layout = map::to_layout(&lo.style, &lo.geometry);
    if tag == "svg" {
        let mut markup = String::new();
        emit_svg_markup(
            &mut markup,
            html_node,
            html_nodes,
            lo.style.color.as_deref(),
        );
        kind = NodeKind::Svg(markup);
        skipped_ids.insert(html_node.id);

        let size = |name| {
            attribute(html_node, name)
                .and_then(|value| {
                    value
                        .strip_suffix("px")
                        .unwrap_or(value)
                        .parse::<f32>()
                        .ok()
                })
                .filter(|size| size.is_finite() && *size >= 0.0)
        };
        layout.width = layout.width.or_else(|| size("width"));
        layout.height = layout.height.or_else(|| size("height"));
        if let Some(ratio) = svg_aspect_ratio(html_node) {
            if layout.width.is_none() {
                layout.width = layout.height.map(|height| height * ratio);
            }
            if layout.height.is_none() {
                layout.height = layout.width.map(|width| width / ratio);
            }
        }
    }

    let typography = map::to_typography(&lo.style);
    let text_content = if control_kind
        .as_ref()
        .and_then(ControlKind::text_input_value)
        .is_some()
    {
        if layout.height.is_none() {
            let height = typography
                .as_ref()
                .and_then(|type_style| match type_style.line_height {
                    LineHeight::Length { value } => Some(value),
                    LineHeight::Normal => None,
                })
                .unwrap_or_else(|| lo.style.font_size.unwrap_or(16.0) * 1.2);
            layout.height = Some(if layout.box_sizing == BoxSizing::BorderBox {
                height + layout.padding.top + layout.padding.bottom + 2.0 * lo.style.border_width
            } else {
                height
            });
        }
        input_attribute(html_node, "placeholder").map(str::to_owned)
    } else {
        None
    };

    ir_nodes.push(IrNode {
        id: ir_id,
        kind,
        parent_id: parent_ir_id,
        control_kind,
        text_content,
        layout,
        paint: map::to_paint(&lo.style),
        typography,
        letter_spacing: lo.style.letter_spacing,
        source: SourceRef {
            doc_id: html_node.document_id,
            dom_path: html_node.dom_path.clone(),
            span: None,
        },
    });
}

fn attribute<'a>(node: &'a HtmlNode, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn svg_aspect_ratio(node: &HtmlNode) -> Option<f32> {
    let view_box = attribute(node, "viewBox")?;
    let coordinates = view_box
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    match coordinates.as_slice() {
        [_, _, width, height]
            if width.is_finite() && height.is_finite() && *width > 0.0 && *height > 0.0 =>
        {
            let ratio = width / height;
            (ratio.is_finite() && ratio > 0.0).then_some(ratio)
        }
        _ => None,
    }
}

fn emit_svg_markup(
    out: &mut String,
    node: &HtmlNode,
    nodes: &[HtmlNode],
    inherited_color: Option<&str>,
) {
    if node.kind == HtmlNodeKind::Text {
        out.push_str(
            &node
                .text
                .as_deref()
                .unwrap_or_default()
                .replace('<', "&lt;"),
        );
        return;
    }
    let Some(tag) = node.name.as_deref() else {
        return;
    };
    write!(out, "<{tag}").unwrap();
    for (name, value) in &node.attributes {
        write!(
            out,
            " {name}=\"{}\"",
            value.replace('"', "&quot;").replace('<', "&lt;")
        )
        .unwrap();
    }
    if tag == "svg" && attribute(node, "xmlns").is_none() {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }
    if let Some(color) = inherited_color.filter(|_| attribute(node, "color").is_none()) {
        write!(out, " color=\"{color}\"").unwrap();
    }
    out.push('>');
    for child in nodes
        .iter()
        .filter(|child| child.parent_id == Some(node.id))
    {
        emit_svg_markup(out, child, nodes, None);
    }
    write!(out, "</{tag}>").unwrap();
}

fn process_text(
    html_node: &HtmlNode,
    node_map: &HashMap<usize, &LaidOutNode>,
    html_to_ir_id: &HashMap<usize, String>,
    next_id: usize,
    skipped_ids: &HashSet<usize>,
) -> Option<IrNode> {
    let text = html_node.text.as_ref()?.trim().to_owned();
    if text.is_empty() {
        return None;
    }

    let parent_id = match html_node.parent_id {
        Some(pid) if !skipped_ids.contains(&pid) => pid,
        _ => return None,
    };

    let parent_ir_id = html_to_ir_id.get(&parent_id)?.clone();

    let style = node_map.get(&parent_id).map(|lo| &lo.style);
    let typography = style.and_then(map::to_typography);

    Some(IrNode {
        id: format!("n{next_id}"),
        kind: NodeKind::Text,
        parent_id: Some(parent_ir_id),
        control_kind: None,
        text_content: Some(text),
        layout: map::to_layout_default(),
        paint: map::to_paint_default(),
        typography,
        letter_spacing: style.and_then(|style| style.letter_spacing),
        source: SourceRef {
            doc_id: html_node.document_id,
            dom_path: html_node.dom_path.clone(),
            span: None,
        },
    })
}

pub(crate) fn control_kind(node: &HtmlNode) -> Option<ControlKind> {
    if node.name.as_deref()?.eq_ignore_ascii_case("button") {
        return Some(ControlKind::Button);
    }
    if !is_text_input(node) || unsupported_text_input_attribute(node).is_some() {
        return None;
    }

    let value = input_attribute(node, "value")
        .unwrap_or_default()
        .replace(['\r', '\n'], "");
    Some(if value.is_empty() {
        ControlKind::TextInput
    } else {
        ControlKind::TextInputWithValue(value)
    })
}

fn is_text_input(node: &HtmlNode) -> bool {
    node.name
        .as_deref()
        .is_some_and(|tag| tag.eq_ignore_ascii_case("input"))
        && input_type(node).is_none_or(|kind| kind.eq_ignore_ascii_case("text"))
}

pub(crate) fn unsupported_text_input_attribute(node: &HtmlNode) -> Option<&'static str> {
    if !is_text_input(node) {
        return None;
    }
    ["disabled", "readonly"]
        .into_iter()
        .find(|name| input_attribute(node, name).is_some())
}

pub(crate) fn input_type(node: &HtmlNode) -> Option<&str> {
    input_attribute(node, "type")
}

pub(crate) fn input_attribute<'a>(node: &'a HtmlNode, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}
