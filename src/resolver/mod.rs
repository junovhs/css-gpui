//! Static-scene style resolver for the SEMUI v0.1 subset.
//!
//! Takes a [`SceneSourceGraph`] (parsed HTML + CSS) and produces a
//! [`ResolvedScene`]: every element node paired with its fully computed CSS
//! properties, after selector matching, cascade, shorthand expansion, and
//! inheritance.
//!
//! This is the output contract consumed by `EXT-01`.

pub(crate) mod cascade;
pub(crate) mod interaction;
mod model;
pub(crate) mod selector;
pub(crate) mod values;

pub use model::ComputedStyle;

use crate::ir::paint::InteractionPaint;
use crate::{CssRule, HtmlNode, HtmlNodeKind, SceneSourceGraph, radio::RadioMarkup};
use cascade::{apply_inheritance, apply_with_lengths, split_important};
use interaction::{
    Interaction, StateSelector, StateValueLoss, has_element_children, paint_difference,
    state_value_loss, supports_state_property,
};
use selector::{CheckedSnapshot, Selector, Specificity};
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
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyGraph => write!(f, "scene source graph contains no HTML nodes"),
            Self::UnsupportedRadio(id) => write!(f, "node {id} is not a supported radio"),
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
        Some(selected_radios),
        &mut BTreeSet::new(),
    )
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
    selected_radios: Option<&[usize]>,
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

    let radios = RadioMarkup::new(&graph.html_nodes);
    let snapshot = if let Some(overrides) = selected_radios {
        let mut selection = radios.initial.clone();
        for &id in overrides {
            let &name = radios
                .radios
                .get(&id)
                .ok_or(ResolverError::UnsupportedRadio(id))?;
            selection.insert(name, id);
        }
        Some(CheckedSnapshot::new(
            &graph.html_nodes,
            selection.into_values().collect(),
        ))
    } else {
        None
    };
    let snapshot = snapshot.as_ref();
    let rules = parse_rules(graph, None, snapshot.is_some());
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
            parse_rules(graph, Some("before"), snapshot.is_some()),
        ),
        (
            "after",
            parse_rules(graph, Some("after"), snapshot.is_some()),
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
        let matches = matching_rules(node, &graph.html_nodes, &rules, snapshot);
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
            let matches = matching_rules(node, &graph.html_nodes, rules, snapshot);
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

fn parse_rules<'a>(
    graph: &'a SceneSourceGraph,
    pseudo_element: Option<&str>,
    snapshot: bool,
) -> Vec<(&'a crate::CssRule, Vec<Selector<'a>>)> {
    graph
        .css_rules
        .iter()
        .filter_map(|rule| {
            let selectors: Vec<_> = rule
                .selectors
                .iter()
                .filter_map(|text| {
                    let origin = match pseudo_element {
                        Some(pseudo_element) => {
                            let origin = text.strip_suffix(pseudo_element)?;
                            origin
                                .strip_suffix("::")
                                .or_else(|| snapshot.then(|| origin.strip_suffix(":")).flatten())?
                        }
                        None => text,
                    };
                    if snapshot {
                        Selector::parse_snapshot(origin)
                    } else {
                        Selector::parse(origin)
                    }
                })
                .collect();
            (!selectors.is_empty()).then_some((rule, selectors))
        })
        .collect()
}

type MatchedRule<'a> = (Specificity, &'a CssRule, Option<usize>);

fn matching_rules<'a>(
    node: &HtmlNode,
    nodes: &[HtmlNode],
    rules: &[(&'a CssRule, Vec<Selector<'_>>)],
    snapshot: Option<&CheckedSnapshot>,
) -> Vec<MatchedRule<'a>> {
    rules
        .iter()
        .filter_map(|(rule, selectors)| {
            let specificity = selectors
                .iter()
                .filter(|selector| selector.matches_in(node, nodes, snapshot))
                .map(Selector::specificity)
                .max()?;
            Some((specificity, *rule, None))
        })
        .collect()
}

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
                    Some((property, value, important, source))
                })
        })
        .collect();
    declarations.sort_by_key(|(_, _, important, _)| *important);

    let custom: BTreeMap<_, _> = declarations
        .iter()
        .filter(|(name, _, _, _)| name.starts_with("--"))
        .map(|(name, value, _, _)| (*name, *value))
        .collect();
    let variables = cascade_variables(inherited, &custom);
    let declarations: Vec<_> = declarations
        .into_iter()
        .filter(|(name, _, _, _)| !name.starts_with("--"))
        .filter_map(|(name, value, _, source)| {
            let value = substitute_variables(value, &mut |name| variables.get(name).cloned());
            if let Some(source) = source
                && !validate_state_value(source, name, value.as_deref())
            {
                return None;
            }
            Some((name, value))
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
    if let Some((_, value)) = declarations
        .iter()
        .rev()
        .find(|(name, _)| *name == "font-size")
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
    for (property, value) in declarations
        .into_iter()
        .filter(|(name, _)| *name != "font-size")
    {
        if let Some(value) = value {
            apply_with_lengths(&mut style, property, &value, lengths);
        }
    }
    (style, variables)
}

#[cfg(test)]
mod tests;
