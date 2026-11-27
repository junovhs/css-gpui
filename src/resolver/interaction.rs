use crate::extractor::{INVISIBLE_TAGS, TRANSPARENT_TAGS, control_kind};
use crate::ir::paint::{Color, StatePaint};
use crate::{HtmlNode, HtmlNodeKind};

use super::ComputedStyle;
use super::selector::{Selector, Specificity};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Interaction {
    Hover,
    Focus,
    HoverFocus,
}

impl Interaction {
    pub(super) fn allows(self, requirement: Self) -> bool {
        self == requirement || self == Self::HoverFocus
    }
}

pub(crate) struct StateSelector<'a> {
    element: Selector<'a>,
    pub state: Interaction,
}

impl<'a> StateSelector<'a> {
    pub(crate) fn parse(text: &'a str) -> Option<Self> {
        let text = text.trim_ascii();
        let (element, state) = if let Some(element) = text
            .strip_suffix(":hover:focus")
            .or_else(|| text.strip_suffix(":focus:hover"))
        {
            (element, Interaction::HoverFocus)
        } else if let Some(element) = text.strip_suffix(":hover") {
            (element, Interaction::Hover)
        } else {
            (text.strip_suffix(":focus")?, Interaction::Focus)
        };
        // A combinator before a bare pseudo-class needs its own compound.
        if element.ends_with(|ch: char| ch.is_ascii_whitespace()) {
            return None;
        }
        let element = Selector::parse(if element.is_empty() { "*" } else { element })?;
        Some(Self { element, state })
    }

    pub(crate) fn matches(&self, node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
        self.element.matches(node, nodes)
    }

    pub(super) fn specificity(&self) -> Specificity {
        let mut specificity = self.element.specificity();
        specificity.1 += if self.state == Interaction::HoverFocus {
            2
        } else {
            1
        };
        specificity
    }

    pub(crate) fn supports_element(&self, node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
        let ancestors = std::iter::successors(Some(node), |node| {
            node.parent_id.and_then(|id| nodes.get(id))
        });
        let name = node.name.as_deref().unwrap_or_default();
        if TRANSPARENT_TAGS.contains(&name)
            || ancestors.clone().any(|node| {
                node.name
                    .as_deref()
                    .is_some_and(|tag| tag == "svg" || INVISIBLE_TAGS.contains(&tag))
            })
        {
            return false;
        }
        self.state == Interaction::Hover
            || (control_kind(node).is_some() && unsupported_focus_order(node, nodes).is_none())
    }
}

pub(crate) fn unsupported_focus_order(node: &HtmlNode, nodes: &[HtmlNode]) -> Option<&'static str> {
    let has_attribute = |node: &HtmlNode, name| {
        node.attributes
            .iter()
            .any(|(attr, _)| attr.eq_ignore_ascii_case(name))
    };
    if has_attribute(node, "disabled") {
        return Some("disabled");
    }
    if node.attributes.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("tabindex") && value.trim_ascii().parse::<i32>() != Ok(0)
    }) {
        return Some("nondefault tabindex");
    }

    std::iter::successors(Some(node), |node| {
        node.parent_id.and_then(|id| nodes.get(id))
    })
    .find_map(|ancestor| {
        if has_attribute(ancestor, "inert") {
            Some("inert")
        } else if ancestor.name.as_deref() == Some("fieldset")
            && has_attribute(ancestor, "disabled")
        {
            Some("inside a disabled fieldset")
        } else {
            None
        }
    })
}

pub(crate) fn supports_state_property(property: &str) -> bool {
    matches!(
        property,
        "background" | "background-color" | "border-color" | "color"
    )
}

pub(crate) fn has_element_children(node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
    nodes
        .iter()
        .any(|child| child.parent_id == Some(node.id) && child.kind == HtmlNodeKind::Element)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StateValueLoss {
    Color,
    LayeredBackground,
    BorderColor,
}

pub(super) fn state_value_loss(
    base: &ComputedStyle,
    property: &str,
    value: Option<&str>,
) -> Option<StateValueLoss> {
    if value.is_none_or(|value| !is_state_color(property, value)) {
        return Some(StateValueLoss::Color);
    }
    match property {
        "background" | "background-color"
            if base
                .paint_effects
                .keys()
                .any(|name| name.starts_with("background")) =>
        {
            Some(StateValueLoss::LayeredBackground)
        }
        "border-color" if base.border_color.is_none() => Some(StateValueLoss::BorderColor),
        _ => None,
    }
}

pub(crate) fn is_state_color(property: &str, value: &str) -> bool {
    (property == "background" && value == "none") || super::cascade::parse_color(value).is_some()
}

pub(super) fn paint_difference(base: &ComputedStyle, state: &ComputedStyle) -> StatePaint {
    let color_difference = |base: Option<&str>, current: Option<&str>, initial: &str| {
        let current = current.unwrap_or(initial);
        (base.unwrap_or(initial) != current).then(|| Color(current.to_owned()))
    };
    StatePaint {
        background_color: color_difference(
            base.background_color.as_deref(),
            state.background_color.as_deref(),
            "#00000000",
        ),
        border_color: if base.border_width > 0.0 {
            color_difference(
                base.border_color.as_deref(),
                state.border_color.as_deref(),
                "#000000",
            )
        } else {
            None
        },
        text_color: color_difference(base.color.as_deref(), state.color.as_deref(), "#000000"),
    }
}
