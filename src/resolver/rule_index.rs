use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

use crate::{CssRule, HtmlNode, SceneSourceGraph};

use super::{
    MatchedRule,
    selector::{CheckedSnapshot, Selector, Subject},
};

type Candidate<'a> = (&'a CssRule, Selector<'a>);

/// Index necessary subject conditions. Matching still checks the complete
/// selector, and a selector list competes at its highest matching specificity.
pub(super) struct RuleIndex<'a> {
    ids: HashMap<&'a str, Vec<Candidate<'a>>>,
    classes: HashMap<&'a str, Vec<Candidate<'a>>>,
    tags: HashMap<Cow<'a, str>, Vec<Candidate<'a>>>,
    types: HashMap<(Cow<'a, str>, usize), Vec<Candidate<'a>>>,
    fallback: Vec<Candidate<'a>>,
}

impl<'a> RuleIndex<'a> {
    pub(super) fn parse(graph: &'a SceneSourceGraph, pseudo: Option<&str>, snapshot: bool) -> Self {
        let mut index = Self {
            ids: HashMap::new(),
            classes: HashMap::new(),
            tags: HashMap::new(),
            types: HashMap::new(),
            fallback: Vec::new(),
        };
        for rule in &graph.css_rules {
            for text in &rule.selectors {
                let origin = if let Some(pseudo) = pseudo {
                    let Some(origin) = text.strip_suffix(pseudo).and_then(|origin| {
                        origin
                            .strip_suffix("::")
                            .or_else(|| snapshot.then(|| origin.strip_suffix(":")).flatten())
                    }) else {
                        continue;
                    };
                    origin
                } else {
                    text
                };
                let selector = if snapshot {
                    Selector::parse_snapshot(origin)
                } else {
                    Selector::parse(origin)
                };
                let Some(selector) = selector else { continue };
                let bucket = match selector.subject(snapshot) {
                    Some(Subject::Id(id)) => index.ids.entry(id).or_default(),
                    Some(Subject::Class(class)) => index.classes.entry(class).or_default(),
                    Some(Subject::Tag(tag)) => index.tags.entry(canonical_tag(tag)).or_default(),
                    Some(Subject::NthOfType(tag, nth)) => {
                        index.types.entry((canonical_tag(tag), nth)).or_default()
                    }
                    None => &mut index.fallback,
                };
                bucket.push((rule, selector));
            }
        }
        index
    }

    pub(super) fn matches(
        &self,
        node: &HtmlNode,
        nodes: &[HtmlNode],
        snapshot: Option<&CheckedSnapshot>,
    ) -> Vec<MatchedRule<'a>> {
        let mut matched = BTreeMap::new();
        let mut check = |candidates: &[Candidate<'a>]| {
            for (rule, selector) in candidates {
                if selector.matches_in(node, nodes, snapshot) {
                    let specificity = selector.specificity();
                    let entry = matched.entry(rule.id).or_insert((specificity, *rule, None));
                    entry.0 = entry.0.max(specificity);
                }
            }
        };
        check(&self.fallback);
        if let Some(tag) = node.name.as_deref().map(canonical_tag) {
            if let Some(candidates) = self.tags.get(tag.as_ref()) {
                check(candidates);
            }
            if let Some(candidates) =
                snapshot.and_then(|snapshot| self.types.get(&(tag, snapshot.nth_of_type(node))))
            {
                check(candidates);
            }
        }
        let attribute = |name| {
            node.attributes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        if let Some(candidates) = attribute("id").and_then(|id| self.ids.get(id)) {
            check(candidates);
        }
        if let Some(classes) = attribute("class") {
            for class in classes.split_ascii_whitespace() {
                if let Some(candidates) = self.classes.get(class) {
                    check(candidates);
                }
            }
        }
        matched.into_values().collect()
    }
}

fn canonical_tag(tag: &str) -> Cow<'_, str> {
    if tag.bytes().any(|byte| byte.is_ascii_uppercase()) {
        tag.to_ascii_lowercase().into()
    } else {
        tag.into()
    }
}
