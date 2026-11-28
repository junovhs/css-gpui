use super::*;
use crate::FixtureManifest;

#[test]
fn bare_html_attributes_keep_following_attribute_values() {
    let graph = SceneSourceGraph::from_strings(
        "bare-attributes",
        r#"<button disabled data-action="open" autofocus>Open</button>"#,
        "",
    )
    .unwrap();
    let button = graph
        .html_nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("button"))
        .unwrap();
    assert_eq!(
        button.attributes,
        [
            ("disabled".to_owned(), String::new()),
            ("data-action".to_owned(), "open".to_owned()),
            ("autofocus".to_owned(), String::new()),
        ]
    );
}
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn loads_fixture_manifest() {
    let manifest_path = repo_root()
        .join("fixtures")
        .join("v0.1")
        .join("manifest.toml");
    let manifest = FixtureManifest::load(manifest_path).expect("manifest should load");

    assert_eq!(manifest.corpus, "v0.1");
    assert_eq!(manifest.scenes.len(), 11);
    assert_eq!(
        manifest
            .scenes
            .iter()
            .filter(|scene| scene.tags.iter().any(|tag| tag == "browser"))
            .count(),
        9
    );
    assert!(
        manifest
            .scenes
            .iter()
            .any(|scene| scene.id == "profile_card_absolute")
    );
}

#[test]
fn builds_source_graph_for_profile_card() {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")
        .expect("fixture source graph should load");

    assert_eq!(graph.scene_id, "profile_card_absolute");
    assert!(
        graph
            .html_nodes
            .iter()
            .any(|node| node.name.as_deref() == Some("button"))
    );
    assert!(
        graph
            .html_nodes
            .iter()
            .any(|node| node.text.as_deref() == Some("Ava Martinez"))
    );
    assert!(graph.css_rules.iter().any(|rule| {
        rule.selectors
            .iter()
            .any(|selector| selector == ".profile-card")
    }));
}

#[test]
fn returns_error_for_unknown_scene() {
    let error = load_scene_source_graph(repo_root(), "missing_scene")
        .expect_err("unknown scenes should fail");

    let message = error.to_string();
    assert!(message.contains("missing_scene"));
}

#[test]
fn matches_media_width_and_keeps_unevaluated_at_rule_locations() {
    let css = "@media (max-width: 480px) { .card { width: 100px; } }
@keyframes fade { to { opacity: 0; } }
@media (min-width: 500px) and (max-height: 480px) { .card { height: 100px; } }";
    let small =
        SceneSourceGraph::from_strings_at_viewport("media", "<div></div>", css, 390.0).unwrap();
    let large =
        SceneSourceGraph::from_strings_at_viewport("media", "<div></div>", css, 1100.0).unwrap();

    assert_eq!(small.css_rules[0].selectors, [".card"]);
    assert_eq!(large.css_rules.len(), 2);
    let unsupported = &large.css_rules[0];
    assert_eq!(unsupported.selectors, ["@keyframes fade"]);
    assert_eq!((unsupported.span.line, unsupported.span.column), (2, 1));
    for graph in [&small, &large] {
        let height_query = graph.css_rules.last().unwrap();
        assert_eq!(
            height_query.selectors,
            ["@media (min-width: 500px) and (max-height: 480px)"]
        );
        assert_eq!((height_query.span.line, height_query.span.column), (3, 1));
    }
}

#[test]
fn parses_original_neon_glass_html_and_nested_css() {
    let html = include_str!("../../fixtures/neon-glass/source.html");
    let graph = SceneSourceGraph::from_strings_at_viewport("neon-glass", html, "", 390.0).unwrap();

    let search_icon = graph
        .html_nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("svg"))
        .unwrap();
    assert!(
        search_icon
            .attributes
            .iter()
            .any(|(name, value)| name == "viewBox" && value == "0 0 24 24")
    );
    let nested = graph
        .css_rules
        .iter()
        .find(|rule| rule.selectors == ["#menu label > *"])
        .unwrap();
    assert_eq!(nested.declarations[0].property, "grid-area");
    assert_eq!(nested.document_id, graph.html.id);
    assert!(html[nested.span.start..nested.span.end].starts_with("& > *"));
    assert!(graph.css_rules.iter().any(|rule| {
        rule.selectors == ["#app > footer"]
            && rule
                .declarations
                .iter()
                .any(|d| d.property == "gap" && d.value == "1em")
    }));
}

#[test]
fn css_comments_do_not_corrupt_selectors_or_source_offsets() {
    let source = "/* thème { } */\n.menu/**/.open, /* } */ #menu {\n  /* ;{} */ width: 275px;\n  font-family: '/* literal */';\n}";
    let rules = css::parse_css_region(1, source, 0..source.len(), None)
        .expect("commented rule should parse");
    let rule = &rules[0];

    assert_eq!(rule.selectors, [".menu.open", "#menu"]);
    assert_eq!((rule.span.line, rule.span.column), (2, 1));
    assert_eq!(rule.declarations[0].property, "width");
    assert_eq!(rule.declarations[0].value, "275px");
    assert_eq!(
        &source[rule.declarations[0].span.start..rule.declarations[0].span.end],
        "width: 275px"
    );
    assert_eq!(rule.declarations[1].value, "'/* literal */'");
}
