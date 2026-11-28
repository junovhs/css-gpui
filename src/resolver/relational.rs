//! Source validation for an ancestor's background keyed to one generated radio.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    HtmlNode, SceneSourceGraph,
    extractor::{TRANSPARENT_TAGS, input_attribute},
    radio::RadioMarkup,
};

use super::{
    cascade::{parse_color, split_important},
    selector::{CheckedSnapshot, Selector, Specificity},
};

#[derive(Clone, Copy)]
pub(super) struct Rule {
    pub id: usize,
    selector: usize,
    pub radio: usize,
    pub specificity: Specificity,
}

impl Rule {
    pub fn at(self, declaration: Option<usize>, reason: Reason) -> Loss {
        Loss {
            rule: self.id,
            selector: self.selector,
            declaration,
            reason,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reason {
    SelectorList,
    MissingRadio,
    MissingAncestor,
    UnsupportedBox,
    InlineStyle,
    Property,
    Color,
    CompetingRadio,
    CompetingInteraction,
    RestingBackground,
    InheritedBackground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Loss {
    pub rule: usize,
    pub selector: usize,
    pub declaration: Option<usize>,
    pub reason: Reason,
}

#[derive(Default)]
pub(super) struct Rules {
    pub by_node: BTreeMap<usize, Vec<Rule>>,
    pub losses: BTreeSet<Loss>,
}

pub(crate) fn has_shape(graph: &SceneSourceGraph) -> bool {
    graph
        .css_rules
        .iter()
        .flat_map(|rule| &rule.selectors)
        .any(|text| supports_shape(text))
}

pub(crate) fn supports_shape(text: &str) -> bool {
    parse(text).is_some()
}

fn parse(text: &str) -> Option<(Selector<'_>, &str)> {
    if !text.contains(":has(") {
        return None;
    }
    let selector = Selector::parse_snapshot(text)?;
    let radio = selector.checked_descendant_id()?;
    Some((selector, radio))
}

impl Rules {
    pub fn collect(graph: &SceneSourceGraph, radios: &RadioMarkup<'_>) -> Self {
        let mut result = Self::default();
        let nodes = &graph.html_nodes;
        let mut markup = None;

        for rule in &graph.css_rules {
            for (index, text) in rule.selectors.iter().enumerate() {
                let Some((selector, id)) = parse(text) else {
                    continue;
                };
                // Set every eligible bit for shape matching. Each selector is
                // independent and tests precisely the one ID referenced below.
                let (snapshot, by_id, inline_groups) = markup.get_or_insert_with(|| {
                    let snapshot =
                        CheckedSnapshot::new(nodes, radios.radios.keys().copied().collect());
                    let by_id: BTreeMap<_, _> = radios
                        .radios
                        .keys()
                        .filter_map(|&node| Some((input_attribute(&nodes[node], "id")?, node)))
                        .collect();
                    let inline_groups: BTreeSet<_> = radios
                        .radios
                        .iter()
                        .filter_map(|(&id, &name)| {
                            std::iter::once(&nodes[id])
                                .chain(ancestors(&nodes[id], nodes))
                                .any(|node| input_attribute(node, "style").is_some())
                                .then_some(name)
                        })
                        .collect();
                    (snapshot, by_id, inline_groups)
                });
                let mut report = |declaration, reason| {
                    result.losses.insert(Loss {
                        rule: rule.id,
                        selector: index,
                        declaration,
                        reason,
                    });
                };
                let mut valid = true;
                for (index, declaration) in rule.declarations.iter().enumerate() {
                    let (value, _) = split_important(&declaration.value);
                    let reason = if declaration.property != "background-color" {
                        Reason::Property
                    } else if !is_literal_color(value) {
                        Reason::Color
                    } else {
                        continue;
                    };
                    report(Some(index), reason);
                    valid = false;
                }
                if rule.selectors.len() != 1 {
                    report(None, Reason::SelectorList);
                    continue;
                }
                let Some(&radio) = by_id.get(id) else {
                    report(None, Reason::MissingRadio);
                    continue;
                };
                if inline_groups.contains(radios.radios[&radio]) {
                    report(None, Reason::InlineStyle);
                    continue;
                }
                let source = Rule {
                    id: rule.id,
                    selector: index,
                    radio,
                    specificity: selector.specificity(),
                };
                let mut matched = false;
                for node in ancestors(&nodes[radio], nodes)
                    .filter(|node| selector.matches_in(node, nodes, Some(snapshot)))
                {
                    matched = true;
                    let tag = node.name.as_deref().unwrap_or_default();
                    if TRANSPARENT_TAGS.contains(&tag)
                        || matches!(tag, "label" | "button" | "textarea" | "select" | "option")
                    {
                        report(None, Reason::UnsupportedBox);
                    } else if valid {
                        result.by_node.entry(node.id).or_default().push(source);
                    }
                }
                if !matched {
                    report(None, Reason::MissingAncestor);
                }
            }
        }

        result.by_node.retain(|_, sources| {
            if sources
                .iter()
                .any(|source| source.radio != sources[0].radio)
            {
                result.losses.extend(
                    sources
                        .iter()
                        .map(|source| source.at(None, Reason::CompetingRadio)),
                );
                return false;
            }
            true
        });
        result
    }
}

fn is_literal_color(value: &str) -> bool {
    if value.starts_with('#') {
        return parse_color(value).is_some();
    }
    value.eq_ignore_ascii_case("black")
        || value.eq_ignore_ascii_case("white")
        || value == "transparent"
}

pub(super) fn ancestors<'a>(
    node: &'a HtmlNode,
    nodes: &'a [HtmlNode],
) -> impl Iterator<Item = &'a HtmlNode> {
    std::iter::successors(node.parent_id.and_then(|id| nodes.get(id)), |node| {
        node.parent_id.and_then(|id| nodes.get(id))
    })
}
