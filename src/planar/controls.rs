//! Flat authored controls positioned over a separately rendered 3D scene.

use std::collections::HashMap;

use crate::{
    HtmlNodeKind, SceneSourceGraph,
    layout::{BorderBox, UsedBoxLayout},
    paint::rgba,
    projection::{Layer, accumulated_matrix},
    resolver::{ResolvedNode, ResolvedScene, SnapshotRadioLabel},
};

use super::{css_value, overlay};

#[derive(Clone, Debug, serde::Serialize)]
pub struct ViewportControl {
    /// Left, top, width, and height in viewport CSS pixels.
    pub bounds: [f32; 4],
    pub corner_radius: f32,
    pub background: [u8; 4],
    pub color: [u8; 4],
    pub glyph: char,
    pub font_size: f32,
    pub padding_top: f32,
    /// The anchor's z-index and source order, followed by the local z-index.
    pub stacking_order: (i32, usize, i32),
}

impl ViewportControl {
    pub fn contains(&self, [x, y]: [f32; 2]) -> bool {
        let [left, top, width, height] = self.bounds;
        let [x, y] = [x - left, y - top];
        if x < 0.0 || y < 0.0 || x >= width || y >= height {
            return false;
        }
        let radius = self.corner_radius;
        let dx = x - x.clamp(radius, width - radius);
        let dy = y - y.clamp(radius, height - radius);
        dx * dx + dy * dy <= radius * radius
    }
}

/// Resolve single-glyph labels in a sibling stacking context above the 3D
/// scene. The current subset supports viewport-clipped, axis-aligned absolute
/// boxes and a translated anchor containing out-of-flow children.
pub fn viewport_controls(
    graph: &SceneSourceGraph,
    scene: &ResolvedScene,
    root: usize,
    viewport: [u32; 2],
    labels: &[SnapshotRadioLabel<'_>],
) -> HashMap<usize, ViewportControl> {
    let viewport = viewport.map(f64::from);
    let nodes: HashMap<_, _> = scene
        .nodes
        .iter()
        .map(|node| (node.node.id, node))
        .collect();
    let Some(root) = nodes.get(&root) else {
        return HashMap::new();
    };
    let mut layout = UsedBoxLayout::new(scene, viewport);
    let rem = layout.root_font_size();
    let mut ancestor_id = root.node.parent_id;
    while let Some(ancestor) = ancestor_id.and_then(|id| nodes.get(&id)).copied() {
        if !overlay::flat_ancestor(ancestor, &mut layout, viewport) {
            return HashMap::new();
        }
        ancestor_id = ancestor.node.parent_id;
    }
    let mut anchors = HashMap::new();
    labels
        .iter()
        .filter_map(|label| {
            if label.checkbox || !label.text.is_empty() {
                return None;
            }
            let glyph = glyph(label.generated_content?)?;
            let node = *nodes.get(&label.node_id)?;
            let parent_id = node.node.parent_id?;
            let anchor = *anchors.entry(parent_id).or_insert_with(|| {
                let parent = *nodes.get(&parent_id)?;
                if parent.node.parent_id != root.node.parent_id || z(parent)? <= z(root)? {
                    return None;
                }
                if !parent.style.decorations.is_empty()
                    || ["overflow", "overflow-x", "overflow-y"]
                        .iter()
                        .any(|property| {
                            css_value(&parent.style.projection_properties, property, "visible")
                                != "visible"
                        })
                    || scene.nodes.iter().any(|child| {
                        child.node.parent_id == Some(parent_id)
                            && child.style.display != "none"
                            && child.style.position != "absolute"
                    })
                    || graph.html_nodes.iter().any(|child| {
                        child.parent_id == Some(parent_id)
                            && child.kind == HtmlNodeKind::Text
                            && child
                                .text
                                .as_deref()
                                .is_some_and(|text| !text.trim().is_empty())
                    })
                {
                    return None;
                }
                let box_ = layout.absolute_border_box(parent_id, None, true).ok()?;
                let translated = translated_box(parent, box_, viewport, rem)?;
                Some((translated, z(parent)?))
            });
            let (anchor, anchor_z) = anchor?;
            let bounds = layout
                .absolute_border_box(label.node_id, Some(anchor), false)
                .ok()?;
            let bounds = translated_box(node, bounds, viewport, rem)?;
            let style = &node.style;
            let pseudo = style.decorations.iter().find_map(|decoration| {
                let content = decoration.effects.get("content")?;
                (content == label.generated_content?).then_some(decoration.pseudo.as_str())
            })?;
            let background = style.background_color.as_deref().and_then(rgba)?;
            let color = style.decoration_projection_properties[pseudo]
                .get("color")
                .or_else(|| style.projection_properties.get("color"))
                .and_then(|color| rgba(color))?;
            Some((
                label.node_id,
                ViewportControl {
                    bounds: [
                        bounds.origin[0],
                        bounds.origin[1],
                        bounds.size[0],
                        bounds.size[1],
                    ]
                    .map(|value| value as f32),
                    corner_radius: style
                        .border_radius
                        .clamp(0.0, (bounds.size[0].min(bounds.size[1]) / 2.0) as f32),
                    background,
                    color,
                    glyph,
                    font_size: style.decoration_box_styles[pseudo].font_size(),
                    padding_top: style.padding_top,
                    stacking_order: (anchor_z, parent_id, z(node)?),
                },
            ))
        })
        .collect()
}

fn z(node: &ResolvedNode) -> Option<i32> {
    match css_value(&node.style.projection_properties, "z-index", "auto") {
        "auto" => Some(0),
        value => value.parse().ok(),
    }
}

fn translated_box(
    node: &ResolvedNode,
    bounds: BorderBox,
    viewport: [f64; 2],
    rem: f32,
) -> Option<BorderBox> {
    let css = &node.style.projection_properties;
    if ["perspective", "clip-path", "translate", "rotate", "scale"]
        .iter()
        .any(|name| css_value(css, name, "none") != "none")
    {
        return None;
    }
    let matrix = accumulated_matrix(
        &[Layer {
            origin: bounds.origin,
            size: bounds.size,
            font_size: node.style.used_box_style.font_size(),
            css,
        }],
        viewport,
        rem,
    )
    .ok()?;
    // Project a unit box so rotations and scales are detectable even for
    // a zero-sized anchor. Keep the author's sizing and transform origin.
    let projected = matrix.project_box([1.0, 1.0], viewport);
    let [Some(a), Some(b), _, Some(d)] = projected.corners else {
        return None;
    };
    if (b[0] - a[0] - 1.0).abs() > 1e-8
        || (d[1] - a[1] - 1.0).abs() > 1e-8
        || (a[0] - d[0]).abs() > 1e-8
        || (a[1] - b[1]).abs() > 1e-8
    {
        return None;
    }
    Some(BorderBox {
        origin: a,
        ..bounds
    })
}

fn glyph(content: &str) -> Option<char> {
    let quote = content
        .chars()
        .next()
        .filter(|ch| matches!(ch, '"' | '\''))?;
    let text = content.strip_prefix(quote)?.strip_suffix(quote)?;
    let character = if let Some(hex) = text.strip_prefix('\\') {
        char::from_u32(u32::from_str_radix(hex.trim_ascii_end(), 16).ok()?)?
    } else {
        let mut characters = text.chars();
        let character = characters.next()?;
        if characters.next().is_some() {
            return None;
        }
        character
    };
    (!character.is_whitespace() && !character.is_control()).then_some(character)
}
