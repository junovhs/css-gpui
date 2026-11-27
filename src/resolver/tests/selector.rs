use crate::HtmlNode;
use crate::HtmlNodeKind;
use crate::resolver::selector::{Specificity, selector_matches, specificity};

fn el(name: &str, class: &str) -> HtmlNode {
    let mut attributes = Vec::new();
    if !class.is_empty() {
        attributes.push(("class".to_owned(), class.to_owned()));
    }
    HtmlNode {
        id: 0,
        parent_id: None,
        kind: HtmlNodeKind::Element,
        name: Some(name.to_owned()),
        text: None,
        attributes,
        dom_path: "0/0".to_owned(),
        document_id: 0,
        start_tag_range: None,
    }
}

// --- Type selectors ---

#[test]
fn type_selector_matches_same_tag() {
    let node = el("div", "");
    assert!(selector_matches("div", &node));
}

#[test]
fn type_selector_rejects_different_tag() {
    let node = el("span", "");
    assert!(!selector_matches("div", &node));
}

#[test]
fn body_selector_matches_body_element() {
    let node = el("body", "");
    assert!(selector_matches("body", &node));
}

// --- Class selectors ---

#[test]
fn class_selector_matches_node_with_that_class() {
    let node = el("div", "card");
    assert!(selector_matches(".card", &node));
}

#[test]
fn class_selector_rejects_node_without_class() {
    let node = el("div", "");
    assert!(!selector_matches(".card", &node));
}

#[test]
fn class_selector_matches_one_of_multiple_classes() {
    let node = el("div", "card highlight");
    assert!(selector_matches(".card", &node));
    assert!(selector_matches(".highlight", &node));
}

// --- Compound selectors ---

#[test]
fn compound_matches_node_with_correct_type_and_class() {
    let node = el("button", "primary");
    assert!(selector_matches("button.primary", &node));
}

#[test]
fn compound_rejects_wrong_type() {
    let node = el("div", "primary");
    assert!(!selector_matches("button.primary", &node));
}

#[test]
fn compound_rejects_missing_class() {
    let node = el("button", "secondary");
    assert!(!selector_matches("button.primary", &node));
}

#[test]
fn compound_two_classes_both_required() {
    let node_both = el("div", "button secondary");
    let node_one = el("div", "button");
    assert!(selector_matches(".button.secondary", &node_both));
    assert!(!selector_matches(".button.secondary", &node_one));
}

// --- Specificity ---

#[test]
fn type_selector_has_specificity_zero_one() {
    assert_eq!(specificity("div"), Specificity(0, 0, 1));
}

#[test]
fn class_selector_has_specificity_one_zero() {
    assert_eq!(specificity(".card"), Specificity(0, 1, 0));
}

#[test]
fn compound_two_classes_one_type_has_specificity_two_one() {
    assert_eq!(specificity("button.primary"), Specificity(0, 1, 1));
}

#[test]
fn higher_specificity_compares_greater() {
    assert!(Specificity(0, 1, 0) > Specificity(0, 0, 1));
    assert!(Specificity(0, 2, 0) > Specificity(0, 1, 1));
    assert!(Specificity(1, 0, 0) > Specificity(0, 10, 10));
}

#[test]
fn neon_selectors_match_ancestry_attributes_and_id_precedence() {
    let html = r#"<html><body><aside id="menu" class="open"><section><input type=range><section><input type=range></section></section></aside></body></html>"#;
    let css = r#"
        :root { color: #fff; }
        #menu.open > section > input[type="range"] { width: 275px; }
        body aside.open section input { width: 200px; }
    "#;
    let graph = crate::SceneSourceGraph::from_strings("selectors", html, css).unwrap();
    let scene = crate::resolver::resolve_scene(&graph).unwrap();
    let mut inputs = scene
        .nodes
        .iter()
        .filter(|node| node.node.name.as_deref() == Some("input"));

    let direct = inputs.next().unwrap();
    assert_eq!(direct.style.width, Some(275.0));
    assert_eq!(direct.style.color.as_deref(), Some("#fff"));
    assert_eq!(inputs.next().unwrap().style.width, Some(200.0));
}
