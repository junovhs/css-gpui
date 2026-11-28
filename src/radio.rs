//! Bounded radio and label associations in the parsed HTML document.

use std::collections::{BTreeMap, HashMap};

use crate::extractor::{INVISIBLE_TAGS, input_attribute, input_type};
use crate::resolver::interaction::unsupported_focus_order;
use crate::{HtmlNode, HtmlNodeKind};

#[derive(Default)]
pub(crate) struct RadioMarkup<'a> {
    pub radios: HashMap<usize, &'a str>,
    pub labels: HashMap<usize, usize>,
    snapshot_labels: HashMap<usize, usize>,
    pub initial: HashMap<&'a str, usize>,
    pub losses: HashMap<usize, &'static str>,
}

pub(crate) fn is_radio(node: &HtmlNode) -> bool {
    is_tag(node, "input") && input_type(node).is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
}

pub(crate) fn is_checkable(node: &HtmlNode) -> bool {
    is_radio(node)
        || (is_tag(node, "input")
            && input_type(node).is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox")))
        || is_tag(node, "option")
}

impl<'a> RadioMarkup<'a> {
    pub fn new(nodes: &'a [HtmlNode]) -> Self {
        let mut markup = Self::default();
        let mut radios = nodes.iter().filter(|node| is_radio(node)).peekable();
        if radios.peek().is_none() {
            return markup;
        }

        let mut by_id = HashMap::<_, Vec<_>>::new();
        for node in nodes {
            if let Some(id) = input_attribute(node, "id") {
                by_id.entry(id).or_default().push(node);
            }
        }

        for radio in radios {
            let name = input_attribute(radio, "name");
            let issue = if let Some(reason) = unsupported_focus_order(radio, nodes) {
                Some(reason)
            } else if name.is_none_or(|name| name.is_empty() || !literal_attribute(name)) {
                Some("requires a nonempty literal name")
            } else if let Some(reason) = unsupported_radio_attribute(radio) {
                Some(reason)
            } else if input_attribute(radio, "id").is_some_and(|id| {
                id.is_empty()
                    || id.chars().any(char::is_whitespace)
                    || !literal_attribute(id)
                    || by_id[id].len() != 1
            }) {
                Some("requires a unique literal id when id is present")
            } else if input_attribute(radio, "value").is_some_and(|value| !literal_attribute(value))
            {
                Some("requires a literal value")
            } else if ancestors(radio, nodes).any(|ancestor| is_tag(ancestor, "form")) {
                Some("form ownership is unsupported")
            } else if ancestors(radio, nodes).any(|ancestor| {
                is_tag(ancestor, "label") && input_attribute(ancestor, "for").is_none()
            }) {
                Some("requires explicit label for association")
            } else if unavailable(radio, nodes) {
                Some("hidden radio")
            } else {
                None
            };

            if let Some(issue) = issue {
                markup.losses.insert(radio.id, issue);
            } else if let Some(name) = name {
                markup.radios.insert(radio.id, name);
                // HTML keeps the last authored checked option in document order.
                if input_attribute(radio, "checked").is_some() {
                    markup.initial.insert(name, radio.id);
                }
            }
        }

        for label in nodes.iter().filter(|node| is_tag(node, "label")) {
            let Some(target) = input_attribute(label, "for") else {
                continue;
            };
            let matches = by_id.get(target).map(Vec::as_slice).unwrap_or_default();
            if !matches.iter().any(|node| is_radio(node)) {
                if matches.is_empty() {
                    markup.losses.insert(label.id, "unresolved label for");
                }
                continue;
            }
            let supported = matches.len() == 1
                && markup.radios.contains_key(&matches[0].id)
                && !unavailable(label, nodes)
                && unsupported_focus_order(label, nodes).is_none()
                && input_attribute(label, "tabindex").is_none()
                && !ancestors(label, nodes).any(|ancestor| is_tag(ancestor, "label"))
                && !nodes.iter().any(|child| {
                    interactive_label_child(child, matches[0].id)
                        && ancestors(child, nodes).any(|ancestor| ancestor.id == label.id)
                });
            if supported {
                markup.snapshot_labels.insert(label.id, matches[0].id);
            }
            if supported && !has_event_handler(label) {
                markup.labels.insert(label.id, matches[0].id);
            } else {
                markup
                    .losses
                    .insert(label.id, "unsupported radio label association");
            }
        }
        markup
    }

    pub fn selection(&self, overrides: &[usize]) -> Result<HashMap<&'a str, usize>, usize> {
        let mut selection = self.initial.clone();
        for &id in overrides {
            let &name = self.radios.get(&id).ok_or(id)?;
            selection.insert(name, id);
        }
        Ok(selection)
    }
}

/// A scoped explicit label as resolved in a resting radio snapshot.
#[derive(Debug, serde::Serialize)]
pub struct SnapshotRadioLabel<'a> {
    pub node_id: usize,
    pub group: &'a str,
    pub target: &'a str,
    pub class_name: &'a str,
    pub text: String,
    pub generated_content: Option<&'a str>,
    /// Authored handlers are outside CSS snapshot activation.
    pub inline_handler: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct SnapshotRadioControls<'a> {
    pub selection: BTreeMap<&'a str, &'a str>,
    pub labels: Vec<SnapshotRadioLabel<'a>>,
}

