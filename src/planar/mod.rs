//! Render one authored, continuous CSS 3D context.
//!
//! This preview uses the bounded CSS box layout and source cascade. The caller
//! supplies a context root and the resolved checked state. Diagnostics describe
//! the visible planes and paint that the bounded renderer has to approximate or
//! omit. The standard strict GPUI emitter is independent of this preview.

mod material;
mod overlay;
mod raster;
mod transition;

pub use transition::{Pose, Transition};

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use serde::Serialize;

use crate::{
    HtmlNodeKind, SceneSourceGraph,
    layout::UsedBoxLayout,
    projection::{self, Layer, Point},
    resolver::{ResolvedNode, ResolvedScene, values::LengthContext},
};

use material::{ConfiguredFacePaint, FacePaint, Material, PaintCache};
use raster::Face;

#[derive(Debug, Serialize)]
pub struct Loss {
    pub path: String,
    pub property: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct Plane {
    pub path: String,
    pub polygon: Vec<Point>,
}

pub struct Frame {
    /// Opaque row-major RGBA pixels at the requested raster dimensions.
    pub rgba: Vec<u8>,
    pub planes: Vec<Plane>,
    pub losses: Vec<Loss>,
}

/// Reuse plane paint while rendering one resolved scene at multiple poses.
pub struct Preview {
    graph: Arc<SceneSourceGraph>,
    scene: ResolvedScene,
    root: usize,
    viewport: [u32; 2],
    paint: PaintCache,
}

impl Preview {
    pub fn new(
        graph: Arc<SceneSourceGraph>,
        scene: ResolvedScene,
        root: usize,
        viewport: [u32; 2],
    ) -> Self {
        Self {
            graph,
            scene,
            root,
            viewport,
            paint: PaintCache::default(),
        }
    }

    /// Derive a supported transition between two cascades of this document.
    pub fn transition_from(&self, before: &Self, pose: Option<&Pose>) -> Option<Transition> {
        if !Arc::ptr_eq(&self.graph, &before.graph)
            || self.root != before.root
            || self.viewport != before.viewport
        {
            return None;
        }
        Transition::between(&before.scene, &self.scene, self.root, pose)
    }

