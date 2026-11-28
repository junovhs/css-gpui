//! Static-scene style resolver for the SEMUI v0.1 subset.
//!
//! Takes a [`SceneSourceGraph`] (parsed HTML + CSS) and produces a
//! [`ResolvedScene`]: every element node paired with its fully computed CSS
//! properties, after selector matching, cascade, shorthand expansion, and
//! inheritance.
//!
//! This is the output contract consumed by `EXT-01`.

mod aspect_ratio;
pub(crate) mod cascade;
pub(crate) mod inset;
pub(crate) mod interaction;
mod model;
mod rule_index;
pub(crate) mod selector;
pub(crate) mod values;

pub use crate::radio::{SnapshotRadioControls, SnapshotRadioLabel};
pub use model::{ComputedStyle, UsedBoxStyle};

use crate::ir::paint::InteractionPaint;
use crate::{CssRule, HtmlNode, HtmlNodeKind, SceneSourceGraph, radio::RadioMarkup};
use cascade::{apply_inheritance, apply_with_lengths, split_important};
use interaction::{
    Interaction, StateSelector, StateValueLoss, has_element_children, paint_difference,
    state_value_loss, supports_state_property,
};
use rule_index::RuleIndex;
use selector::{CheckedSnapshot, Specificity};
use std::collections::{BTreeMap, BTreeSet};
use values::{LengthContext, Variables, cascade_variables, substitute_variables};

/// A single HTML element node paired with its fully resolved style.
#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub node: HtmlNode,
    pub style: ComputedStyle,
}

/// The full resolved scene: all element nodes in document order, each with
/// its computed style after cascade and inheritance.
///
/// Text nodes are excluded; their typography comes from their parent element.
#[derive(Debug, Clone)]
pub struct ResolvedScene {
    pub scene_id: String,
    pub nodes: Vec<ResolvedNode>,
}

/// Errors that can occur during style resolution.
#[derive(Debug)]
pub enum ResolverError {
    /// The scene has no HTML nodes at all (empty source graph).
    EmptyGraph,
    /// A supplied node ID does not identify a supported radio in this graph.
    UnsupportedRadio(usize),
    /// A supplied node ID does not identify a supported radio or checkbox.
    UnsupportedCheckedControl(usize),
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyGraph => write!(f, "scene source graph contains no HTML nodes"),
            Self::UnsupportedRadio(id) => write!(f, "node {id} is not a supported radio"),
            Self::UnsupportedCheckedControl(id) => {
                write!(f, "node {id} is not a supported radio or checkbox")
            }
        }
    }
}

impl std::error::Error for ResolverError {}

/// Resolve all element nodes in `graph` to their computed styles.
///
/// Processing order:
/// 1. For each element node, find all matching CSS rules.
/// 2. Sort by (specificity ASC, rule index ASC) — later/higher wins on apply.
/// 3. Apply declarations in order; shorthands are expanded in place.
/// 4. Propagate inheritable properties from parent elements.
pub fn resolve_scene(graph: &SceneSourceGraph) -> Result<ResolvedScene, ResolverError> {
    resolve(graph, None, None, &mut BTreeSet::new())
}

/// Resolve small viewport and viewport units using logical CSS pixels.
pub fn resolve_scene_at_viewport(
    graph: &SceneSourceGraph,
    width: f32,
    height: f32,
) -> Result<ResolvedScene, ResolverError> {
    resolve(graph, Some((width, height)), None, &mut BTreeSet::new())
}

/// Resolve a fixed radio-selection snapshot for a source graph at this viewport.
///
/// Each supplied node ID selects that supported radio and replaces its group's
/// authored selection. IDs are applied in order. Groups without an override
/// retain their authored selection. An empty slice resolves the authored state.
/// This snapshot supports relational selectors and uses supported radios as
/// the source of `:checked`. The returned styles can be inspected or used by a
/// caller that owns input and rendering for successive snapshots.
pub fn resolve_scene_with_radio_selection(
    graph: &SceneSourceGraph,
    width: f32,
    height: f32,
    selected_radios: &[usize],
) -> Result<ResolvedScene, ResolverError> {
    resolve(
        graph,
        Some((width, height)),
        Some(SnapshotRequest::RadioSelection(selected_radios)),
        &mut BTreeSet::new(),
    )
}

