use crate::{HtmlNode, HtmlNodeKind};

/// CSS specificity in ID, class/attribute/pseudo-class, type order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Specificity(pub u32, pub u32, pub u32);

pub(crate) struct Selector<'a> {
    head: Compound<'a>,
    tail: Vec<(Combinator, Compound<'a>)>,
    specificity: Specificity,
}

struct Compound<'a> {
    tag: Option<&'a str>,
    conditions: Vec<Condition<'a>>,
}

#[derive(Clone, Copy)]
enum Combinator {
    Descendant,
    Child,
}

enum Condition<'a> {
    Id(&'a str),
    Class(&'a str),
    Attribute(&'a str, Option<&'a str>),
    Root,
    FirstChild,
    LastChild,
    NthOfType(usize),
    AnyOf(Vec<Selector<'a>>),
    Not(Vec<Selector<'a>>),
}

impl<'a> Selector<'a> {
    pub(crate) fn parse(mut input: &'a str) -> Option<Self> {
        let selector = Self::parse_prefix(&mut input)?;
        input.is_empty().then_some(selector)
    }

    fn parse_prefix(input: &mut &'a str) -> Option<Self> {
        let mut specificity = Specificity::default();
        *input = input.trim_ascii_start();
        let head = Compound::parse(input, &mut specificity)?;
        let mut tail = Vec::new();
        loop {
            let before = input.len();
            *input = input.trim_ascii_start();
            let combinator = match input.as_bytes().first() {
                None | Some(b',' | b')') => break,
                Some(b'>') => {
                    *input = input[1..].trim_ascii_start();
                    Combinator::Child
                }
                _ if input.len() < before => Combinator::Descendant,
                _ => return None,
            };
            tail.push((combinator, Compound::parse(input, &mut specificity)?));
        }
        Some(Self {
            head,
            tail,
            specificity,
        })
    }

    pub(super) fn is_compound(&self) -> bool {
        self.tail.is_empty()
    }

    pub(super) fn specificity(&self) -> Specificity {
        self.specificity
    }

    pub(super) fn matches(&self, node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
        self.matches_tail(node, nodes, self.tail.len())
    }

    fn matches_tail(&self, node: &HtmlNode, nodes: &[HtmlNode], len: usize) -> bool {
        let Some((combinator, compound)) = self.tail[..len].last() else {
            return self.head.matches(node, nodes);
        };
        if !compound.matches(node, nodes) {
            return false;
        }

        let mut parent = node.parent_id.and_then(|id| nodes.get(id));
        while let Some(ancestor) = parent {
            if self.matches_tail(ancestor, nodes, len - 1) {
                return true;
            }
            if matches!(combinator, Combinator::Child) {
                break;
            }
            parent = ancestor.parent_id.and_then(|id| nodes.get(id));
        }
        false
    }
}

impl<'a> Compound<'a> {
    fn parse(input: &mut &'a str, specificity: &mut Specificity) -> Option<Self> {
        let before = input.len();
        let tag = if take(input, "*") { None } else { ident(input) };
        specificity.2 += u32::from(tag.is_some());
        let mut conditions = Vec::new();
        loop {
            let condition = match input.as_bytes().first() {
                Some(b'#') => {
                    *input = &input[1..];
                    specificity.0 += 1;
                    Condition::Id(ident(input)?)
                }
                Some(b'.') => {
                    *input = &input[1..];
                    specificity.1 += 1;
                    Condition::Class(ident(input)?)
                }
                Some(b'[') => {
                    *input = input[1..].trim_ascii_start();
                    specificity.1 += 1;
                    let name = ident(input)?;
                    *input = input.trim_ascii_start();
                    let value = if take(input, "=") {
                        *input = input.trim_ascii_start();
                        Some(attribute_value(input)?)
                    } else {
                        None
                    };
                    *input = input.trim_ascii_start();
                    if !take(input, "]") {
                        return None;
                    }
                    Condition::Attribute(name, value)
                }
                Some(b':') => {
                    *input = &input[1..];
                    pseudo_class(input, specificity)?
                }
                _ => break,
            };
            conditions.push(condition);
        }
        (input.len() != before).then_some(Self { tag, conditions })
    }

    fn matches(&self, node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
        node.kind == HtmlNodeKind::Element
            && self.tag.is_none_or(|expected| {
                node.name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(expected))
            })
            && self
                .conditions
                .iter()
                .all(|condition| condition.matches(node, nodes))
    }
}

impl Condition<'_> {
    fn matches(&self, node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
        let attribute = |name| {
            node.attributes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        let siblings = || {
            nodes.iter().filter(|other| {
                other.kind == HtmlNodeKind::Element && other.parent_id == node.parent_id
            })
        };
        match self {
            Self::Id(id) => attribute("id") == Some(id),
            Self::Class(class) => attribute("class")
                .is_some_and(|value| value.split_ascii_whitespace().any(|item| item == *class)),
            Self::Attribute(name, value) => attribute(name)
                .is_some_and(|actual| value.is_none_or(|expected| actual == expected)),
            Self::Root => node
                .parent_id
                .and_then(|id| nodes.get(id))
                .is_some_and(|parent| parent.kind == HtmlNodeKind::Document),
            Self::FirstChild => siblings().next().is_some_and(|other| other.id == node.id),
            Self::LastChild => siblings()
                .next_back()
                .is_some_and(|other| other.id == node.id),
            Self::NthOfType(index) => index.checked_sub(1).is_some_and(|index| {
                siblings()
                    .filter(|other| other.name == node.name)
                    .nth(index)
                    .is_some_and(|other| other.id == node.id)
            }),
            Self::AnyOf(selectors) => selectors
                .iter()
                .any(|selector| selector.matches(node, nodes)),
            Self::Not(selectors) => !selectors
                .iter()
                .any(|selector| selector.matches(node, nodes)),
        }
    }
}

fn pseudo_class<'a>(input: &mut &'a str, specificity: &mut Specificity) -> Option<Condition<'a>> {
    let name = ident(input)?;
    let condition = match name {
        "root" => Condition::Root,
        "first-child" => Condition::FirstChild,
        "last-child" => Condition::LastChild,
        "nth-of-type" if take(input, "(") => {
            let (value, rest) = input.split_once(')')?;
            *input = rest;
            Condition::NthOfType(value.trim_ascii().parse().ok()?)
        }
        "is" | "where" | "not" if take(input, "(") => {
            let mut selectors = Vec::new();
            loop {
                selectors.push(Selector::parse_prefix(input)?);
                if !take(input, ",") {
                    break;
                }
            }
            if !take(input, ")") {
                return None;
            }
            if name != "where" {
                let max = selectors.iter().map(Selector::specificity).max()?;
                specificity.0 += max.0;
                specificity.1 += max.1;
                specificity.2 += max.2;
            }
            return Some(if name == "not" {
                Condition::Not(selectors)
            } else {
                Condition::AnyOf(selectors)
            });
        }
        _ => return None,
    };
    specificity.1 += 1;
    Some(condition)
}

fn attribute_value<'a>(input: &mut &'a str) -> Option<&'a str> {
    if let Some(quote @ ('\'' | '"')) = input.chars().next() {
        let (value, rest) = input[1..].split_once(quote)?;
        *input = rest;
        return (!value.contains('\\')).then_some(value);
    }
    ident(input)
}

fn ident<'a>(input: &mut &'a str) -> Option<&'a str> {
    if input.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let end = input
        .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .unwrap_or(input.len());
    let (name, rest) = input.split_at(end);
    *input = rest;
    (!name.is_empty()).then_some(name)
}

fn take(input: &mut &str, prefix: &str) -> bool {
    if let Some(rest) = input.strip_prefix(prefix) {
        *input = rest;
        true
    } else {
        false
    }
}

#[cfg(test)]
pub(super) fn specificity(selector: &str) -> Specificity {
    Selector::parse(selector).unwrap().specificity()
}

#[cfg(test)]
pub(super) fn selector_matches(selector: &str, node: &HtmlNode) -> bool {
    Selector::parse(selector).is_some_and(|selector| selector.matches(node, &[]))
}
