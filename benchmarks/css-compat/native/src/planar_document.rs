use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::Path,
    sync::Arc,
};

use css_gpui::{HtmlNode, SceneSourceGraph, planar, resolver};

use super::{CheckedState, Label};
use crate::upload_frame;
use gpui::RenderImage;

pub(super) struct Document {
    graph: Arc<SceneSourceGraph>,
    viewport: [u32; 2],
    root: usize,
    scope: usize,
    authored_controls: bool,
    ids: BTreeMap<String, usize>,
}

pub(super) struct State {
    pub preview: planar::Preview,
    pub controls: CheckedState,
    pub labels: Vec<Label>,
}

// A radio retains only its group's last activation. Each checkbox records
// whether its current value differs from its own authored default.
#[derive(Clone, Default)]
pub(super) struct Activations {
    radios: BTreeMap<String, usize>,
    checkboxes: BTreeSet<usize>,
}

impl Document {
    pub(super) fn new(
        viewport: [u32; 2],
        html: &Path,
        css: &Path,
        root: &str,
        scope: &str,
        authored_controls: bool,
    ) -> Result<Self, Box<dyn Error>> {
        if viewport.iter().any(|length| !(1..=4096).contains(length)) {
            return Err("planar preview requires each viewport dimension in 1..=4096".into());
        }
        let [width, height] = viewport.map(|length| length as f32);
        let graph = SceneSourceGraph::from_strings_at_viewport_size(
            "document",
            &fs::read_to_string(html)?,
            &fs::read_to_string(css)?,
            width,
            height,
        )?;
        let root = unique_node(&graph, root)?;
        let scope = unique_node(&graph, scope)?;
        let ids = graph
            .html_nodes
            .iter()
            .filter_map(|node| element_id(node).map(|id| (id.to_owned(), node.id)))
            .collect();
        Ok(Self {
            graph: Arc::new(graph),
            viewport,
            root,
            scope,
            authored_controls,
            ids,
        })
    }

    pub(super) fn activate(&self, activations: &mut Activations, label: &Label) {
        // Every offered target came from the graph's unique input IDs.
        let id = self.ids[&label.target];
        if label.checkbox {
            if !activations.checkboxes.remove(&id) {
                activations.checkboxes.insert(id);
            }
        } else {
            activations.radios.insert(label.group.clone(), id);
        }
    }

    pub(super) fn resolve(&self, activations: &Activations) -> Result<State, String> {
        let activations: Vec<_> = activations
            .radios
            .values()
            .chain(&activations.checkboxes)
            .copied()
            .collect();
        let [width, height] = self.viewport.map(|length| length as f32);
        let scene = resolver::resolve_scene_with_checked_activations(
            &self.graph,
            width,
            height,
            &activations,
        )
        .map_err(|error| error.to_string())?;
        let controls =
            resolver::checked_snapshot_controls(&self.graph, &scene, self.scope, &activations)
                .map_err(|error| error.to_string())?;
        let mut movement = if self.authored_controls {
            planar::viewport_controls(
                &self.graph,
                &scene,
                self.root,
                self.viewport,
                &controls.labels,
            )
        } else {
            Default::default()
        };
        let labels = controls
            .labels
            .into_iter()
            .map(|label| Label {
                caption: caption(&label),
                group: label.group.to_owned(),
                target: label.target.to_owned(),
                class_name: label.class_name.to_owned(),
                checkbox: label.checkbox,
                movement: movement.remove(&label.node_id),
            })
            .collect();
        let controls = CheckedState {
            selection: controls
                .selection
                .into_iter()
                .map(|(group, id)| (group.to_owned(), id.to_owned()))
                .collect(),
            checked: controls.checked.into_iter().map(str::to_owned).collect(),
        };
        Ok(State {
            preview: planar::Preview::new(self.graph.clone(), scene, self.root, self.viewport),
            controls,
            labels,
        })
    }

    pub(super) fn image(
        &self,
        state: &State,
        pose: Option<&planar::Pose>,
    ) -> Result<Arc<RenderImage>, String> {
        let raster = if pose.is_some() && self.viewport[0] > 720 {
            self.viewport
                .map(|length| (length * 640 / self.viewport[0]).max(1))
        } else {
            self.viewport
        };
        let frame = state.preview.render(raster, pose)?;
        Ok(upload_frame(raster[0], raster[1], frame.rgba)?)
    }
}

fn element_id(node: &HtmlNode) -> Option<&str> {
    node.attributes
        .iter()
        .find_map(|(name, value)| name.eq_ignore_ascii_case("id").then_some(value.as_str()))
}

fn unique_node(graph: &SceneSourceGraph, selector: &str) -> Result<usize, String> {
    let mut matches = graph.html_nodes.iter().filter(|node| {
        if let Some(id) = selector.strip_prefix('#') {
            element_id(node) == Some(id)
        } else {
            node.name.as_deref() == Some(selector)
        }
    });
    let node = matches
        .next()
        .ok_or_else(|| format!("{selector:?} does not match an element"))?;
    if matches.next().is_some() {
        return Err(format!("{selector:?} must match exactly one element"));
    }
    Ok(node.id)
}

fn caption(label: &resolver::SnapshotRadioLabel<'_>) -> String {
    if !label.text.is_empty() {
        return label.text.clone();
    }
    if let Some(content) = label.generated_content
        && let Some(text) = content
            .strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
    {
        if let Some(hex) = text.strip_prefix('\\').map(str::trim_ascii_end)
            && hex.len() <= 6
            && let Ok(value) = u32::from_str_radix(hex, 16)
            && let Some(character) = char::from_u32(value)
        {
            return character.to_string();
        }
        if !text.is_empty() && !text.contains(['\\', '"']) {
            return text.to_owned();
        }
    }
    label.target.to_owned()
}
