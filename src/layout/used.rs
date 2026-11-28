//! Bounded, untransformed layout for explicitly sized 3D scene boxes.
//!
//! This pass uses physical CSS axes in a horizontal, left-to-right document.
//! It solves unbordered absolute boxes with explicit insets, and sized,
//! unbordered element items in row-wrapping flex containers with start or end
//! line packing. Absolute generated faces use the same containing-block rules
//! as elements. The author cascade supplies all sizes and insets. Callers use
//! a settled viewport (the large, small and dynamic viewport sizes coincide).

use std::collections::HashMap;

use crate::resolver::{ResolvedNode, ResolvedScene, UsedBoxStyle, values::LengthContext};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct BorderBox {
    /// The untransformed border origin at scroll zero, in CSS pixels.
    pub origin: [f64; 2],
    pub size: [f64; 2],
}

#[derive(Clone, Copy)]
struct UsedBox {
    border: BorderBox,
    content: BorderBox,
}

#[derive(Debug)]
pub struct BoxLayoutError {
    node: usize,
    detail: String,
}

impl std::fmt::Display for BoxLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "box layout at node {}: {}", self.node, self.detail)
    }
}

impl std::error::Error for BoxLayoutError {}

/// Compute and cache the used boxes needed for projection from a fixed cascade.
pub struct UsedBoxLayout<'a> {
    nodes: HashMap<usize, &'a ResolvedNode>,
    children: HashMap<usize, Vec<usize>>,
    boxes: HashMap<usize, Option<UsedBox>>,
    viewport: [f64; 2],
    root_font_size: f32,
}

impl<'a> UsedBoxLayout<'a> {
    pub fn new(scene: &'a ResolvedScene, viewport: [f64; 2]) -> Self {
        let mut children: HashMap<_, Vec<_>> = HashMap::new();
        for resolved in &scene.nodes {
            let node = &resolved.node;
            if let Some(parent) = node.parent_id {
                children.entry(parent).or_default().push(node.id);
            }
        }
        let root_font_size = scene
            .nodes
            .iter()
            .find(|node| node.node.name.as_deref() == Some("html"))
            .and_then(|node| node.style.font_size)
            .unwrap_or(16.0);
        Self {
            nodes: scene
                .nodes
                .iter()
                .map(|node| (node.node.id, node))
                .collect(),
            children,
            boxes: HashMap::new(),
            viewport,
            root_font_size,
        }
    }

    pub fn root_font_size(&self) -> f32 {
        self.root_font_size
    }

    /// Return the border box for a node or its named generated pseudo-element.
    /// A hidden element or an absent generated box returns `Ok(None)`.
    pub fn border_box(
        &mut self,
        id: usize,
        pseudo: Option<&str>,
    ) -> Result<Option<BorderBox>, BoxLayoutError> {
        let used = if let Some(pseudo) = pseudo {
            let node = self.node(id)?;
            let Some(style) = node.style.decoration_box_styles.get(pseudo) else {
                return Ok(None);
            };
            if style.value("display", "inline") == "none" || self.hidden(id) {
                return Ok(None);
            }
            if style.value("position", "static") != "absolute" {
                return Err(invalid(
                    id,
                    "expected an absolutely positioned pseudo-element",
                ));
            }
            let container = self.containing_block(Some(id))?;
            Some(self.absolute_box(
                id,
                self.style_box(id, style, container, false)?,
                container,
                false,
            )?)
        } else {
            self.element_box(id)?
        };
        Ok(used.map(|used| used.border))
    }

    /// Lay out an absolute box, optionally with an explicit containing block.
    /// The caller may establish that the element has no in-flow content,
    /// so an unstretched auto dimension can collapse to its padding.
    pub(crate) fn absolute_border_box(
        &mut self,
        id: usize,
        area: Option<BorderBox>,
        empty: bool,
    ) -> Result<BorderBox, BoxLayoutError> {
        let node = self.node(id)?;
        let style = &node.style.used_box_style;
        style.require(id, "position", "static", &["absolute"])?;
        let area = match area {
            Some(area) => area,
            None => self.containing_block(node.node.parent_id)?,
        };
        let sized = self.style_box(id, style, area, true)?;
        Ok(self.absolute_box(id, sized, area, empty)?.border)
    }

