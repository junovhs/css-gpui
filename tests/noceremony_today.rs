use css_gpui::{SceneSourceGraph, diagnostics, emitter, extractor, layout, resolver};

const HTML: &str = include_str!("../fixtures/noceremony-today/source.html");
const CSS: &str = include_str!("../fixtures/noceremony-today/source.css");

#[test]
fn today_panel_preserves_task_order_and_reports_grid_losses() {
    let graph = SceneSourceGraph::from_strings("noceremony-today", HTML, CSS).unwrap();
    let losses = diagnostics::analyze(&graph);

    for (kind, declaration) in [
        (diagnostics::DiagnosticKind::UnsupportedValue, "display"),
        (
            diagnostics::DiagnosticKind::UnsupportedProperty,
            "grid-template-columns",
        ),
    ] {
        assert!(
            losses.iter().any(|loss| loss.kind == kind
                && loss.selector.as_deref() == Some(".task-row")
                && loss.message.contains(declaration)),
            "expected a source-linked loss for .task-row {declaration}: {losses:?}"
        );
    }

    let resolved = resolver::resolve_scene(&graph).unwrap();
    let scene = extractor::extract_ir(&layout::compute_layout(&resolved), &graph).unwrap();
    let reference = emitter::emit(&scene);

    let mut remaining = reference.html.as_str();
    for text in [
        "UP NEXT",
        "Review the campaign proof",
        "Send the revised brief",
        "Pick Friday’s launch image",
    ] {
        let (_, after) = remaining
            .split_once(text)
            .unwrap_or_else(|| panic!("missing or out-of-order task text: {text}"));
        remaining = after;
    }
    assert!(reference.css.contains("flex-direction: column"));
}