    /// Render with layout still measured in CSS pixels when the raster is smaller.
    pub fn render(&self, raster: [u32; 2], pose: Option<&Pose>) -> Result<Frame, String> {
        compile(
            &self.graph,
            &self.scene,
            self.root,
            self.viewport,
            raster,
            pose,
            &self.paint,
        )
    }
}

/// Preview the subtree of a 3D context in document coordinates at scroll zero.
/// The viewport clips the entire context. Pixels outside its planes show the
/// document canvas and the solid backing of a viewport-covering body. Flat
/// viewport overlays stack over the settled 3D context and its backing.
pub fn preview(
    graph: &SceneSourceGraph,
    scene: &ResolvedScene,
    root: usize,
    viewport: [u32; 2],
) -> Result<Frame, String> {
    compile(
        graph,
        scene,
        root,
        viewport,
        viewport,
        None,
        &PaintCache::default(),
    )
}

fn compile(
    graph: &SceneSourceGraph,
    scene: &ResolvedScene,
    root: usize,
    viewport: [u32; 2],
    raster: [u32; 2],
    pose: Option<&Pose>,
    paint: &PaintCache,
) -> Result<Frame, String> {
    if viewport.iter().any(|&length| !(1..=4096).contains(&length)) {
        return Err("planar preview requires each viewport dimension in 1..=4096".into());
    }
    if raster
        .into_iter()
        .zip(viewport)
        .any(|(raster, css)| raster == 0 || raster > css)
    {
        return Err("planar raster dimensions must be positive and fit the CSS viewport".into());
    }
    let viewport_css = viewport.map(f64::from);
    let mut children: HashMap<_, Vec<_>> = HashMap::new();
    let mut nodes = HashMap::new();
    let mut document = None;
    for node in &scene.nodes {
        nodes.insert(node.node.id, node);
        if let Some(parent) = node.node.parent_id {
            children.entry(parent).or_default().push(node);
        }
        if node.node.name.as_deref() == Some("html") {
            document = Some(node);
        }
    }
    let mut text = HashSet::new();
    for node in &graph.html_nodes {
        if node.kind == HtmlNodeKind::Text
            && let Some(parent) = node.parent_id
            && node
                .text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
        {
            text.insert(parent);
        }
    }
    let root = *nodes.get(&root).ok_or("unknown planar context root")?;
    let overrides = pose
        .into_iter()
        .flat_map(|pose| {
            scene.nodes.iter().filter_map(|node| {
                let id = node.node.id;
                let transform = pose.transform(id)?;
                let mut css = node.style.projection_properties.clone();
                css.insert("transform".to_owned(), transform.to_owned());
                Some((id, css))
            })
        })
        .collect();
    let layout = UsedBoxLayout::new(scene, viewport_css);
    let mut compiler = Compiler {
        root_font_size: layout.root_font_size(),
        viewport: viewport_css,
        layout,
        children,
        text,
        overrides,
        paint,
        faces: Vec::new(),
        planes: Vec::new(),
        losses: Vec::new(),
    };
    let mut background = [255; 4];
    if let Some(document) = document {
        let style = &document.style;
        let path = &document.node.dom_path;
        let mut loss =
            |property: &str, value: &str| record_loss(&mut compiler.losses, path, property, value);
        let paint = Material::from_css(
            &style.projection_properties,
            &style.paint_effects,
            None,
            &mut loss,
        );
        if paint.has_images() {
            loss(
                "background-image",
                "document canvas requires a solid background",
            );
        }
        background = raster::over(
            paint.color,
            background.map(|value| f32::from(value) / 255.0),
        )
        .map(raster::byte);
    }
    let mut color = None;
    let mut visibility = None;
    let mut ancestors = Vec::new();
    let mut parent = root.node.parent_id.and_then(|id| nodes.get(&id));
    while let Some(node) = parent {
        ancestors.push(*node);
        let css = &node.style.projection_properties;
        if node.node.name.as_deref() == Some("body") {
            background = compiler.body_backdrop(node, background);
        }
        color = color.or_else(|| css.get("color").and_then(|value| crate::paint::rgba(value)));
        visibility = visibility.or_else(|| css.get("visibility").map(String::as_str));
        for property in ["transform", "perspective", "filter", "opacity", "clip-path"] {
            let value = css
                .get(property)
                .or_else(|| node.style.paint_effects.get(property));
            if let Some(value) = value.filter(|value| !matches!(value.as_str(), "none" | "1")) {
                record_loss(&mut compiler.losses, &node.node.dom_path, property, value);
            }
        }
        parent = node.node.parent_id.and_then(|id| nodes.get(&id));
    }
    let overlays = overlay::compile(
        root,
        &ancestors,
        &mut compiler.layout,
        viewport_css,
        &mut compiler.losses,
    );
    compiler.visit(
        root,
        &mut Vec::new(),
        color,
        visibility.unwrap_or("visible"),
    );
    Ok(Frame {
        rgba: raster::render(&compiler.faces, &overlays, background, viewport, raster),
        planes: compiler.planes,
        losses: compiler.losses,
    })
}

struct Compiler<'a> {
    root_font_size: f32,
    viewport: Point,
    layout: UsedBoxLayout<'a>,
    children: HashMap<usize, Vec<&'a ResolvedNode>>,
    text: HashSet<usize>,
    overrides: HashMap<usize, BTreeMap<String, String>>,
    paint: &'a PaintCache,
    faces: Vec<Face>,
    planes: Vec<Plane>,
    losses: Vec<Loss>,
}

