use css_gpui::{SceneSourceGraph, diagnostics, emitter, extractor, ir::SceneIr, layout, resolver};

const HTML: &str = include_str!("../fixtures/toolbar/source.html");
const CSS: &str = include_str!("../fixtures/toolbar/source.css");

#[test]
fn toolbar_spacing_and_intrinsic_width_survive_conversion() {
    let graph = SceneSourceGraph::from_strings("toolbar", HTML, CSS).unwrap();
    assert!(diagnostics::analyze(&graph).is_empty());
    let resolved = resolver::resolve_scene(&graph).unwrap();
    let scene = extractor::extract_ir(&layout::compute_layout(&resolved), &graph).unwrap();
    let json = serde_json::to_value(&scene).unwrap();
    assert_eq!(
        json["nodes"][0]["layout"]["justify_content"],
        "space_between"
    );
    let round_trip: SceneIr = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip.nodes[0].layout.height, Some(56.0));
    assert!(
        round_trip
            .nodes
            .iter()
            .all(|node| node.layout.width.is_none())
    );
    let reference = emitter::emit(&round_trip);
    assert!(reference.css.contains("justify-content: space-between"));
    assert!(!reference.css.contains("width:"));
}
