//! Static analysis pass: report constructs that are silently dropped or
//! fall back to defaults during v0.1 normalization.
//!
//! Call [`analyze`] after building a [`SceneSourceGraph`] to receive a list
//! of [`Diagnostic`] items before (or alongside) the resolver and extractor.

#[cfg(test)]
mod tests;

use crate::extractor::{control_kind, input_type, unsupported_text_input_attribute};
use crate::ir::{FlexWrap, TextAlign};
use crate::radio::RadioMarkup;
use crate::resolver::{
    cascade::{split_important, supports_letter_spacing},
    interaction::{
        Interaction, StateSelector, StateValueLoss, has_element_children, is_state_color,
        supports_state_property, unsupported_focus_order,
    },
    selector::Selector,
    state_value_losses,
};
use crate::source_graph::SceneSourceGraph;
use crate::{CssRule, TraceSpan};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// The coarse category of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// A CSS property that the v0.1 resolver does not handle. The declaration
    /// is silently dropped during cascade.
    UnsupportedProperty,
    /// A CSS property is recognized but the specific value is not mapped. The
    /// property falls back to its CSS initial value.
    UnsupportedValue,
    /// A selector outside the resolver's supported subset.
    UnsupportedSelector,
    /// An at-rule that the source graph could not evaluate.
    UnsupportedAtRule,
}

/// A single item produced by [`analyze`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    /// Index into `SceneSourceGraph::css_rules`.
    pub rule_id: usize,
    /// The selector that triggered this diagnostic, if applicable.
    pub selector: Option<String>,
    /// The CSS property name, if applicable.
    pub property: Option<String>,
    /// The raw CSS value, if applicable.
    pub value: Option<String>,
}

/// Analyze `graph` for unsupported CSS constructs.
pub fn analyze(graph: &SceneSourceGraph) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut has_state_rules = false;
    let radios = RadioMarkup::new(&graph.html_nodes);
    for rule in &graph.css_rules {
        has_state_rules |= analyze_rule(rule, graph, &radios, &mut out);
    }
    if has_state_rules {
        for (source, loss) in state_value_losses(graph) {
            let rule = &graph.css_rules[source.rule];
            let declaration = &rule.declarations[source.declaration];
            let message = match loss {
                StateValueLoss::Color => "interaction paint requires a supported solid color",
                StateValueLoss::LayeredBackground => {
                    "interaction paint on a layered background is unsupported"
                }
                StateValueLoss::BorderColor => {
                    "changing border color requires an explicit resting border color"
                }
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::UnsupportedValue,
                message: format!("{}: {message}", declaration.property),
                rule_id: rule.id,
                selector: Some(rule.selectors[source.selector].clone()),
                property: Some(declaration.property.clone()),
                value: Some(declaration.value.clone()),
            });
        }
    }
    out
}

/// An HTML form control whose behavior cannot be represented by the extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormControlLoss {
    pub element: String,
    pub span: TraceSpan,
}

