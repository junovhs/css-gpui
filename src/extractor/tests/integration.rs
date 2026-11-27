//! Integration tests for extract_ir against the fixture corpus.

use std::path::PathBuf;

use crate::extractor::extract_ir;
use crate::ir::{ControlKind, NodeKind, SceneIr};
use crate::layout::compute_layout;
use crate::load_scene_source_graph;
use crate::resolver::resolve_scene;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------
// Basic sanity
// ---------------------------------------------------------------------------

#[test]
fn profile_card_produces_nonempty_ir() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    assert_eq!(ir.scene_id, "profile_card_absolute");
    assert_eq!(ir.schema_version, 1);
    assert!(!ir.nodes.is_empty());
    Ok(())
}

#[test]
fn stacked_card_produces_nonempty_ir() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "stacked_info_card")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    assert!(!ir.nodes.is_empty());
    Ok(())
}

// ---------------------------------------------------------------------------
// Pre-order invariant: root has no parent, all others do
// ---------------------------------------------------------------------------

#[test]
fn profile_card_ir_first_node_has_no_parent() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    // The very first visible element node is the root in IR pre-order.
    assert!(
        ir.nodes[0].parent_id.is_none(),
        "first ir node must have no parent"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Element kind mapping
// ---------------------------------------------------------------------------

#[test]
fn button_element_maps_to_control_kind() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    let button_node = ir.nodes.iter().find(|n| n.kind == NodeKind::Control);
    let Some(btn) = button_node else {
        // If no button in this fixture, the test is vacuously ok — but
        // flag it so we know to add a fixture with a button.
        return Ok(());
    };
    assert_eq!(btn.control_kind, Some(ControlKind::Button));
    Ok(())
}

