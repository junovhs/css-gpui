use std::collections::{HashMap, HashSet};

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
    AdjacentSibling,
    FollowingSibling,
}

#[derive(Clone, Copy)]
enum SelectorMode {
    Static,
    Snapshot,
    Relative,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) enum Subject<'a> {
    Id(&'a str),
    Class(&'a str),
    Tag(&'a str),
    NthOfType(&'a str, usize),
}

pub(super) struct CheckedSnapshot {
    checked: HashSet<usize>,
    subtree_ends: Vec<usize>,
    previous_sibling: Vec<Option<usize>>,
    next_sibling: Vec<Option<usize>>,
    nth_of_type: Vec<usize>,
}

impl CheckedSnapshot {
    pub(super) fn new(nodes: &[HtmlNode], checked: HashSet<usize>) -> Self {
        // The graph stores each subtree as a contiguous document-order range.
        let mut subtree_ends: Vec<_> = (1..=nodes.len()).collect();
        for node in nodes.iter().rev() {
            if let Some(parent) = node.parent_id {
                subtree_ends[parent] = subtree_ends[parent].max(subtree_ends[node.id]);
            }
        }

        let mut previous_sibling = vec![None; nodes.len()];
        let mut next_sibling = vec![None; nodes.len()];
        let mut last_child = vec![None; nodes.len()];
        let mut nth_of_type = vec![0; nodes.len()];
        let mut type_counts = HashMap::new();
        for node in nodes
            .iter()
            .filter(|node| node.kind == HtmlNodeKind::Element)
        {
            let count = type_counts
                .entry((node.parent_id, node.name.as_deref()))
                .or_insert(0);
            *count += 1;
            nth_of_type[node.id] = *count;
            if let Some(parent) = node.parent_id
                && let Some(previous) = last_child[parent].replace(node.id)
            {
                previous_sibling[node.id] = Some(previous);
                next_sibling[previous] = Some(node.id);
            }
        }

        Self {
            checked,
            subtree_ends,
            previous_sibling,
            next_sibling,
            nth_of_type,
        }
    }

    pub(super) fn nth_of_type(&self, node: &HtmlNode) -> usize {
        self.nth_of_type[node.id]
    }
}

enum Condition<'a> {
    Id(&'a str),
    Class(&'a str),
    Attribute(&'a str, Option<&'a str>),
    Root,
    FirstChild,
    LastChild,
    NthOfType(usize),
    NthChild { stride: i32, offset: i32 },
    AnyOf(Vec<Selector<'a>>),
    Not(Vec<Selector<'a>>),
    Checked,
    Has(Vec<(Combinator, Selector<'a>)>),
}

impl<'a> Selector<'a> {
    pub(crate) fn parse(input: &'a str) -> Option<Self> {
        Self::parse_with_mode(input, SelectorMode::Static)
    }

    pub(super) fn parse_snapshot(input: &'a str) -> Option<Self> {
        Self::parse_with_mode(input, SelectorMode::Snapshot)
    }

    fn parse_with_mode(mut input: &'a str, mode: SelectorMode) -> Option<Self> {
        let selector = Self::parse_prefix(&mut input, mode)?;
        input.is_empty().then_some(selector)
    }

    fn parse_prefix(input: &mut &'a str, mode: SelectorMode) -> Option<Self> {
        let mut specificity = Specificity::default();
        *input = input.trim_ascii_start();
        let head = Compound::parse(input, &mut specificity, mode)?;
        let mut tail = Vec::new();
        loop {
            let before = input.len();
            *input = input.trim_ascii_start();
            if matches!(input.as_bytes().first(), None | Some(b',' | b')')) {
                break;
            }
            let combinator = take_combinator(input, mode)
                .or_else(|| (input.len() < before).then_some(Combinator::Descendant))?;
            tail.push((combinator, Compound::parse(input, &mut specificity, mode)?));
        }
        Some(Self {
            head,
            tail,
            specificity,
        })
    }

    /// A necessary match on the rightmost compound. Functional selectors
    /// and attribute conditions still run through the ordinary matcher.
    pub(super) fn subject(&self, snapshot: bool) -> Option<Subject<'a>> {
        let compound = self
            .tail
            .last()
            .map_or(&self.head, |(_, compound)| compound);
        let mut class = None;
        let mut nth = None;
        for condition in &compound.conditions {
            match *condition {
                Condition::Id(id) => return Some(Subject::Id(id)),
                Condition::Class(name) => class = Some(name),
                Condition::NthOfType(index) if snapshot => nth = Some(index),
                _ => {}
            }
        }
        match (compound.tag, nth, class) {
            (Some(tag), Some(index), _) => Some(Subject::NthOfType(tag, index)),
            (_, _, Some(name)) => Some(Subject::Class(name)),
            (Some(tag), _, _) => Some(Subject::Tag(tag)),
            _ => None,
        }
    }

    pub(super) fn is_compound(&self) -> bool {
        self.tail.is_empty()
    }

    pub(super) fn specificity(&self) -> Specificity {
        self.specificity
    }

    pub(super) fn matches(&self, node: &HtmlNode, nodes: &[HtmlNode]) -> bool {
        self.matches_in(node, nodes, None)
    }

    pub(super) fn matches_in(
        &self,
        node: &HtmlNode,
        nodes: &[HtmlNode],
        snapshot: Option<&CheckedSnapshot>,
    ) -> bool {
        self.matches_tail(node, nodes, self.tail.len(), snapshot)
    }

    fn matches_tail(
        &self,
        node: &HtmlNode,
        nodes: &[HtmlNode],
        len: usize,
        snapshot: Option<&CheckedSnapshot>,
    ) -> bool {
        let Some((combinator, compound)) = self.tail[..len].last() else {
            return self.head.matches(node, nodes, snapshot);
        };
        if !compound.matches(node, nodes, snapshot) {
            return false;
        }

        let matches_previous = |other| self.matches_tail(other, nodes, len - 1, snapshot);
        let parent = node.parent_id.and_then(|id| nodes.get(id));
        match combinator {
            Combinator::Descendant => std::iter::successors(parent, |ancestor| {
                ancestor.parent_id.and_then(|id| nodes.get(id))
            })
            .any(matches_previous),
            Combinator::Child => parent.is_some_and(matches_previous),
            Combinator::AdjacentSibling | Combinator::FollowingSibling => {
                let Some(snapshot) = snapshot else {
                    return false;
                };
                let mut previous = snapshot.previous_sibling[node.id];
                while let Some(id) = previous {
                    if matches_previous(&nodes[id]) {
                        return true;
                    }
                    if matches!(combinator, Combinator::AdjacentSibling) {
                        break;
                    }
                    previous = snapshot.previous_sibling[id];
                }
                false
            }
        }
    }

    fn matches_relative(
        &self,
        anchor: &HtmlNode,
        combinator: Combinator,
        nodes: &[HtmlNode],
        snapshot: &CheckedSnapshot,
    ) -> bool {
        Self::matches_forward(anchor, combinator, &self.head, &self.tail, nodes, snapshot)
    }

    fn matches_forward(
        anchor: &HtmlNode,
        combinator: Combinator,
        compound: &Compound<'_>,
        tail: &[(Combinator, Compound<'_>)],
        nodes: &[HtmlNode],
        snapshot: &CheckedSnapshot,
    ) -> bool {
        let matches_next = |node: &HtmlNode| {
            compound.matches(node, nodes, Some(snapshot))
                && tail
                    .split_first()
                    .is_none_or(|((combinator, compound), rest)| {
                        Self::matches_forward(node, *combinator, compound, rest, nodes, snapshot)
                    })
        };
        match combinator {
            Combinator::Descendant | Combinator::Child => {
                let end = snapshot.subtree_ends[anchor.id];
                let is_child = |node: &HtmlNode| node.parent_id == Some(anchor.id);
                if compound
                    .conditions
                    .iter()
                    .any(|condition| matches!(condition, Condition::Checked))
                {
                    // The selected radios are usually much fewer than the anchor's descendants.
                    return snapshot.checked.iter().copied().any(|id| {
                        (anchor.id + 1..end).contains(&id)
                            && (matches!(combinator, Combinator::Descendant)
                                || is_child(&nodes[id]))
                            && matches_next(&nodes[id])
                    });
                }
                if matches!(combinator, Combinator::Descendant) {
                    return nodes[anchor.id + 1..end].iter().any(matches_next);
                }
                let mut child = anchor.id + 1;
                while child < end {
                    if matches_next(&nodes[child]) {
                        return true;
                    }
                    child = snapshot.subtree_ends[child];
                }
                false
            }
            Combinator::AdjacentSibling | Combinator::FollowingSibling => {
                let mut next = snapshot.next_sibling[anchor.id];
                while let Some(id) = next {
                    if matches_next(&nodes[id]) {
                        return true;
                    }
                    if matches!(combinator, Combinator::AdjacentSibling) {
                        break;
                    }
                    next = snapshot.next_sibling[id];
                }
                false
            }
        }
    }
}

impl<'a> Compound<'a> {
    fn parse(
        input: &mut &'a str,
        specificity: &mut Specificity,
        mode: SelectorMode,
    ) -> Option<Self> {
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
                    pseudo_class(input, specificity, mode)?
                }
                _ => break,
            };
            conditions.push(condition);
        }
        (input.len() != before).then_some(Self { tag, conditions })
    }

    fn matches(
        &self,
        node: &HtmlNode,
        nodes: &[HtmlNode],
        snapshot: Option<&CheckedSnapshot>,
    ) -> bool {
        node.kind == HtmlNodeKind::Element
            && self.tag.is_none_or(|expected| {
                node.name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(expected))
            })
            && self
                .conditions
                .iter()
                .all(|condition| condition.matches(node, nodes, snapshot))
    }
}

impl Condition<'_> {
    fn matches(
        &self,
        node: &HtmlNode,
        nodes: &[HtmlNode],
        snapshot: Option<&CheckedSnapshot>,
    ) -> bool {
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
            Self::NthChild { stride, offset } => {
                let Some(index) = siblings().position(|other| other.id == node.id) else {
                    return false;
                };
                let difference = index as i64 + 1 - i64::from(*offset);
                let stride = i64::from(*stride);
                if stride == 0 {
                    difference == 0
                } else {
                    difference % stride == 0 && difference / stride >= 0
                }
            }
            Self::NthOfType(index) => match snapshot {
                Some(snapshot) => snapshot.nth_of_type(node) == *index,
                None => index.checked_sub(1).is_some_and(|index| {
                    siblings()
                        .filter(|other| other.name == node.name)
                        .nth(index)
                        .is_some_and(|other| other.id == node.id)
                }),
            },
            Self::Checked => snapshot.is_some_and(|snapshot| snapshot.checked.contains(&node.id)),
            Self::Has(selectors) => snapshot.is_some_and(|snapshot| {
                selectors.iter().any(|(combinator, selector)| {
                    selector.matches_relative(node, *combinator, nodes, snapshot)
                })
            }),
            Self::AnyOf(selectors) => selectors
                .iter()
                .any(|selector| selector.matches_in(node, nodes, snapshot)),
            Self::Not(selectors) => !selectors
                .iter()
                .any(|selector| selector.matches_in(node, nodes, snapshot)),
        }
    }
}

fn pseudo_class<'a>(
    input: &mut &'a str,
    specificity: &mut Specificity,
    mode: SelectorMode,
) -> Option<Condition<'a>> {
    let name = ident(input)?;
    let condition = match name {
        "root" => Condition::Root,
        "first-child" => Condition::FirstChild,
        "last-child" => Condition::LastChild,
        "nth-child" if take(input, "(") => {
            let (value, rest) = input.split_once(')')?;
            *input = rest;
            let (stride, offset) = nth_child(value.trim_ascii())?;
            Condition::NthChild { stride, offset }
        }
        "nth-of-type" if take(input, "(") => {
            let (value, rest) = input.split_once(')')?;
            *input = rest;
            Condition::NthOfType(value.trim_ascii().parse().ok()?)
        }
        "checked" if !matches!(mode, SelectorMode::Static) => Condition::Checked,
        "has" if matches!(mode, SelectorMode::Snapshot) && take(input, "(") => {
            let mut selectors = Vec::new();
            loop {
                *input = input.trim_ascii_start();
                let combinator = take_combinator(input, SelectorMode::Relative)
                    .unwrap_or(Combinator::Descendant);
                selectors.push((
                    combinator,
                    Selector::parse_prefix(input, SelectorMode::Relative)?,
                ));
                if !take(input, ",") {
                    break;
                }
            }
            if !take(input, ")") {
                return None;
            }
            add_specificity(
                specificity,
                selectors
                    .iter()
                    .map(|(_, selector)| selector)
                    .map(Selector::specificity)
                    .max()?,
            );
            return Some(Condition::Has(selectors));
        }
        "is" | "where" | "not" if take(input, "(") => {
            let mut selectors = Vec::new();
            loop {
                selectors.push(Selector::parse_prefix(input, mode)?);
                if !take(input, ",") {
                    break;
                }
            }
            if !take(input, ")") {
                return None;
            }
            if name != "where" {
                let max = selectors.iter().map(Selector::specificity).max()?;
                add_specificity(specificity, max);
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

fn add_specificity(specificity: &mut Specificity, additional: Specificity) {
    specificity.0 += additional.0;
    specificity.1 += additional.1;
    specificity.2 += additional.2;
}

fn take_combinator(input: &mut &str, mode: SelectorMode) -> Option<Combinator> {
    let combinator = match input.as_bytes().first()? {
        b'>' => Combinator::Child,
        b'+' if !matches!(mode, SelectorMode::Static) => Combinator::AdjacentSibling,
        b'~' if !matches!(mode, SelectorMode::Static) => Combinator::FollowingSibling,
        _ => return None,
    };
    *input = input[1..].trim_ascii_start();
    Some(combinator)
}

fn nth_child(value: &str) -> Option<(i32, i32)> {
    match value {
        "even" => return Some((2, 0)),
        "odd" => return Some((2, 1)),
        _ => {}
    }
    if let Some((stride, offset)) = value.split_once('n') {
        let stride = match stride {
            "" | "+" => 1,
            "-" => -1,
            stride => stride.parse().ok()?,
        };
        let offset = offset.trim_ascii_start();
        let offset = if offset.is_empty() {
            0
        } else {
            let sign = match offset.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let digits = offset[1..].trim_ascii_start();
            if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            sign * digits.parse::<i32>().ok()?
        };
        Some((stride, offset))
    } else {
        Some((0, value.parse().ok()?))
    }
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