impl<'a> Compiler<'a> {
    fn body_backdrop(&mut self, node: &ResolvedNode, background: [u8; 4]) -> [u8; 4] {
        let style = &node.style;
        let css = &style.projection_properties;
        let effects = &style.paint_effects;
        let Some(color) = css
            .get("background-color")
            .and_then(|value| crate::paint::rgba(value))
            .filter(|color| color[3] != 0)
        else {
            return background;
        };
        if style.display == "none"
            || css_value(css, "visibility", "visible") != "visible"
            || css_value(effects, "opacity", "1") == "0"
        {
            return background;
        }
        let simple = css_value(css, "transform", "none") == "none"
            && css_value(css, "clip-path", "none") == "none"
            && css_value(effects, "filter", "none") == "none"
            && css_value(effects, "mask", "none") == "none"
            && css_value(effects, "opacity", "1") == "1"
            && css_value(effects, "mix-blend-mode", "normal") == "normal"
            && style.border_radius == 0.0;
        let covers_viewport = self
            .layout
            .border_box(node.node.id, None)
            .ok()
            .flatten()
            .is_some_and(|bounds| overlay::covers_viewport(bounds, self.viewport));
        if !simple || !covers_viewport {
            record_loss(
                &mut self.losses,
                &node.node.dom_path,
                "background-color",
                "body backing requires a simple viewport-covering box",
            );
            return background;
        }
        if let Some(image) = effects
            .get("background-image")
            .filter(|value| *value != "none")
        {
            record_loss(
                &mut self.losses,
                &node.node.dom_path,
                "background-image",
                image,
            );
        }
        raster::over(
            color.map(|byte| f32::from(byte) / 255.0),
            background.map(|byte| f32::from(byte) / 255.0),
        )
        .map(raster::byte)
    }

    fn visit(
        &mut self,
        node: &'a ResolvedNode,
        ancestors: &mut Vec<&'a ResolvedNode>,
        inherited_color: Option<[u8; 4]>,
        inherited_visibility: &str,
    ) {
        let style = &node.style;
        if style.display == "none" {
            return;
        }

        let css = &style.projection_properties;
        let color = css
            .get("color")
            .and_then(|value| crate::paint::rgba(value))
            .or(inherited_color);
        let visibility = css
            .get("visibility")
            .map_or(inherited_visibility, String::as_str);
        if css_value(&style.paint_effects, "opacity", "1") == "0" {
            return;
        }
        let children = self
            .children
            .get(&node.node.id)
            .cloned()
            .unwrap_or_default();
        if !children.is_empty() || !style.decorations.is_empty() {
            let grouping = [
                (
                    "opacity",
                    css_value(&style.paint_effects, "opacity", "1"),
                    "1",
                ),
                (
                    "filter",
                    css_value(&style.paint_effects, "filter", "none"),
                    "none",
                ),
                ("overflow", css_value(css, "overflow", "visible"), "visible"),
                (
                    "overflow-x",
                    css_value(css, "overflow-x", "visible"),
                    "visible",
                ),
                (
                    "overflow-y",
                    css_value(css, "overflow-y", "visible"),
                    "visible",
                ),
            ];
            for (property, value, initial) in grouping {
                if value != initial {
                    record_loss(
                        &mut self.losses,
                        &node.node.dom_path,
                        property,
                        "3D group effects",
                    );
                }
            }
        }
        ancestors.push(node);
        if visibility == "visible" {
            self.face(
                ancestors,
                None,
                css,
                &style.paint_effects,
                color,
                self.text.contains(&node.node.id),
            );
        }
        for pseudo in style
            .decorations
            .iter()
            .filter(|pseudo| pseudo.pseudo == "before")
        {
            self.decoration(ancestors, pseudo, color, visibility);
        }
        for child in children {
            self.visit(child, ancestors, color, visibility);
        }
        for pseudo in style
            .decorations
            .iter()
            .filter(|pseudo| pseudo.pseudo == "after")
        {
            self.decoration(ancestors, pseudo, color, visibility);
        }
        ancestors.pop();
    }

    fn decoration(
        &mut self,
        chain: &[&ResolvedNode],
        pseudo: &crate::ir::paint::Decoration,
        color: Option<[u8; 4]>,
        visibility: &str,
    ) {
        let node = chain.last().unwrap();
        let css = &node.style.decoration_projection_properties[&pseudo.pseudo];
        if css_value(css, "visibility", visibility) != "visible" {
            return;
        }
        let color = css
            .get("color")
            .and_then(|value| crate::paint::rgba(value))
            .or(color);
        let content = css_value(&pseudo.effects, "content", "");
        let text = !matches!(content, "" | "\"\"" | "''");
        self.face(
            chain,
            Some(&pseudo.pseudo),
            css,
            &pseudo.effects,
            color,
            text,
        );
    }