/// Read explicit radio labels within an authored subtree in a resting snapshot.
/// The caller supplies the scene resolved with the same radio overrides. This
/// reports cascade visibility and pointer eligibility, without viewport
/// hit-testing, CSS motion, or running authored event handlers.
pub fn radio_snapshot_controls<'a>(
    graph: &'a SceneSourceGraph,
    scene: &'a ResolvedScene,
    root: usize,
    overrides: &[usize],
) -> Result<SnapshotRadioControls<'a>, ResolverError> {
    crate::radio::snapshot_controls(graph, scene, root, overrides, false)
        .map_err(ResolverError::UnsupportedRadio)
}

/// Resolve a fixed snapshot by applying input activations in order. A radio
/// selects its group member and a checkbox toggles independently. The sequence
/// starts from the authored checked state each time.
pub fn resolve_scene_with_checked_activations(
    graph: &SceneSourceGraph,
    width: f32,
    height: f32,
    activations: &[usize],
) -> Result<ResolvedScene, ResolverError> {
    resolve(
        graph,
        Some((width, height)),
        Some(SnapshotRequest::CheckedActivations(activations)),
        &mut BTreeSet::new(),
    )
}

/// Read explicit radio and checkbox labels and visible direct checkboxes in a
/// subtree. Pass the same activation sequence used to resolve the snapshot.
pub fn checked_snapshot_controls<'a>(
    graph: &'a SceneSourceGraph,
    scene: &'a ResolvedScene,
    root: usize,
    activations: &[usize],
) -> Result<SnapshotRadioControls<'a>, ResolverError> {
    crate::radio::snapshot_controls(graph, scene, root, activations, true)
        .map_err(ResolverError::UnsupportedCheckedControl)
}