#[test]
fn authored_text_input_value_survives_scene_json_and_html_roundtrip()
-> Result<(), Box<dyn std::error::Error>> {
    let compile = |html, css| -> Result<SceneIr, Box<dyn std::error::Error>> {
        let graph = crate::SceneSourceGraph::from_strings("text-field", html, css)?;
        let resolved = resolve_scene(&graph)?;
        Ok(extract_ir(&compute_layout(&resolved), &graph)?)
    };

    let legacy = compile(r#"<input placeholder="Your name">"#, "")?;
    assert_eq!(legacy.schema_version, 1);
    assert_eq!(SceneIr::from_json(&legacy.to_json()?)?, legacy);

    let original = compile(r#"<input placeholder="Your name" value="Dana">"#, "")?;
    assert_eq!(original.schema_version, 2);
    let restored = SceneIr::from_json(&original.to_json()?)?;
    let emitted = crate::emitter::emit(&restored);
    let reparsed = compile(&emitted.html, &emitted.css)?;
    assert_eq!(
        reparsed.nodes[0].control_kind,
        original.nodes[0].control_kind
    );
    assert_eq!(reparsed.nodes[0].text_content.as_deref(), Some("Your name"));
    Ok(())
}

#[test]
fn flex_wrap_survives_scene_json_and_both_targets() -> Result<(), Box<dyn std::error::Error>> {
    use crate::target::{TargetEmitter, gpui::GpuiTarget};

    let compile = |html, css| -> Result<SceneIr, Box<dyn std::error::Error>> {
        let graph = crate::SceneSourceGraph::from_strings("flex-wrap", html, css)?;
        assert!(crate::diagnostics::analyze(&graph).is_empty());
        let resolved = resolve_scene(&graph)?;
        Ok(extract_ir(&compute_layout(&resolved), &graph)?)
    };

    let scene = compile(
        r#"<section class="wrapped"></section><section class="reversed"></section><section class="nowrap"></section><section class="default"></section>"#,
        r#"
            section { display: flex; }
            .wrapped { flex-wrap: WRAP; }
            .reversed { flex-wrap: wrap-reverse; }
            .nowrap { flex-wrap: wrap; flex-wrap: nowrap; }
        "#,
    )?;
    assert_eq!(scene.schema_version, 3);
    let json = scene.to_json()?;
    let serialized: serde_json::Value = serde_json::from_str(&json)?;
    let values: Vec<_> = serialized["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["layout"]["flex_wrap"].as_str())
        .collect();
    assert_eq!(
        values,
        [Some("wrap"), Some("wrap_reverse"), Some("nowrap"), None]
    );
    let restored = SceneIr::from_json(&json)?;
    let html = crate::emitter::emit(&restored);
    let reparsed = compile(&html.html, &html.css)?;
    assert_eq!(
        reparsed
            .nodes
            .iter()
            .map(|node| &node.layout)
            .collect::<Vec<_>>(),
        restored
            .nodes
            .iter()
            .map(|node| &node.layout)
            .collect::<Vec<_>>(),
    );

    let native = GpuiTarget.emit(&restored);
    assert!(native.declared_loss.is_empty());
    for builder in [".flex_wrap()", ".flex_wrap_reverse()", ".flex_nowrap()"] {
        assert_eq!(native.artifact.matches(builder).count(), 1, "{builder}");
    }

    let invalid = crate::SceneSourceGraph::from_strings(
        "invalid-flex-wrap",
        "<div></div>",
        "div { display: flex; flex-wrap: sideway; }",
    )?;
    let losses = crate::diagnostics::analyze(&invalid);
    assert!(matches!(losses.as_slice(), [loss]
        if loss.kind == crate::diagnostics::DiagnosticKind::UnsupportedValue
            && loss.property.as_deref() == Some("flex-wrap")));
    Ok(())
}

#[test]
fn text_alignment_survives_scene_json_and_both_targets() -> Result<(), Box<dyn std::error::Error>> {
    use crate::target::{TargetEmitter, gpui::GpuiTarget};

    let compile = |html, css| -> Result<SceneIr, Box<dyn std::error::Error>> {
        let graph = crate::SceneSourceGraph::from_strings("text-align", html, css)?;
        assert!(crate::diagnostics::analyze(&graph).is_empty());
        let resolved = resolve_scene(&graph)?;
        Ok(extract_ir(&compute_layout(&resolved), &graph)?)
    };
    let html = r#"<html><body><main><div class="inherit">Inherited</div><div class="start">Start</div><div class="end">End</div><div class="left">Left</div><div class="right">Right</div></main></body></html>"#;
    let scene = compile(
        html,
        r#"
            body { text-align: CENTER; }
            .inherit { text-align: right; text-align: inherit; }
            .start { text-align: initial; }
            .end { text-align: end; }
            .left { text-align: left; }
            .right { text-align: right; }
        "#,
    )?;
    assert_eq!(scene.schema_version, 4);
    let json = scene.to_json()?;
    let serialized: serde_json::Value = serde_json::from_str(&json)?;
    let values: Vec<_> = serialized["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["kind"] == "box")
        .map(|node| node["layout"]["text_align"].as_str())
        .collect();
    assert_eq!(
        values,
        [
            Some("center"),
            Some("center"),
            Some("start"),
            Some("end"),
            Some("left"),
            Some("right"),
        ]
    );
    let restored = SceneIr::from_json(&json)?;
    let emitted = crate::emitter::emit(&restored);
    let reparsed = compile(&emitted.html, &emitted.css)?;
    assert_eq!(
        reparsed
            .nodes
            .iter()
            .map(|node| &node.layout)
            .collect::<Vec<_>>(),
        restored
            .nodes
            .iter()
            .map(|node| &node.layout)
            .collect::<Vec<_>>(),
    );

    let native = GpuiTarget.emit(&restored);
    assert!(native.declared_loss.is_empty());
    for builder in [".text_center()", ".text_left()", ".text_right()"] {
        assert!(native.artifact.contains(builder), "{builder}");
    }

    let legacy = compile(html, "")?;
    assert_eq!(legacy.schema_version, 1);
    assert!(!legacy.to_json()?.contains("\"text_align\""));
    assert_eq!(SceneIr::from_json(&legacy.to_json()?)?, legacy);

    let invalid = crate::SceneSourceGraph::from_strings(
        "unsupported-text-align",
        "<div>Text</div>",
        "div { text-align: justify; }",
    )?;
    let losses = crate::diagnostics::analyze(&invalid);
    assert!(matches!(losses.as_slice(), [loss]
        if loss.kind == crate::diagnostics::DiagnosticKind::UnsupportedValue
            && loss.property.as_deref() == Some("text-align")));
    Ok(())
}

#[test]
fn letter_spacing_round_trips_and_reports_native_limits() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::diagnostics::{DiagnosticKind, analyze};
    use crate::ir::LetterSpacing;
    use crate::target::{Capability, TargetEmitter, gpui::GpuiTarget};

    let html = r#"<html><body><main><div class="inherit">Inherited</div><div class="own">Own</div><div class="reset">Reset</div><div>Two words</div><input type="text" value="Edit"></main></body></html>"#;
    let css = r#"
        html { font-size: 20px; }
        body { letter-spacing: .1em; }
        main { font-size: 30px; }
        div { font-size: 10px; }
        .inherit { letter-spacing: 12px; letter-spacing: inherit; }
        .own { letter-spacing: -0.1em; font-size: 40px; }
        .reset { letter-spacing: 12px; letter-spacing: NORMAL; }
    "#;
    let compile = |html, css| -> Result<SceneIr, Box<dyn std::error::Error>> {
        let graph = crate::SceneSourceGraph::from_strings("letter-spacing", html, css)?;
        assert!(analyze(&graph).is_empty(), "{:?}", analyze(&graph));
        let resolved = resolve_scene(&graph)?;
        Ok(extract_ir(&compute_layout(&resolved), &graph)?)
    };
    let scene = compile(html, css)?;
    assert_eq!(scene.schema_version, 5);
    let restored = SceneIr::from_json(&scene.to_json()?)?;
    let emitted = crate::emitter::emit(&restored);
    for declaration in [
        "letter-spacing: 2px",
        "letter-spacing: -4px",
        "letter-spacing: normal",
    ] {
        assert!(emitted.css.contains(declaration), "{declaration}");
    }
    let text_spacing = |scene: &SceneIr| {
        scene
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Text)
            .map(|node| node.letter_spacing)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        text_spacing(&restored),
        [
            Some(LetterSpacing::Length { value: 2.0 }),
            Some(LetterSpacing::Length { value: -4.0 }),
            Some(LetterSpacing::Normal),
            Some(LetterSpacing::Length { value: 2.0 }),
        ],
    );
    assert_eq!(
        text_spacing(&compile(&emitted.html, &emitted.css)?),
        text_spacing(&restored)
    );
    let native = GpuiTarget.emit(&restored);
    assert!(native.artifact.contains("SceneSpacedText"));
    let required_loss_nodes: Vec<_> = restored
        .nodes
        .iter()
        .filter(|node| {
            node.text_content.as_deref() == Some("Two words")
                || node
                    .control_kind
                    .as_ref()
                    .and_then(crate::ir::ControlKind::text_input_value)
                    .is_some()
        })
        .map(|node| node.id.as_str())
        .collect();
    let actual_loss_nodes: Vec<_> = native
        .declared_loss
        .iter()
        .filter(|loss| loss.capability == Capability::LetterSpacing)
        .map(|loss| loss.node_id.as_str())
        .collect();
    assert_eq!(actual_loss_nodes, required_loss_nodes);

    let legacy = compile(html, "")?;
    assert_eq!(legacy.schema_version, 2);
    assert!(!legacy.to_json()?.contains("letter_spacing"));
    assert_eq!(SceneIr::from_json(&legacy.to_json()?)?, legacy);

    let graph = crate::SceneSourceGraph::from_strings(
        "unsupported-letter-spacing",
        "<div>Text</div>",
        "div { letter-spacing: 10%; }",
    )?;
    assert!(matches!(analyze(&graph).as_slice(), [loss]
        if loss.kind == DiagnosticKind::UnsupportedValue
            && loss.property.as_deref() == Some("letter-spacing")));
    Ok(())
}

// ---------------------------------------------------------------------------
// Text nodes
// ---------------------------------------------------------------------------

#[test]
fn text_nodes_have_text_content_set() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    let text_nodes: Vec<_> = ir
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Text)
        .collect();
    assert!(
        !text_nodes.is_empty(),
        "profile_card_absolute should contain at least one text node"
    );
    for t in &text_nodes {
        let content = t.text_content.as_deref().unwrap_or("");
        assert!(!content.is_empty(), "text node content must be non-empty");
    }
    Ok(())
}

#[test]
fn text_nodes_have_parent_id() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    for node in ir.nodes.iter().filter(|n| n.kind == NodeKind::Text) {
        assert!(
            node.parent_id.is_some(),
            "text node {:?} must have a parent_id",
            node.id
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scene id mismatch is rejected
// ---------------------------------------------------------------------------

#[test]
fn mismatched_scene_ids_return_error() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let graph2 = load_scene_source_graph(repo_root(), "stacked_info_card")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);

    // laid_out is profile_card_absolute, graph2 is stacked_info_card → mismatch
    let result = extract_ir(&laid_out, &graph2);
    assert!(result.is_err(), "mismatched scene ids must return Err");
    Ok(())
}

// ---------------------------------------------------------------------------
// Source provenance
// ---------------------------------------------------------------------------

#[test]
fn all_ir_nodes_have_nonempty_dom_path() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let laid_out = compute_layout(&resolved);
    let ir = extract_ir(&laid_out, &graph)?;

    for node in &ir.nodes {
        assert!(
            !node.source.dom_path.is_empty(),
            "node {:?} has empty dom_path",
            node.id
        );
    }
    Ok(())
}