    fn node(&self, id: usize) -> Result<&'a ResolvedNode, BoxLayoutError> {
        self.nodes
            .get(&id)
            .copied()
            .ok_or_else(|| invalid(id, "unknown element"))
    }

    fn hidden(&self, id: usize) -> bool {
        let mut current = self.nodes.get(&id).copied();
        while let Some(node) = current {
            if node.style.display == "none" {
                return true;
            }
            current = node
                .node
                .parent_id
                .and_then(|id| self.nodes.get(&id).copied());
        }
        false
    }

    fn element_box(&mut self, id: usize) -> Result<Option<UsedBox>, BoxLayoutError> {
        if let Some(used) = self.boxes.get(&id) {
            return Ok(*used);
        }
        let node = self.node(id)?;
        if self.hidden(id) {
            self.boxes.insert(id, None);
            return Ok(None);
        }
        let style = &node.style.used_box_style;
        if style.value("display", "inline") == "contents" {
            return Err(invalid(
                id,
                "display: contents is outside the bounded box layout",
            ));
        }
        if style.value("position", "static") == "absolute" {
            let container = self.containing_block(node.node.parent_id)?;
            let sized = self.style_box(id, style, container, false)?;
            let used = self.absolute_box(id, sized, container, false)?;
            self.boxes.insert(id, Some(used));
            return Ok(Some(used));
        }

        if let Some(parent) = node.node.parent_id
            && self
                .nodes
                .get(&parent)
                .is_some_and(|node| node.style.display == "flex")
        {
            self.flex_items(parent)?;
            if let Some(used) = self.boxes.get(&id) {
                return Ok(*used);
            }
        }
        Err(invalid(
            id,
            "expected an absolute box or a sized item in a wrapping flex box",
        ))
    }

    fn containing_block(&mut self, mut parent: Option<usize>) -> Result<BorderBox, BoxLayoutError> {
        while let Some(node) = parent.and_then(|id| self.nodes.get(&id).copied()) {
            let css = &node.style.projection_properties;
            if node.style.position != "static"
                || ["transform", "perspective"]
                    .iter()
                    .any(|key| css.get(*key).is_some_and(|value| value != "none"))
            {
                return self
                    .element_box(node.node.id)?
                    .map(|used| used.border)
                    .ok_or_else(|| invalid(node.node.id, "hidden containing block"));
            }
            parent = node.node.parent_id;
        }
        Ok(BorderBox {
            origin: [0.0; 2],
            size: self.viewport,
        })
    }

    fn style_box(
        &self,
        id: usize,
        style: &'a UsedBoxStyle,
        area: BorderBox,
        transfer_ratio: bool,
    ) -> Result<SizedBox, BoxLayoutError> {
        for (name, initial) in [
            ("min-width", "auto"),
            ("min-height", "auto"),
            ("max-width", "none"),
            ("max-height", "none"),
        ] {
            style.require(id, name, initial, &[initial])?;
        }
        if style
            .properties
            .keys()
            .any(|name| name.starts_with("border"))
        {
            return Err(invalid(
                id,
                "authored borders are outside the bounded box layout",
            ));
        }
        let lengths = LengthContext {
            em: style.font_size(),
            rem: self.root_font_size,
            viewport: Some((self.viewport[0] as f32, self.viewport[1] as f32)),
            percent: None,
        };
        style.require(
            id,
            "box-sizing",
            "content-box",
            &["content-box", "border-box"],
        )?;
        let border_sizing = style.value("box-sizing", "content-box") == "border-box";
        let read = |name, initial, percent| read_length(id, style, lengths, name, initial, percent);
        let mut dimensions = [
            read("width", "auto", area.size[0])?,
            read("height", "auto", area.size[1])?,
        ];
        let mut padding = [0.0; 4];
        let mut margins = [None; 4];
        let mut offsets = [None; 4];
        for (i, (offset, margin, pad)) in [
            ("top", "margin-top", "padding-top"),
            ("right", "margin-right", "padding-right"),
            ("bottom", "margin-bottom", "padding-bottom"),
            ("left", "margin-left", "padding-left"),
        ]
        .into_iter()
        .enumerate()
        {
            let value = read(pad, "0", area.size[0])?;
            padding[i] = value
                .filter(|value| *value >= 0.0)
                .ok_or_else(|| invalid(id, format!("expected nonnegative {pad}")))?;
            margins[i] = read(margin, "0", area.size[0])?;
            offsets[i] = read(offset, "auto", area.size[1 - i % 2])?;
        }
        let total_padding = [padding[3] + padding[1], padding[0] + padding[2]];
        let ratio = style.value("aspect-ratio", "auto");
        if ratio != "auto" {
            if !transfer_ratio {
                return Err(invalid(id, format!("unsupported aspect-ratio: {ratio}")));
            }
            let (width, height) = ratio.split_once('/').unwrap_or((ratio, "1"));
            let positive = |value: &str| {
                value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value > 0.0)
            };
            let ratio = positive(width)
                .zip(positive(height))
                .map(|(width, height)| width / height)
                .ok_or_else(|| invalid(id, "unsupported aspect-ratio"))?;
            // Transfer an explicit dimension only when the other axis is
            // auto-sized from content, rather than stretched between insets.
            match dimensions {
                [Some(width), None] if offsets[0].is_none() || offsets[2].is_none() => {
                    dimensions[1] = Some(width / ratio);
                }
                [None, Some(height)] if offsets[1].is_none() || offsets[3].is_none() => {
                    dimensions[0] = Some(height * ratio);
                }
                _ => {}
            }
        }
        for (dimension, padding) in dimensions.iter_mut().zip(total_padding) {
            if let Some(length) = *dimension {
                if length < 0.0 {
                    return Err(invalid(id, "expected nonnegative width and height"));
                }
                *dimension = Some(if border_sizing {
                    length.max(padding)
                } else {
                    length + padding
                });
            }
        }
        Ok(SizedBox {
            dimensions,
            padding,
            margins,
            offsets,
        })
    }

    fn absolute_box(
        &self,
        id: usize,
        sized: SizedBox,
        area: BorderBox,
        empty: bool,
    ) -> Result<UsedBox, BoxLayoutError> {
        let mut border = area;
        for (axis, [start, end]) in [[3, 1], [0, 2]].into_iter().enumerate() {
            let [leading, trailing] = [start, end].map(|edge| sized.offsets[edge]);
            let [margin_start, margin_end] = [start, end].map(|edge| sized.margins[edge]);
            let margin = margin_start.unwrap_or(0.0);
            let opposite_margin = margin_end.unwrap_or(0.0);
            let space = area.size[axis];
            let padding = sized.padding[start] + sized.padding[end];
            let dimension = match (sized.dimensions[axis], leading, trailing) {
                (Some(size), _, _) => size,
                (None, Some(start), Some(end)) => {
                    (space - start - end - margin - opposite_margin).max(padding)
                }
                (None, _, _) if empty => padding,
                _ => return Err(invalid(id, "auto size requires both insets")),
            };
            let offset = match (leading, trailing) {
                (Some(start), Some(end)) => {
                    let free = space - start - end - dimension - margin - opposite_margin;
                    let auto_margin = match (margin_start, margin_end) {
                        (None, None) if axis == 0 => free.max(0.0) / 2.0,
                        (None, None) => free / 2.0,
                        (None, Some(_)) => free,
                        (Some(_), _) => 0.0,
                    };
                    start + margin + auto_margin
                }
                (Some(start), None) => start + margin,
                (None, Some(end)) => space - end - opposite_margin - dimension,
                (None, None) => {
                    return Err(invalid(
                        id,
                        "static-position insets are outside the bounded box layout",
                    ));
                }
            };
            border.origin[axis] += offset;
            border.size[axis] = dimension;
        }
        Ok(sized.with_border(border))
    }

    fn flex_items(&mut self, parent: usize) -> Result<(), BoxLayoutError> {
        let container = self.node(parent)?;
        let style = &container.style.used_box_style;
        style.require(parent, "flex-direction", "row", &["row"])?;
        style.require(parent, "flex-wrap", "nowrap", &["wrap"])?;
        style.require(parent, "flex-flow", "", &[""])?;
        style.require(
            parent,
            "align-content",
            "normal",
            &["flex-start", "flex-end"],
        )?;
        style.require(
            parent,
            "justify-content",
            "normal",
            &["normal", "start", "flex-start"],
        )?;
        style.require(
            parent,
            "align-items",
            "normal",
            &["normal", "stretch", "start", "flex-start"],
        )?;
        for property in ["gap", "row-gap", "column-gap"] {
            style.require(parent, property, "normal", &["normal", "0", "0px"])?;
        }
        if container
            .style
            .decoration_box_styles
            .values()
            .any(|pseudo| {
                pseudo.value("display", "inline") != "none"
                    && pseudo.value("position", "static") != "absolute"
            })
        {
            return Err(invalid(
                parent,
                "in-flow pseudo-elements are outside the bounded flex layout",
            ));
        }
        let area = self
            .element_box(parent)?
            .ok_or_else(|| invalid(parent, "hidden flex container"))?
            .content;
        let mut x = 0.0;
        let mut y = 0.0;
        let mut line_height: f64 = 0.0;
        let mut boxes = Vec::new();
        for &id in self.children.get(&parent).into_iter().flatten() {
            let node = self.node(id)?;
            let style = &node.style.used_box_style;
            let display = style.value("display", "inline");
            if display == "none" || style.value("position", "static") == "absolute" {
                continue;
            }
            if display == "contents" {
                return Err(invalid(
                    id,
                    "display: contents is outside the bounded flex layout",
                ));
            }
            style.require(id, "position", "static", &["static", "relative"])?;
            for (name, initial) in [
                ("align-self", "auto"),
                ("flex", ""),
                ("flex-basis", "auto"),
                ("flex-grow", "0"),
                ("flex-shrink", "1"),
                ("order", "0"),
            ] {
                style.require(id, name, initial, &[initial])?;
            }
            let sized = self.style_box(id, style, area, false)?;
            let [Some(width), Some(height)] = sized.dimensions else {
                return Err(invalid(
                    id,
                    "intrinsic flex sizing is outside the bounded box layout",
                ));
            };
            let [Some(top), Some(right), Some(bottom), Some(left)] = sized.margins else {
                return Err(invalid(
                    id,
                    "auto flex margins are outside the bounded box layout",
                ));
            };
            let outer_width = left + width + right;
            if outer_width > area.size[0] {
                return Err(invalid(id, "flex item would need to shrink"));
            }
            if x + outer_width > area.size[0] {
                y += line_height;
                x = 0.0;
                line_height = 0.0;
            }
            let offset = if style.value("position", "static") == "relative" {
                sized.relative_offset()
            } else {
                [0.0; 2]
            };
            boxes.push((
                id,
                sized.with_border(BorderBox {
                    origin: [
                        area.origin[0] + x + left + offset[0],
                        area.origin[1] + y + top + offset[1],
                    ],
                    size: [width, height],
                }),
            ));
            x += outer_width;
            line_height = line_height.max(top + height + bottom);
        }
        let free = if style.value("align-content", "normal") == "flex-end" {
            area.size[1] - y - line_height
        } else {
            0.0
        };
        for (id, mut used) in boxes {
            used.border.origin[1] += free;
            used.content.origin[1] += free;
            self.boxes.insert(id, Some(used));
        }
        Ok(())
    }
}

