//! Viewport paint in the flat stacking context around the selected 3D scene.

use std::collections::BTreeMap;

use crate::{
    layout::{BorderBox, UsedBoxLayout},
    projection::Point,
    resolver::{ResolvedNode, values::LengthContext},
};

use super::{Loss, Material, css_value, material::number, raster::Overlay, record_loss};

pub(super) fn compile(
    root: &ResolvedNode,
    ancestors: &[&ResolvedNode],
    layout: &mut UsedBoxLayout,
    viewport: Point,
    losses: &mut Vec<Loss>,
) -> Vec<Overlay> {
    let mut ordered = Vec::new();
    let mut inherited_color = None;
    let mut visibility = "visible";
    let mut flat_backdrop = None;
    let root_z = context_z_index(root);
    // Ancestors arrive nearest first. Visit from the document down to resolve
    // inherited paint and assign source order to equally stacked pseudo-boxes.
    for (depth, node) in ancestors.iter().rev().enumerate() {
        let style = &node.style;
        let css = &style.projection_properties;
        inherited_color = css
            .get("color")
            .and_then(|color| crate::paint::rgba(color))
            .or(inherited_color);
        visibility = css_value(css, "visibility", visibility);
        if style.display == "none" {
            break;
        }
        for decoration in &style.decorations {
            let effects = &decoration.effects;
            let blend = css_value(effects, "mix-blend-mode", "normal");
            let pseudo = decoration.pseudo.as_str();
            let css = &style.decoration_projection_properties[pseudo];
            let box_style = &style.decoration_box_styles[pseudo].properties;
            if blend == "normal"
                || number(css_value(effects, "opacity", "1")) == Some(0.0)
                || css_value(css, "visibility", visibility) != "visible"
                || css_value(box_style, "display", "inline") == "none"
            {
                continue;
            }
            let path = format!("{}::{pseudo}", node.node.dom_path);
            let order = if pseudo == "before" {
                depth
            } else {
                2 * ancestors.len() - depth
            };
            let z = z_index(css);
            if blend == "overlay"
                && let (Some(root_z), Some(z)) = (root_z, z)
                && z > 0
                && (z > root_z || z == root_z && pseudo == "after")
                && matches!(css_value(effects, "content", "normal"), "\"\"" | "''")
                && decoration.radius == 0.0
                && simple_coordinates(css)
                && *flat_backdrop.get_or_insert_with(|| {
                    ancestors
                        .iter()
                        .all(|ancestor| flat_ancestor(ancestor, layout, viewport))
                })
                && let Ok(Some(bounds)) = layout.border_box(node.node.id, Some(pseudo))
                && covers_viewport(bounds, viewport)
            {
                let color = css
                    .get("color")
                    .and_then(|color| crate::paint::rgba(color))
                    .or(inherited_color);
                let mut exact = true;
                let mut loss = |property: &str, value: &str| {
                    if property != "mix-blend-mode" {
                        exact = false;
                        record_loss(losses, &path, property, value);
                    }
                };
                let mut paint = Material::from_css(css, effects, color, &mut loss);
                paint.configure_background(
                    css,
                    effects,
                    bounds.size.map(|value| value as f32),
                    LengthContext {
                        em: style.decoration_box_styles[pseudo].font_size(),
                        rem: layout.root_font_size(),
                        viewport: Some((viewport[0] as f32, viewport[1] as f32)),
                        percent: None,
                    },
                    &mut loss,
                );
                if exact {
                    if !paint.is_empty() {
                        ordered.push(((z, order), Overlay { bounds, paint }));
                    }
                    continue;
                }
            }
            record_loss(losses, &path, "mix-blend-mode", blend);
        }
    }
    ordered.sort_by_key(|(key, _)| *key);
    ordered.into_iter().map(|(_, overlay)| overlay).collect()
}

fn z_index(css: &BTreeMap<String, String>) -> Option<i32> {
    match css_value(css, "z-index", "auto") {
        "auto" => Some(0),
        value => value.parse().ok(),
    }
}

fn context_z_index(root: &ResolvedNode) -> Option<i32> {
    let css = &root.style.projection_properties;
    let positioned = matches!(root.style.position.as_str(), "absolute" | "relative");
    // Avoid interleaving with descendants of a root that paints directly
    // into its parent stack. A transformed or perspective root is atomic.
    let atomic = css_value(css, "transform", "none") != "none"
        || css_value(css, "perspective", "none") != "none"
        || positioned && css_value(css, "z-index", "auto") != "auto";
    if atomic { z_index(css) } else { None }
}

pub(super) fn flat_ancestor(
    node: &ResolvedNode,
    layout: &mut UsedBoxLayout,
    viewport: Point,
) -> bool {
    let style = &node.style;
    let css = &style.projection_properties;
    simple_coordinates(css)
        && !matches!(style.position.as_str(), "fixed" | "sticky")
        && css_value(css, "z-index", "auto") == "auto"
        && defaults(
            &style.paint_effects,
            &[
                ("opacity", "1"),
                ("filter", "none"),
                ("backdrop-filter", "none"),
                ("mask", "none"),
                ("mix-blend-mode", "normal"),
            ],
        )
        && (["overflow", "overflow-x", "overflow-y"]
            .iter()
            .all(|property| css_value(css, property, "visible") == "visible")
            || (style.border_radius == 0.0
                && layout
                    .border_box(node.node.id, None)
                    .ok()
                    .flatten()
                    .is_some_and(|bounds| covers_viewport(bounds, viewport))))
}

fn simple_coordinates(css: &BTreeMap<String, String>) -> bool {
    defaults(
        css,
        &[
            ("transform", "none"),
            ("translate", "none"),
            ("rotate", "none"),
            ("scale", "none"),
            ("perspective", "none"),
            ("transform-style", "flat"),
            ("clip", "auto"),
            ("clip-path", "none"),
            ("mask-image", "none"),
            ("isolation", "auto"),
            ("contain", "none"),
            ("container-type", "normal"),
            ("content-visibility", "visible"),
            ("will-change", "auto"),
        ],
    )
}

fn defaults(css: &BTreeMap<String, String>, properties: &[(&str, &str)]) -> bool {
    properties
        .iter()
        .all(|&(name, initial)| css_value(css, name, initial) == initial)
}

pub(super) fn covers_viewport(bounds: BorderBox, viewport: Point) -> bool {
    // Viewport-relative CSS lengths are computed as f32 in the layout pass.
    let tolerance = 1.0 / 64.0;
    (0..2).all(|axis| {
        bounds.origin[axis] <= tolerance
            && bounds.origin[axis] + bounds.size[axis] >= viewport[axis] - tolerance
    })
}