pub(crate) fn snapshot_controls<'a>(
    graph: &'a crate::SceneSourceGraph,
    scene: &'a crate::resolver::ResolvedScene,
    root: usize,
    overrides: &[usize],
) -> Result<SnapshotRadioControls<'a>, usize> {
    let nodes = &graph.html_nodes;
    let markup = RadioMarkup::new(nodes);
    let selection = markup
        .selection(overrides)?
        .into_iter()
        .filter_map(|(group, id)| Some((group, input_attribute(&nodes[id], "id")?)))
        .collect();
    let styles: HashMap<_, _> = scene
        .nodes
        .iter()
        .map(|node| (node.node.id, &node.style))
        .collect();
    let labels = nodes
        .iter()
        .filter_map(|label| {
            let &radio = markup.snapshot_labels.get(&label.id)?;
            let branch: Vec<_> = std::iter::once(label)
                .chain(ancestors(label, nodes))
                .collect();
            if !branch.iter().any(|node| node.id == root)
                || branch.iter().any(|node| {
                    styles
                        .get(&node.id)
                        .is_some_and(|style| style.display == "none")
                })
            {
                return None;
            }
            let inherited = |property| {
                branch.iter().find_map(|node| {
                    styles
                        .get(&node.id)?
                        .projection_properties
                        .get(property)
                        .map(String::as_str)
                        .filter(|value| !matches!(*value, "inherit" | "unset"))
                })
            };
            if matches!(inherited("visibility"), Some("hidden" | "collapse"))
                || inherited("pointer-events") == Some("none")
            {
                return None;
            }
            let style = styles.get(&label.id)?;
            let generated_content = style.decorations.iter().find_map(|decoration| {
                (decoration.pseudo == "after" || decoration.pseudo == "before")
                    .then(|| decoration.effects.get("content").map(String::as_str))
                    .flatten()
                    .filter(|content| !matches!(*content, "none" | "normal" | "\"\""))
            });
            let text = nodes
                .iter()
                .skip(label.id + 1)
                .take_while(|child| ancestors(child, nodes).any(|node| node.id == label.id))
                .filter_map(|child| {
                    let text = child.text.as_deref()?;
                    ancestors(child, nodes)
                        .take_while(|node| node.id != label.id)
                        .all(|node| {
                            styles
                                .get(&node.id)
                                .is_none_or(|style| style.display != "none")
                        })
                        .then_some(text.trim())
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Some(SnapshotRadioLabel {
                node_id: label.id,
                group: markup.radios[&radio],
                target: input_attribute(&nodes[radio], "id")?,
                class_name: input_attribute(label, "class").unwrap_or_default(),
                text,
                generated_content,
                inline_handler: has_event_handler(label),
            })
        })
        .collect();
    Ok(SnapshotRadioControls { selection, labels })
}

fn is_tag(node: &HtmlNode, tag: &str) -> bool {
    node.name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case(tag))
}

fn ancestors<'a>(node: &'a HtmlNode, nodes: &'a [HtmlNode]) -> impl Iterator<Item = &'a HtmlNode> {
    std::iter::successors(node.parent_id.and_then(|id| nodes.get(id)), |node| {
        node.parent_id.and_then(|id| nodes.get(id))
    })
}

fn unavailable(node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
    std::iter::once(node)
        .chain(ancestors(node, nodes))
        .any(|node| {
            node.name.as_deref().is_some_and(|tag| {
                INVISIBLE_TAGS.contains(&tag) || matches!(tag, "svg" | "template")
            }) || input_attribute(node, "hidden").is_some()
        })
}

fn interactive_label_child(node: &HtmlNode, target: usize) -> bool {
    node.kind == HtmlNodeKind::Element
        && node.id != target
        && ([
            "input", "button", "label", "select", "textarea", "audio", "video", "details",
            "summary",
        ]
        .iter()
        .any(|tag| is_tag(node, tag))
            || (is_tag(node, "a") && input_attribute(node, "href").is_some())
            || input_attribute(node, "contenteditable").is_some()
            || input_attribute(node, "tabindex").is_some())
}

fn unsupported_radio_attribute(radio: &HtmlNode) -> Option<&'static str> {
    if input_attribute(radio, "form").is_some() {
        return Some("form ownership is unsupported");
    }
    [
        "required",
        "readonly",
        "hidden",
        "tabindex",
        "autofocus",
        "role",
        "contenteditable",
        "aria-checked",
    ]
    .into_iter()
    .find(|attr| input_attribute(radio, attr).is_some())
    .or_else(|| has_event_handler(radio).then_some("inline event handler"))
}

fn has_event_handler(node: &HtmlNode) -> bool {
    node.attributes.iter().any(|(name, _)| {
        name.get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("on"))
    })
}

fn literal_attribute(value: &str) -> bool {
    // The source graph retains the attribute spelling without HTML entity decoding.
    !value.contains(['&', '\r', '\0'])
}