enum SnapshotRequest<'a> {
    RadioSelection(&'a [usize]),
    CheckedActivations(&'a [usize]),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StateDeclaration {
    pub rule: usize,
    pub selector: usize,
    pub declaration: usize,
}

pub(crate) fn state_value_losses(
    graph: &SceneSourceGraph,
) -> BTreeSet<(StateDeclaration, StateValueLoss)> {
    let mut losses = BTreeSet::new();
    let _ = resolve(graph, None, None, &mut losses);
    losses
}

fn resolve(
    graph: &SceneSourceGraph,
    viewport: Option<(f32, f32)>,
    snapshot: Option<SnapshotRequest<'_>>,
    losses: &mut BTreeSet<(StateDeclaration, StateValueLoss)>,
) -> Result<ResolvedScene, ResolverError> {
    if graph.html_nodes.is_empty() {
        return Err(ResolverError::EmptyGraph);
    }

    // Build a resolved-style slot for every node, keyed by node index (= node.id).
    // We process in vector order, which is document order (parent before children).
    let n = graph.html_nodes.len();
    let mut styles: Vec<Option<(ComputedStyle, Variables)>> = (0..n).map(|_| None).collect();
    let initial_variables = Variables::new();
    let mut root_font_size = 16.0;

    let radios = RadioMarkup::with_checkboxes(
        &graph.html_nodes,
        matches!(snapshot, Some(SnapshotRequest::CheckedActivations(_))),
    );
    let snapshot = snapshot
        .map(|request| {
            let selection = match request {
                SnapshotRequest::RadioSelection(ids) => radios
                    .selection(ids)
                    .map_err(ResolverError::UnsupportedRadio)?,
                SnapshotRequest::CheckedActivations(ids) => radios
                    .selection(ids)
                    .map_err(ResolverError::UnsupportedCheckedControl)?,
            };
            Ok::<_, ResolverError>(CheckedSnapshot::new(
                &graph.html_nodes,
                selection.into_checked(),
            ))
        })
        .transpose()?;
    let snapshot = snapshot.as_ref();
    let rules = RuleIndex::parse(graph, None, snapshot.is_some());
    let state_rules: Vec<_> = graph
        .css_rules
        .iter()
        .filter_map(|rule| {
            let selectors: Vec<_> = rule
                .selectors
                .iter()
                .enumerate()
                .filter_map(|(index, text)| {
                    let selector = StateSelector::parse(text)?;
                    (snapshot.is_none() || selector.state != Interaction::Checked)
                        .then_some((index, selector))
                })
                .collect();
            (!selectors.is_empty()).then_some((rule, selectors))
        })
        .collect();
    let decoration_rules = [
        (
            "before",
            RuleIndex::parse(graph, Some("before"), snapshot.is_some()),
        ),
        (
            "after",
            RuleIndex::parse(graph, Some("after"), snapshot.is_some()),
        ),
    ];

    for node in &graph.html_nodes {
        if node.kind != HtmlNodeKind::Element {
            continue;
        }

        let parent = node.parent_id.and_then(|id| styles.get(id)?.as_ref());
        let lengths = LengthContext {
            em: parent
                .and_then(|(style, _)| style.font_size)
                .unwrap_or(16.0),
            rem: root_font_size,
            viewport,
            percent: None,
        };
        let inherited = parent.map_or(&initial_variables, |(_, variables)| variables);
        let matches = rules.matches(node, &graph.html_nodes, snapshot);
        let radio = radios.radios.contains_key(&node.id);
        let (mut style, variables) =
            cascade(matches.clone(), inherited, lengths, radio, |_, _, _| true);
        if let Some((parent_style, _)) = parent {
            apply_inheritance(&mut style, parent_style);
        } else if node.name.as_deref() == Some("html") {
            root_font_size = style.font_size.unwrap_or(16.0);
        }
        if !state_rules.is_empty() {
            let active: Vec<_> = state_rules
                .iter()
                .filter_map(|(rule, selectors)| {
                    let selectors: Vec<_> = selectors
                        .iter()
                        .filter(|(_, selector)| {
                            selector.matches(node, &graph.html_nodes)
                                && selector.supports_element(node, &graph.html_nodes, &radios)
                        })
                        .collect();
                    (!selectors.is_empty()).then_some((*rule, selectors))
                })
                .collect();
            if !active.is_empty() {
                let has_children = has_element_children(node, &graph.html_nodes);
                let mut paint_at = |state: Interaction| {
                    let mut matches = matches.clone();
                    matches.extend(active.iter().filter_map(|(rule, selectors)| {
                        let (index, specificity) = selectors
                            .iter()
                            .filter(|(_, selector)| state.allows(selector.state))
                            .map(|(index, selector)| (*index, selector.specificity()))
                            .max_by_key(|(_, specificity)| *specificity)?;
                        Some((specificity, *rule, Some(index)))
                    }));
                    let (mut current, _) = cascade(
                        matches,
                        inherited,
                        lengths,
                        radio,
                        |source, property, value| {
                            if property == "color" && has_children {
                                return false;
                            }
                            if let Some(reason) = state_value_loss(&style, property, value) {
                                losses.insert((source, reason));
                                return false;
                            }
                            true
                        },
                    );
                    if let Some((parent_style, _)) = parent {
                        apply_inheritance(&mut current, parent_style);
                    }
                    paint_difference(&style, &current)
                };
                if radio {
                    style.checked_paint = paint_at(Interaction::Checked);
                } else {
                    let interaction = InteractionPaint {
                        resting_text_color: None,
                        hover: paint_at(Interaction::Hover),
                        focus: paint_at(Interaction::Focus),
                        hover_focus: paint_at(Interaction::HoverFocus),
                    };
                    if interaction != InteractionPaint::default() {
                        style.interaction = Some(interaction);
                    }
                }
            }
        }
        for (name, rules) in &decoration_rules {
            let matches = rules.matches(node, &graph.html_nodes, snapshot);
            if matches.is_empty() {
                continue;
            }
            let (decoration, _) = cascade(
                matches,
                &variables,
                LengthContext {
                    em: style.font_size.unwrap_or(lengths.em),
                    ..lengths
                },
                false,
                |_, _, _| true,
            );
            let content = decoration.paint_effects.get("content").map(String::as_str);
            if matches!(content, None | Some("none" | "normal")) {
                continue;
            }
            style
                .decoration_projection_properties
                .insert((*name).to_owned(), decoration.projection_properties);
            style
                .decoration_box_styles
                .insert((*name).to_owned(), decoration.used_box_style);
            let mut placement = decoration.paint_placement;
            for corner in ["border-top-right-radius", "border-bottom-left-radius"] {
                if !placement.contains_key(corner)
                    && let Some(inherited) = style.paint_placement.get(corner)
                {
                    placement.insert(corner.to_owned(), inherited.clone());
                }
            }
            style.decorations.push(crate::ir::paint::Decoration {
                pseudo: (*name).to_owned(),
                placement,
                effects: decoration.paint_effects,
                border_width: if decoration.border_width > 0.0 {
                    decoration.border_width
                } else {
                    style.border_width
                },
                radius: if decoration.border_radius > 0.0 {
                    decoration.border_radius
                } else {
                    style.border_radius
                },
            });
        }
        styles[node.id] = Some((style, variables));
    }

    aspect_ratio::validate_layout(&graph.html_nodes, &mut styles, viewport, root_font_size);

    let nodes = graph
        .html_nodes
        .iter()
        .filter(|n| n.kind == HtmlNodeKind::Element)
        .filter_map(|n| {
            styles[n.id].as_ref().map(|(style, _)| ResolvedNode {
                node: n.clone(),
                style: style.clone(),
            })
        })
        .collect();

    Ok(ResolvedScene {
        scene_id: graph.scene_id.clone(),
        nodes,
    })
}

type MatchedRule<'a> = (Specificity, &'a CssRule, Option<usize>);