    fn face(
        &mut self,
        nodes: &[&ResolvedNode],
        pseudo: Option<&str>,
        css: &BTreeMap<String, String>,
        effects: &BTreeMap<String, String>,
        text_color: Option<[u8; 4]>,
        text: bool,
    ) {
        let node = nodes.last().unwrap();
        let paint = self.paint.face(node.node.id, pseudo, || {
            let mut losses = Vec::new();
            let mut loss =
                |property: &str, value: &str| losses.push((property.to_owned(), value.to_owned()));
            let material = Material::from_css(css, effects, text_color, &mut loss);
            if text && text_color.is_none_or(|color| color[3] > 0) {
                loss("content", "visible text on a 3D plane");
            }
            FacePaint {
                material,
                losses,
                configured: Default::default(),
            }
        });
        if paint.material.is_empty() && paint.losses.is_empty() {
            return;
        }
        let mut path = node.node.dom_path.clone();
        if let Some(pseudo) = pseudo {
            path.push_str("::");
            path.push_str(pseudo);
        }
        let mut layers = Vec::with_capacity(nodes.len() + usize::from(pseudo.is_some()));
        let mut add = |node: &ResolvedNode, pseudo, css, font_size| {
            if let Some(bounds) = self
                .layout
                .border_box(node.node.id, pseudo)
                .map_err(|error| error.to_string())?
            {
                layers.push(Layer {
                    origin: bounds.origin,
                    size: bounds.size,
                    font_size,
                    css,
                });
                Ok::<_, String>(true)
            } else {
                Ok(false)
            }
        };
        let layers_result = (|| {
            for node in nodes {
                if !add(
                    node,
                    None,
                    self.overrides
                        .get(&node.node.id)
                        .unwrap_or(&node.style.projection_properties),
                    node.style.used_box_style.font_size(),
                )? {
                    return Ok(false);
                }
            }
            if let Some(pseudo) = pseudo {
                return add(
                    node,
                    Some(pseudo),
                    css,
                    node.style.decoration_box_styles[pseudo].font_size(),
                );
            }
            Ok(true)
        })();
        match layers_result {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                record_loss(&mut self.losses, &path, "layout", &error);
                return;
            }
        }
        let size = layers.last().unwrap().size;
        if size.iter().any(|&length| length <= 0.0) {
            return;
        }
        let matrix =
            match projection::accumulated_matrix(&layers, self.viewport, self.root_font_size) {
                Ok(matrix) => matrix,
                Err(error) => {
                    record_loss(&mut self.losses, &path, error.property, &error.to_string());
                    return;
                }
            };
        let Some(sampling) = matrix.sampling_plane() else {
            return;
        };
        if css_value(css, "backface-visibility", "visible") == "hidden" && sampling.back_facing {
            return;
        }
        let polygon = matrix.project_box(size, self.viewport).viewport_polygon;
        if polygon.len() < 3 {
            return;
        }
        let configured = paint.configured.get_or_init(|| {
            let mut material = paint.material.clone();
            let mut losses = Vec::new();
            material.configure_background(
                css,
                effects,
                size.map(|value| value as f32),
                LengthContext {
                    em: layers.last().unwrap().font_size,
                    rem: self.root_font_size,
                    viewport: Some((self.viewport[0] as f32, self.viewport[1] as f32)),
                    percent: None,
                },
                &mut |property, value| losses.push((property.to_owned(), value.to_owned())),
            );
            ConfiguredFacePaint {
                material: Arc::new(material),
                losses,
            }
        });
        for (property, value) in paint.losses.iter().chain(&configured.losses) {
            record_loss(&mut self.losses, &path, property, value);
        }
        if !configured.material.is_empty() {
            self.faces.push(Face::new(
                sampling,
                configured.material.clone(),
                size,
                &polygon,
            ));
            self.planes.push(Plane { path, polygon });
        }
    }
}

fn css_value<'a>(css: &'a BTreeMap<String, String>, property: &str, initial: &'a str) -> &'a str {
    css.get(property).map_or(initial, String::as_str)
}

fn record_loss(losses: &mut Vec<Loss>, path: &str, property: &str, value: &str) {
    losses.push(Loss {
        path: path.into(),
        property: property.into(),
        value: value.into(),
    });
}