/// Report interactive controls that would otherwise become generic IR boxes.
pub fn form_control_losses(graph: &SceneSourceGraph) -> impl Iterator<Item = FormControlLoss> {
    let uses_focus = graph
        .css_rules
        .iter()
        .flat_map(|rule| &rule.selectors)
        .any(|selector| {
            StateSelector::parse(selector).is_some_and(|selector| {
                matches!(selector.state, Interaction::Focus | Interaction::HoverFocus)
            })
        });
    let radios = RadioMarkup::new(&graph.html_nodes);
    let missing_appearance: std::collections::HashSet<_> = if radios.radios.is_empty() {
        Default::default()
    } else {
        crate::resolver::resolve_scene(graph)
            .into_iter()
            .flat_map(|scene| scene.nodes)
            .filter(|node| radios.radios.contains_key(&node.node.id) && !node.style.appearance_none)
            .map(|node| node.node.id)
            .collect()
    };
    graph.html_nodes.iter().filter_map(move |node| {
        let tag = node.name.as_deref()?;
        let range = node.start_tag_range.as_ref()?;
        let element = if let Some(reason) = radios.losses.get(&node.id) {
            format!("<{tag}> ({reason})")
        } else if radios.radios.contains_key(&node.id) {
            if !missing_appearance.contains(&node.id) {
                return None;
            }
            format!("<{tag} type=radio> (requires appearance: none)")
        } else if control_kind(node).is_some() {
            if !uses_focus {
                return None;
            }
            let behavior = unsupported_focus_order(node, &graph.html_nodes)?;
            format!("<{tag} {behavior}>")
        } else {
            match tag {
                tag if tag.eq_ignore_ascii_case("input") => {
                    if let Some(attribute) = unsupported_text_input_attribute(node) {
                        format!("<input {attribute}>")
                    } else {
                        let kind = input_type(node)?;
                        if kind.eq_ignore_ascii_case("hidden") {
                            return None;
                        }
                        format!("<input type={kind:?}>")
                    }
                }
                tag if tag.eq_ignore_ascii_case("select")
                    || tag.eq_ignore_ascii_case("textarea") =>
                {
                    format!("<{tag}>")
                }
                _ => return None,
            }
        };
        Some(FormControlLoss {
            element,
            span: TraceSpan::from_offsets(&graph.html.contents, range.start, range.end),
        })
    })
}

// ---------------------------------------------------------------------------
// Per-rule analysis
// ---------------------------------------------------------------------------

fn analyze_rule(
    rule: &CssRule,
    graph: &SceneSourceGraph,
    radios: &RadioMarkup<'_>,
    out: &mut Vec<Diagnostic>,
) -> bool {
    let mut has_state = false;
    let mut has_other_selector = false;
    for selector in &rule.selectors {
        if let Some(state) = StateSelector::parse(selector) {
            has_state = true;
            analyze_state_selector(rule, selector, &state, &graph.html_nodes, radios, out);
            continue;
        }
        has_other_selector = true;
        let unsupported = if selector.starts_with('@') {
            Some((
                DiagnosticKind::UnsupportedAtRule,
                format!("at-rule '{selector}' was not evaluated"),
            ))
        } else if Selector::parse(selector).is_none() {
            Some((
                DiagnosticKind::UnsupportedSelector,
                format!("selector '{selector}' is outside the static selector subset"),
            ))
        } else {
            None
        };
        if let Some((kind, message)) = unsupported {
            out.push(Diagnostic {
                kind,
                message,
                rule_id: rule.id,
                selector: Some(selector.clone()),
                property: None,
                value: None,
            });
        }
    }

    if !has_other_selector {
        return has_state;
    }
    for decl in &rule.declarations {
        if !is_known_property(&decl.property) {
            out.push(Diagnostic {
                kind: DiagnosticKind::UnsupportedProperty,
                message: format!(
                    "property '{}' is not supported in v0.1 and will be dropped",
                    decl.property
                ),
                rule_id: rule.id,
                selector: rule.selectors.first().cloned(),
                property: Some(decl.property.clone()),
                value: Some(decl.value.clone()),
            });
        } else if let Some(msg) = unsupported_value_message(&decl.property, &decl.value) {
            out.push(Diagnostic {
                kind: DiagnosticKind::UnsupportedValue,
                message: msg,
                rule_id: rule.id,
                selector: rule.selectors.first().cloned(),
                property: Some(decl.property.clone()),
                value: Some(decl.value.clone()),
            });
        }
    }
    has_state
}