/// Apply matched rules in cascade order and return the style before inheritance.
fn cascade(
    mut matches: Vec<MatchedRule<'_>>,
    inherited: &Variables,
    mut lengths: LengthContext,
    radio: bool,
    mut validate_state_value: impl FnMut(StateDeclaration, &str, Option<&str>) -> bool,
) -> (ComputedStyle, Variables) {
    matches.sort_by_key(|&(specificity, rule, _)| (specificity, rule.id));
    let mut declarations: Vec<_> = matches
        .iter()
        .flat_map(|&(_, rule, selector)| {
            rule.declarations
                .iter()
                .enumerate()
                .filter_map(move |(index, declaration)| {
                    let source = selector.map(|selector| StateDeclaration {
                        rule: rule.id,
                        selector,
                        declaration: index,
                    });
                    let property = declaration.property.as_str();
                    if source.is_some() && !supports_state_property(property) {
                        return None;
                    }
                    let (value, important) = split_important(&declaration.value);
                    Some((property, value, important, source, (rule.id, index)))
                })
        })
        .collect();
    declarations.sort_by_key(|(_, _, important, _, _)| *important);

    let custom: BTreeMap<_, _> = declarations
        .iter()
        .filter(|(name, _, _, _, _)| name.starts_with("--"))
        .map(|(name, value, _, _, _)| (*name, *value))
        .collect();
    let variables = cascade_variables(inherited, &custom);
    let declarations: Vec<_> = declarations
        .into_iter()
        .filter(|(name, _, _, _, _)| !name.starts_with("--"))
        .filter_map(|(name, value, _, source, origin)| {
            let value = substitute_variables(value, &mut |name| variables.get(name).cloned());
            if let Some(source) = source
                && !validate_state_value(source, name, value.as_deref())
            {
                return None;
            }
            Some((name, value, origin))
        })
        .collect();

    // em lengths use the element's final font size. On font-size itself, em
    // and percentages refer to the parent's computed font size.
    let mut style = ComputedStyle::default();
    if radio {
        // Radios keep Chromium's user-agent margin even with appearance disabled.
        style.margin_top = 3.0;
        style.margin_right = 3.0;
        style.margin_left = 5.0;
    }
    if let Some((_, value, _)) = declarations
        .iter()
        .rev()
        .find(|(name, _, _)| *name == "font-size")
    {
        style.font_size = value.as_deref().and_then(|value| {
            LengthContext {
                percent: Some(lengths.em),
                ..lengths
            }
            .pixels(value)
        });
        lengths.em = style.font_size.unwrap_or(lengths.em);
    }
    let has_physical_inset = declarations
        .iter()
        .any(|(property, _, _)| inset::is_new_property(property));
    for (property, value, origin) in declarations
        .into_iter()
        .filter(|(name, _, _)| *name != "font-size")
    {
        if property == "aspect-ratio" {
            aspect_ratio::apply(&mut style, origin, value.as_deref());
        }
        let mut value = value.as_deref();
        if has_physical_inset {
            value = inset::track(&mut style, property, value, lengths, origin);
        }
        if let Some(value) = value {
            apply_with_lengths(&mut style, property, value, lengths);
        }
    }
    if has_physical_inset {
        inset::finish_cascade(&mut style, lengths);
    }
    style.used_box_style.font_size = Some(lengths.em);
    (style, variables)
}

#[cfg(test)]
mod tests;
