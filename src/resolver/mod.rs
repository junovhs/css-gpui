//! Static-scene style resolver for the SEMUI v0.1 subset.
//!
//! Takes a [`SceneSourceGraph`] (parsed HTML + CSS) and produces a
//! [`ResolvedScene`]: every element node paired with its fully computed CSS
//! properties, after selector matching, cascade, shorthand expansion, and
//! inheritance.
//!
//! This is the output contract consumed by `EXT-01`.

mod cascade;
mod model;
pub(crate) mod selector;
mod values;

pub use model::ComputedStyle;

use crate::{HtmlNode, HtmlNodeKind, SceneSourceGraph};
use cascade::{apply_inheritance, apply_with_lengths};
use selector::Selector;
use std::collections::BTreeMap;
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
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyGraph => write!(f, "scene source graph contains no HTML nodes"),
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
    resolve(graph, None)
}

/// Resolve small viewport and viewport units using logical CSS pixels.
pub fn resolve_scene_at_viewport(
    graph: &SceneSourceGraph,
    width: f32,
    height: f32,
) -> Result<ResolvedScene, ResolverError> {
    resolve(graph, Some((width, height)))
}

fn resolve(
    graph: &SceneSourceGraph,
    viewport: Option<(f32, f32)>,
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

    let rules: Vec<_> = graph
        .css_rules
        .iter()
        .map(|rule| {
            let selectors: Vec<_> = rule
                .selectors
                .iter()
                .filter_map(|text| Selector::parse(text))
                .collect();
            (rule, selectors)
        })
        .collect();

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
        let (mut style, variables) = cascade_for_node(
            node,
            &graph.html_nodes,
            &rules,
            parent.map_or(&initial_variables, |(_, variables)| variables),
            lengths,
        );
        if let Some((parent_style, _)) = parent {
            apply_inheritance(&mut style, parent_style);
        } else if node.name.as_deref() == Some("html") {
            root_font_size = style.font_size.unwrap_or(16.0);
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

/// Apply cascade for a single element node and return its raw computed style
/// (before inheritance).
fn cascade_for_node(
    node: &HtmlNode,
    nodes: &[HtmlNode],
    rules: &[(&crate::CssRule, Vec<Selector<'_>>)],
    inherited: &Variables,
    mut lengths: LengthContext,
) -> (ComputedStyle, Variables) {
    let mut matches: Vec<_> = rules
        .iter()
        .filter_map(|(rule, selectors)| {
            let specificity = selectors
                .iter()
                .filter(|selector| selector.matches(node, nodes))
                .map(Selector::specificity)
                .max()?;
            Some((specificity, rule.id, rule))
        })
        .collect();

    // Sort ascending — declarations are applied in this order so the last one wins.
    matches.sort_by_key(|&(spec, id, _)| (spec, id));

    let mut declarations: Vec<_> = matches
        .iter()
        .flat_map(|(_, _, rule)| &rule.declarations)
        .map(|declaration| {
            let (value, important) = declaration
                .value
                .strip_suffix("!important")
                .map_or((declaration.value.as_str(), false), |value| {
                    (value.trim_end(), true)
                });
            (declaration.property.as_str(), value, important)
        })
        .collect();
    declarations.sort_by_key(|(_, _, important)| *important);

    let custom: BTreeMap<_, _> = declarations
        .iter()
        .filter(|(name, _, _)| name.starts_with("--"))
        .map(|(name, value, _)| (*name, *value))
        .collect();
    let variables = cascade_variables(inherited, &custom);
    let declarations: Vec<_> = declarations
        .into_iter()
        .filter(|(name, _, _)| !name.starts_with("--"))
        .map(|(name, value, _)| {
            let value = substitute_variables(value, &mut |name| variables.get(name).cloned());
            (name, value)
        })
        .collect();

    // em lengths use the element's final font size. On font-size itself, em
    // and percentages refer to the parent's computed font size.
    let mut style = ComputedStyle::default();
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