fn analyze_state_selector(
    rule: &CssRule,
    selector: &str,
    state: &StateSelector<'_>,
    nodes: &[crate::HtmlNode],
    radios: &RadioMarkup<'_>,
    out: &mut Vec<Diagnostic>,
) {
    let matching = nodes.iter().filter(|node| state.matches(node, nodes));
    if matching
        .clone()
        .any(|node| !state.supports_element(node, nodes, radios))
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::UnsupportedSelector,
            message: format!(
                "selector '{selector}' reaches elements outside the generated interaction subset"
            ),
            rule_id: rule.id,
            selector: Some(selector.to_owned()),
            property: None,
            value: None,
        });
    }

    let mut applicable = matching.filter(|node| state.supports_element(node, nodes, radios));
    let has_applicable_elements = applicable.clone().next().is_some();
    let has_element_children = applicable.any(|node| has_element_children(node, nodes));
    for declaration in &rule.declarations {
        let (value, _) = split_important(&declaration.value);
        let (kind, message) = if !supports_state_property(&declaration.property) {
            (
                DiagnosticKind::UnsupportedProperty,
                "this property cannot change with interactive state",
            )
        } else if declaration.property == "color" && has_element_children {
            (
                DiagnosticKind::UnsupportedProperty,
                "interaction text color requires an element without element children",
            )
        } else if !has_applicable_elements
            && !value.contains("var(")
            && !is_state_color(&declaration.property, value)
        {
            (
                DiagnosticKind::UnsupportedValue,
                "interaction paint requires a supported solid color",
            )
        } else {
            continue;
        };
        out.push(Diagnostic {
            kind,
            message: format!("{}: {message}", declaration.property),
            rule_id: rule.id,
            selector: Some(selector.to_owned()),
            property: Some(declaration.property.clone()),
            value: Some(declaration.value.clone()),
        });
    }
}

// ---------------------------------------------------------------------------
// Property / value analysis
// ---------------------------------------------------------------------------

const KNOWN_PROPERTIES: &[&str] = &[
    "margin",
    "padding",
    "background",
    "border",
    "appearance",
    "position",
    "display",
    "box-sizing",
    "top",
    "left",
    "width",
    "height",
    "min-width",
    "min-height",
    "grid-template",
    "grid-area",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-width",
    "border-color",
    "border-radius",
    "background-color",
    "flex-direction",
    "flex-wrap",
    "align-items",
    "justify-content",
    "align-self",
    "gap",
    "cursor",
    "color",
    "font-family",
    "font-size",
    "font-weight",
    "line-height",
    "text-align",
    "letter-spacing",
];

fn is_known_property(property: &str) -> bool {
    property.starts_with("--") || KNOWN_PROPERTIES.contains(&property)
}

/// Returns a diagnostic message when a known property has a value we cannot
/// map. These are the policy edges most likely to silently mis-render.
fn unsupported_value_message(property: &str, value: &str) -> Option<String> {
    let (value, _) = split_important(value);
    match property {
        "appearance" => match value {
            "none" | "inherit" => None,
            _ => Some(format!(
                "appearance: '{value}' is not supported in v0.1; only the explicit 'none' reset is accepted"
            )),
        },
        "position" => match value {
            "static" | "relative" | "absolute" | "fixed" | "inherit" => None,
            _ => Some(format!(
                "position: '{value}' is not supported in v0.1; treated as static"
            )),
        },
        "flex-wrap" if FlexWrap::from_css_keyword(value).is_none() => Some(format!(
            "flex-wrap: '{value}' is not supported; expected 'nowrap', 'wrap', or 'wrap-reverse'"
        )),
        "text-align"
            if TextAlign::from_css_keyword(value).is_none()
                && !value.eq_ignore_ascii_case("inherit")
                && !value.eq_ignore_ascii_case("unset") =>
        {
            Some(format!(
                "text-align: '{value}' is not supported; expected 'start', 'center', 'end', 'left', or 'right'"
            ))
        }
        "letter-spacing" if !supports_letter_spacing(value) => Some(format!(
            "letter-spacing: '{value}' is not supported; expected normal or a length in px, em, or rem"
        )),
        "justify-content" => match value {
            "center" | "space-between" | "normal" | "flex-start" => None,
            _ => Some(format!(
                "justify-content: '{value}' is not supported in v0.1; using default justification"
            )),
        },
        "display" => match value {
            "block" | "flex" | "inline-flex" | "grid" | "inherit" => None,
            _ => Some(format!(
                "display: '{value}' is not supported in v0.1; treated as block"
            )),
        },
        _ => None,
    }
}
