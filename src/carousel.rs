//! Typed compiler projection for the frozen DeSandro carousel. It reuses the
//! source graph, cascade and paint parsing while keeping its 3D subset local.

use crate::{
    HtmlNode, SceneSourceGraph,
    ir::TextAlign,
    paint::rgba,
    resolver::{ComputedStyle, ResolvedNode, resolve_scene},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CarouselScene {
    pub scene: [f32; 2],
    pub carousel: [f32; 2],
    pub scene_border: Border,
    pub margins_y: [f32; 2],
    pub perspective: f32,
    pub resting_z: f32,
    pub transition_seconds: f32,
    pub panels: Vec<Panel>,
    pub previous: String,
    pub next: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Panel {
    pub label: String,
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub rotation_degrees: f32,
    pub translate_z: f32,
    pub fill: [u8; 4],
    pub border: Border,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_color: [u8; 4],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Border {
    pub width: f32,
    pub color: [u8; 4],
}

pub fn compile(html: &str, css: &str) -> Result<CarouselScene, Box<dyn std::error::Error>> {
    // The frozen HTML is a body fragment, as in the Chromium replay shell.
    let document = format!("<!doctype html><html><body>{html}</body></html>");
    let graph = SceneSourceGraph::from_strings("carousel-rotate", &document, css)?;
    let resolved = resolve_scene(&graph)?;
    let one = |class| {
        let mut matches = resolved
            .nodes
            .iter()
            .filter(|node| has_class(&node.node, class));
        let node = matches.next().ok_or_else(|| format!("missing .{class}"))?;
        if matches.next().is_some() {
            return Err(format!("expected one .{class}"));
        }
        Ok(node)
    };
    let scene = one("scene")?;
    let carousel = one("carousel")?;
    let previous = one("previous-button")?;
    let next = one("next-button")?;
    require(
        scene.style.position == "relative",
        "scene must establish the containing block",
    )?;
    require(
        carousel.node.parent_id == Some(scene.node.id) && carousel.style.position == "absolute",
        "expected absolutely positioned .carousel inside .scene",
    )?;
    require(
        property(&carousel.style, "transform-style") == Some("preserve-3d"),
        "carousel requires transform-style: preserve-3d",
    )?;
    require(
        property(&scene.style, "margin").is_some_and(|value| {
            let parts = value.split_whitespace().collect::<Vec<_>>();
            matches!(
                parts.as_slice(),
                [_, "auto"] | [_, "auto", _] | [_, "auto", _, "auto"]
            )
        }),
        "scene requires auto side margins",
    )?;
    let previous_parent = previous
        .node
        .parent_id
        .and_then(|id| graph.html_nodes.get(id));
    require(
        previous.node.name.as_deref() == Some("button")
            && next.node.name.as_deref() == Some("button")
            && previous.node.parent_id == next.node.parent_id
            && previous_parent.is_some_and(|node| {
                node.name.as_deref() == Some("p")
                    && node.attributes.iter().any(|(name, value)| {
                        name == "style"
                            && value.split(';').any(|declaration| {
                                declaration.split_once(':').is_some_and(|(name, value)| {
                                    name.trim() == "text-align" && value.trim() == "center"
                                })
                            })
                    })
            }),
        "expected two buttons in the centered controls paragraph",
    )?;

    let scene_size = border_box_size(&scene.style)?;
    let scene_border = border(&scene.style)?;
    let interior = scene_size.map(|size| size - 2.0 * scene_border.width);
    let carousel_size = [
        relative_size(&carousel.style, "width", interior[0])?,
        relative_size(&carousel.style, "height", interior[1])?,
    ];
    let transform = property(&carousel.style, "transform").unwrap_or("");
    let (resting_z, rest) = transform_function(transform, "translateZ", "px")?;
    require(rest.is_empty(), "expected a resting translateZ transform")?;
    let perspective = value_in(property(&scene.style, "perspective").unwrap_or(""), "px")
        .filter(|distance| *distance >= 1.0)
        .ok_or("expected positive perspective in px")?;
    let transition_seconds = match property(&carousel.style, "transition") {
        Some(transition) => transition_seconds(transition)?,
        None => 0.0,
    };
    let panels = resolved
        .nodes
        .iter()
        .filter(|node| node.node.parent_id == Some(carousel.node.id))
        .map(|node| compile_panel(node, &graph))
        .collect::<Result<Vec<_>, String>>()?;
    require(!panels.is_empty(), "carousel requires panels")?;

    Ok(CarouselScene {
        scene: scene_size,
        carousel: carousel_size,
        scene_border,
        margins_y: [scene.style.margin_top, scene.style.margin_bottom],
        perspective,
        resting_z,
        transition_seconds,
        panels,
        previous: label(previous, &graph),
        next: label(next, &graph),
    })
}

fn compile_panel(node: &ResolvedNode, graph: &SceneSourceGraph) -> Result<Panel, String> {
    let style = &node.style;
    require(
        has_class(&node.node, "carousel__cell") && style.position == "absolute",
        "expected an absolutely positioned .carousel__cell",
    )?;
    require(
        style.text_align == Some(TextAlign::Center),
        "carousel cell text must be centered",
    )?;
    let transform = property(style, "transform").unwrap_or("");
    let (rotation_degrees, rest) = transform_function(transform, "rotateY", "deg")?;
    let (translate_z, rest) = transform_function(rest, "translateZ", "px")?;
    require(rest.is_empty(), "expected rotateY followed by translateZ")?;
    Ok(Panel {
        label: label(node, graph),
        origin: [
            style.left.ok_or("expected panel left in px")?,
            style.top.ok_or("expected panel top in px")?,
        ],
        size: border_box_size(style)?,
        rotation_degrees,
        translate_z,
        fill: color(style.background_color.as_deref(), "panel background")?,
        border: border(style)?,
        font_family: style
            .font_family
            .clone()
            .ok_or("missing panel font family")?,
        font_size: style.font_size.ok_or("expected font-size in px")?,
        font_weight: style.font_weight.ok_or("missing panel font weight")?,
        line_height: style
            .line_height
            .as_deref()
            .and_then(|value| value_in(value, "px"))
            .ok_or("expected line-height in px")?,
        text_color: color(style.color.as_deref(), "panel text color")?,
    })
}

fn has_class(node: &HtmlNode, class: &str) -> bool {
    node.attributes.iter().any(|(name, classes)| {
        name == "class" && classes.split_ascii_whitespace().any(|value| value == class)
    })
}

fn label(node: &ResolvedNode, graph: &SceneSourceGraph) -> String {
    graph
        .html_nodes
        .iter()
        .filter(|child| child.parent_id == Some(node.node.id))
        .filter_map(|child| child.text.as_deref())
        .collect::<String>()
        .trim()
        .to_owned()
}

fn require(predicate: bool, message: &str) -> Result<(), String> {
    if predicate {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn property<'a>(style: &'a ComputedStyle, name: &str) -> Option<&'a str> {
    style.projection_properties.get(name).map(String::as_str)
}

fn border_box_size(style: &ComputedStyle) -> Result<[f32; 2], String> {
    require(
        style.box_sizing == "border-box",
        "expected border-box sizing",
    )?;
    Ok([
        style
            .width
            .filter(|v| *v > 0.0)
            .ok_or("expected positive width in px")?,
        style
            .height
            .filter(|v| *v > 0.0)
            .ok_or("expected positive height in px")?,
    ])
}

fn relative_size(style: &ComputedStyle, name: &str, parent: f32) -> Result<f32, String> {
    let size = style
        .paint_placement
        .get(name)
        .map(String::as_str)
        .unwrap_or("");
    value_in(size, "px")
        .or_else(|| value_in(size, "%").map(|value| value / 100.0 * parent))
        .filter(|v| *v > 0.0)
        .ok_or_else(|| format!("expected positive carousel {name} in px or %"))
}

fn border(style: &ComputedStyle) -> Result<Border, String> {
    Ok(Border {
        width: style.border_width,
        color: color(style.border_color.as_deref(), "border-color")?,
    })
}

fn color(css: Option<&str>, property: &str) -> Result<[u8; 4], String> {
    css.and_then(rgba)
        .ok_or_else(|| format!("expected solid {property} color"))
}

fn value_in(value: &str, unit: &str) -> Option<f32> {
    value
        .trim()
        .strip_suffix(unit)?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
}

fn transform_function<'a>(
    input: &'a str,
    name: &str,
    unit: &str,
) -> Result<(f32, &'a str), String> {
    input
        .strip_prefix(name)
        .and_then(|input| input.strip_prefix('('))
        .and_then(|input| input.split_once(')'))
        .and_then(|(value, rest)| Some((value_in(value, unit)?, rest.trim_start())))
        .ok_or_else(|| format!("expected {name} with a {unit} value in {input:?}"))
}

fn transition_seconds(value: &str) -> Result<f32, String> {
    let parts: Vec<_> = value.split_whitespace().collect();
    let duration = match parts.as_slice() {
        ["none"] => return Ok(0.0),
        ["transform", duration] | ["transform", duration, "ease"] => duration,
        _ => return Err("expected transition: transform <duration> [ease]".to_owned()),
    };
    value_in(duration, "ms")
        .map(|v| v / 1000.0)
        .or_else(|| value_in(duration, "s"))
        .filter(|v| *v >= 0.0)
        .ok_or_else(|| "expected transition duration in ms or s".to_owned())
}