struct SizedBox {
    dimensions: [Option<f64>; 2],
    padding: [f64; 4],
    margins: [Option<f64>; 4],
    offsets: [Option<f64>; 4],
}

impl SizedBox {
    fn with_border(&self, border: BorderBox) -> UsedBox {
        let [top, right, bottom, left] = self.padding;
        UsedBox {
            border,
            content: BorderBox {
                origin: [border.origin[0] + left, border.origin[1] + top],
                size: [border.size[0] - left - right, border.size[1] - top - bottom],
            },
        }
    }

    fn relative_offset(&self) -> [f64; 2] {
        let [top, right, bottom, left] = self.offsets;
        [
            left.or_else(|| right.map(|value| -value)).unwrap_or(0.0),
            top.or_else(|| bottom.map(|value| -value)).unwrap_or(0.0),
        ]
    }
}

impl UsedBoxStyle {
    fn value<'a>(&'a self, name: &str, initial: &'a str) -> &'a str {
        self.properties.get(name).map_or(initial, String::as_str)
    }

    fn require(
        &self,
        id: usize,
        name: &str,
        initial: &str,
        allowed: &[&str],
    ) -> Result<(), BoxLayoutError> {
        let value = self.value(name, initial);
        if allowed.contains(&value) {
            Ok(())
        } else {
            Err(invalid(id, format!("unsupported {name}: {value}")))
        }
    }
}

fn read_length(
    id: usize,
    style: &UsedBoxStyle,
    lengths: LengthContext,
    name: &str,
    initial: &str,
    percent: f64,
) -> Result<Option<f64>, BoxLayoutError> {
    let value = style.value(name, initial);
    if value == "auto" {
        return Ok(None);
    }
    LengthContext {
        percent: Some(percent as f32),
        ..lengths
    }
    .pixels(value)
    .map(f64::from)
    .map(Some)
    .ok_or_else(|| invalid(id, format!("unsupported {name}: {value}")))
}

fn invalid(node: usize, detail: impl Into<String>) -> BoxLayoutError {
    BoxLayoutError {
        node,
        detail: detail.into(),
    }
}
